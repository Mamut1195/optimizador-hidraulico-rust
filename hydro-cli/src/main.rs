//! hydro-cli — Thin JSON in/out shell for the hydraulic optimization engine.
//!
//! Reads a DesignRequest JSON from `--input <path>` or stdin,
//! runs the optimizer, and emits a Solution JSON on stdout.
//!
//! Exit codes:
//!   0 — success
//!   1 — validation error (bad DesignRequest)
//!   2 — no feasible solution found
//!   3 — no norm-compliant solution (strict_norm_compliance gate)
//!   4 — internal engine error
//!
//! # WU-1 placeholder
//! Full clap shell + dispatch wired in WU-6. Until then the binary produces a
//! typed `InternalError` and exits with its exit_code() (4), preserving the
//! scaffold behaviour while routing through the new error type.

use hydro_cli::CliError;

fn main() {
    let err = CliError::InternalError(
        "hydro-cli: engine not yet implemented (scaffold only — WU-6 will replace this)".into(),
    );
    eprintln!(
        "hydro-cli: {}",
        match &err {
            CliError::InternalError(msg) => msg.as_str(),
            _ => "unexpected error variant in scaffold",
        }
    );
    std::process::exit(err.exit_code());
}

#[cfg(test)]
mod tests {
    #[test]
    fn binary_skeleton_compiles() {
        // The binary must at minimum compile and link.
        // The test passing confirms compilation succeeded.
    }
}
