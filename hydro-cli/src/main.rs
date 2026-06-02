//! hydro-cli — Thin JSON in/out shell for the hydraulic optimization engine.
//!
//! Reads a `DesignRequest` JSON from `--input <path>` or stdin, runs the
//! optimizer (or validates only), and emits a `DesignResult` JSON on stdout
//! or to `--output <path>`.
//!
//! # Exit codes (REQ-003)
//!
//! | Code | Meaning                                                    |
//! |------|------------------------------------------------------------|
//! | 0    | Success                                                    |
//! | 1    | Validation error (bad JSON, missing required field, etc.)  |
//! | 2    | No feasible solution found                                 |
//! | 3    | No norm-compliant solution (strict_norm_compliance gate)   |
//! | 4    | Internal engine error                                      |
//!
//! # Flags (REQ-006)
//!
//! | Flag              | Type              | Behaviour                             |
//! |-------------------|-------------------|---------------------------------------|
//! | `--input <path>`  | Optional PathBuf  | Read DesignRequest from file (stdin if absent) |
//! | `--output <path>` | Optional PathBuf  | Write DesignResult to file (stdout if absent)  |
//! | `--seed <u64>`    | Optional u64      | Override seed before passing to run() |
//! | `--pretty`        | Flag              | Pretty-print JSON output              |
//! | `--validate-only` | Flag              | Run validate_request only; skip optimizer (REQ-006, WU-7 notes) |
//!
//! # WU-6 / WU-7 implementation notes
//!
//! `--validate-only` is fully wired here: routes to `validate_request()` only
//! (no TerrainModel, no GeneticOptimizer, no solver instantiation).
//!
//! WU-7 adds minimal status JSON to stdout on both the Ok and Err paths:
//! - Ok:  `{"status":"valid","project_type":"<type>"}`
//! - Err: `{"status":"invalid","error":"<message>"}`
//!
//! Wall time for the validate-only path is < 50 ms (design §7 target) and
//! < 100 ms (spec REQ-007 ceiling). Both paths are asserted in
//! `tests/validate_only.rs`.
//!
//! `audit_hash` in the DesignResult is intentionally empty string until WU-8.

use std::io::{self, BufReader, Read, Write};
use std::path::PathBuf;
use std::process;

use clap::Parser;

use hydro_cli::{run, validate_request, CliError};
use hydro_types::request::DesignRequest;

// ── CLI argument schema ───────────────────────────────────────────────────────

/// Hydraulic optimization engine — JSON in / JSON out (REQ-006).
///
/// Reads a DesignRequest JSON, runs the NSGA-III genetic optimizer, and emits
/// a DesignResult JSON. Exit codes: 0 success | 1 validation | 2 no solution |
/// 3 norm failure | 4 internal error.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Path to the DesignRequest JSON input file.
    ///
    /// If absent, reads from stdin.
    #[arg(short = 'i', long)]
    input: Option<PathBuf>,

    /// Path to write the DesignResult JSON output file.
    ///
    /// If absent, writes to stdout.
    #[arg(short = 'o', long)]
    output: Option<PathBuf>,

    /// Override the seed in DesignRequest before running the optimizer.
    ///
    /// When set, this value supersedes `DesignRequest.seed`.
    /// Passed directly to `run()` as `seed_override`.
    #[arg(long)]
    seed: Option<u64>,

    /// Pretty-print the JSON output (indent with 2 spaces).
    ///
    /// Default: compact single-line output.
    #[arg(long)]
    pretty: bool,

    /// Run validate_request only; do not invoke the optimizer.
    ///
    /// Exits 0 on Ok(()), 1 on Err(ValidationError). Completes in < 50 ms
    /// (no GA overhead). WU-7 may add timing diagnostics.
    #[arg(long = "validate-only")]
    validate_only: bool,
}

// ── I/O helpers ───────────────────────────────────────────────────────────────

/// Read all bytes from a file path or stdin.
///
/// I/O errors are handled inline here (not via CliError): file-not-found and
/// permission errors are user/environment problems, not request-content errors.
/// Per design §4 ADR: `From<std::io::Error>` is NOT implemented for CliError;
/// I/O failures in main.rs exit with code 4 (InternalError level).
fn read_input(path: Option<&PathBuf>) -> Result<Vec<u8>, ()> {
    match path {
        Some(p) => match std::fs::read(p) {
            Ok(bytes) => Ok(bytes),
            Err(e) => {
                eprintln!("hydro-cli: cannot read input file '{}': {e}", p.display());
                Err(())
            }
        },
        None => {
            // Read from stdin; buffer fully before parsing so errors are clean.
            let stdin = io::stdin();
            let mut reader = BufReader::new(stdin.lock());
            let mut buf = Vec::new();
            if let Err(e) = reader.read_to_end(&mut buf) {
                eprintln!("hydro-cli: failed to read stdin: {e}");
                return Err(());
            }
            Ok(buf)
        }
    }
}

/// Write bytes to a file path or stdout.
///
/// Stdout write errors are fatal (pipe-broken, etc.). File write errors are
/// reported to stderr with an internal exit code (4).
fn write_output(path: Option<&PathBuf>, bytes: &[u8]) -> Result<(), ()> {
    match path {
        Some(p) => match std::fs::write(p, bytes) {
            Ok(()) => Ok(()),
            Err(e) => {
                eprintln!("hydro-cli: cannot write output file '{}': {e}", p.display());
                Err(())
            }
        },
        None => {
            let stdout = io::stdout();
            let mut handle = stdout.lock();
            if let Err(e) = handle.write_all(bytes) {
                eprintln!("hydro-cli: failed to write stdout: {e}");
                return Err(());
            }
            Ok(())
        }
    }
}

// ── main ──────────────────────────────────────────────────────────────────────

fn main() {
    let cli = Cli::parse();

    // Step 1: read input bytes.
    let input_bytes = match read_input(cli.input.as_ref()) {
        Ok(b) => b,
        Err(()) => process::exit(4), // I/O error reading input
    };

    // Step 2: deserialize DesignRequest.
    // serde_json::Error maps to CliError::ValidationError → exit 1 (design §4 ADR).
    let req: DesignRequest = match serde_json::from_slice(&input_bytes) {
        Ok(r) => r,
        Err(e) => {
            let cli_err = CliError::from(e);
            eprintln!("hydro-cli: {}", format_cli_error(&cli_err));
            process::exit(cli_err.exit_code());
        }
    };

    // Step 3: validate-only fast path (REQ-006, REQ-007).
    //
    // Emits a minimal status JSON to stdout then exits:
    //   Success: {"status":"valid","project_type":"<type>"}
    //   Failure: {"status":"invalid","error":"<message>"}
    //
    // No TerrainModel, GeneticOptimizer, or solver is instantiated here.
    // Wall time is dominated by binary startup (~50 ms); pure validate_request
    // + JSON emit is < 1 ms. Total < 50 ms on any reference fixture (REQ-007).
    if cli.validate_only {
        match validate_request(&req) {
            Ok(()) => {
                // Emit {"status":"valid","project_type":"<type>"} to stdout.
                let status_json = serde_json::json!({
                    "status": "valid",
                    "project_type": req.project_type.as_str()
                });
                let status_bytes = match serde_json::to_vec(&status_json) {
                    Ok(b) => b,
                    Err(e) => {
                        eprintln!("hydro-cli: failed to serialize validation status: {e}");
                        process::exit(4);
                    }
                };
                if write_output(cli.output.as_ref(), &status_bytes).is_err() {
                    process::exit(4);
                }
                process::exit(0);
            }
            Err(e) => {
                // Emit {"status":"invalid","error":"<message>"} to stdout.
                let status_json = serde_json::json!({
                    "status": "invalid",
                    "error": format_cli_error(&e)
                });
                let status_bytes = match serde_json::to_vec(&status_json) {
                    Ok(b) => b,
                    Err(se) => {
                        eprintln!("hydro-cli: failed to serialize validation error: {se}");
                        process::exit(4);
                    }
                };
                if write_output(cli.output.as_ref(), &status_bytes).is_err() {
                    process::exit(4);
                }
                eprintln!("hydro-cli: validation failed: {}", format_cli_error(&e));
                process::exit(e.exit_code());
            }
        }
    }

    // Step 4: full optimization path.
    let result = match run(req, cli.seed) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("hydro-cli: {}", format_cli_error(&e));
            process::exit(e.exit_code());
        }
    };

    // Step 5: serialize DesignResult JSON.
    let output_bytes = if cli.pretty {
        match serde_json::to_vec_pretty(&result) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("hydro-cli: failed to serialize result: {e}");
                process::exit(4);
            }
        }
    } else {
        match serde_json::to_vec(&result) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("hydro-cli: failed to serialize result: {e}");
                process::exit(4);
            }
        }
    };

    // Step 6: write output.
    if write_output(cli.output.as_ref(), &output_bytes).is_err() {
        process::exit(4);
    }

    // Success.
    process::exit(0);
}

// ── Error formatting ──────────────────────────────────────────────────────────

/// Format a `CliError` for stderr output.
///
/// Produces a short human-readable description without internal stack frames.
/// The exit code is already surfaced by the caller via `process::exit`.
fn format_cli_error(e: &CliError) -> String {
    match e {
        CliError::ValidationError(he) => format!("validation error: {he}"),
        CliError::NoFeasibleSolution => "no feasible solution found".to_string(),
        CliError::NormComplianceFailed(msg) => format!("norm compliance failed: {msg}"),
        CliError::InternalError(msg) => format!("internal error: {msg}"),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn binary_skeleton_compiles() {
        // Verifies the crate compiles with the clap shell in place.
        // The test itself is a no-op; compilation success is the assertion.
    }
}
