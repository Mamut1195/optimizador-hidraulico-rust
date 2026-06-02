//! hydro-cli library — public API for the JSON-in / JSON-out engine shell.
//!
//! # Public surface (REQ-001, REQ-010)
//!
//! - [`CliError`] — typed error enum with [`CliError::exit_code`] (REQ-003)
//! - [`run`] — full optimization dispatch stub (implemented in WU-4/5)
//! - [`validate_request`] — structural + solver-specific validation stub (WU-2)
//!
//! All items are re-exported at the crate root so integration tests can call
//! them directly without spawning a subprocess (REQ-010).

use hydro_optimizer::OptimizationError;
use hydro_solvers::SolverError;
use hydro_types::{error::HydroTypesError, request::DesignRequest, response::DesignResult};

// ── CliError ────────────────────────────────────────────────────────────────

/// Typed error for the `hydro-cli` library and binary.
///
/// Four variants map to four documented exit codes (REQ-003). The variant set
/// is closed: no additional variants are permitted to preserve the exit-code
/// contract.
///
/// # Exit code table
///
/// | Variant                    | `exit_code()` | Trigger                                                        |
/// |----------------------------|---------------|----------------------------------------------------------------|
/// | `ValidationError(_)`       | 1             | JSON parse error, hydro-types validation, missing solver field |
/// | `NoFeasibleSolution`       | 2             | `OptimizationError::AllInfeasible`                             |
/// | `NormComplianceFailed(_)`  | 3             | `OptimizationError::NormValidationFailure(_)`                  |
/// | `InternalError(_)`         | 4             | All other optimizer/solver failures                            |
#[derive(Debug)]
pub enum CliError {
    /// A request field failed validation (exit 1).
    ///
    /// Covers JSON parse errors (wrapped via `CrossFieldViolation`),
    /// hydro-types range/cross-field violations, and missing solver-required
    /// `Option<_>` fields.
    ValidationError(HydroTypesError),

    /// The optimizer ran to completion but found no feasible solution (exit 2).
    ///
    /// Produced when `OptimizationError::AllInfeasible` is returned.
    NoFeasibleSolution,

    /// All solutions violate norm compliance in strict mode (exit 3).
    ///
    /// Produced when `OptimizationError::NormValidationFailure` is returned.
    /// The inner `String` carries the norm-violation message for `stderr`.
    NormComplianceFailed(String),

    /// An unrecoverable internal engine error occurred (exit 4).
    ///
    /// Covers `InvalidConfig`, `EvaluatorFailure`, `Solver(_)`, `Internal`,
    /// and `TimeBudgetExceeded` from `OptimizationError`.
    InternalError(String),
}

impl CliError {
    /// Returns the process exit code for this error variant.
    ///
    /// Maps exactly to the REQ-003 table:
    /// - `ValidationError` → 1
    /// - `NoFeasibleSolution` → 2
    /// - `NormComplianceFailed` → 3
    /// - `InternalError` → 4
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::ValidationError(_) => 1,
            Self::NoFeasibleSolution => 2,
            Self::NormComplianceFailed(_) => 3,
            Self::InternalError(_) => 4,
        }
    }
}

// ── From impls ──────────────────────────────────────────────────────────────

impl From<HydroTypesError> for CliError {
    /// Wraps a hydro-types domain error as a `ValidationError` (exit 1).
    fn from(e: HydroTypesError) -> Self {
        Self::ValidationError(e)
    }
}

impl From<serde_json::Error> for CliError {
    /// Wraps a JSON parse/serialize error into `ValidationError` via
    /// `HydroTypesError::CrossFieldViolation` (exit 1).
    ///
    /// Design §4 ADR: avoids a separate `CliError::Json` variant; keeps the
    /// exit-code table at exactly four entries.
    fn from(e: serde_json::Error) -> Self {
        Self::ValidationError(HydroTypesError::CrossFieldViolation {
            message: e.to_string(),
        })
    }
}

impl From<OptimizationError> for CliError {
    /// Splits `OptimizationError` across the four `CliError` exit codes per
    /// design §4 arm-by-arm table.
    fn from(err: OptimizationError) -> Self {
        match err {
            // exit 2
            OptimizationError::AllInfeasible => Self::NoFeasibleSolution,

            // exit 3
            OptimizationError::NormValidationFailure(msg) => Self::NormComplianceFailed(msg),

            // exit 4 — config / evaluator / time-budget failures
            OptimizationError::InvalidConfig(msg) => {
                Self::InternalError(format!("invalid config: {msg}"))
            }
            OptimizationError::EvaluatorFailure(msg) => {
                Self::InternalError(format!("evaluator failure: {msg}"))
            }
            OptimizationError::TimeBudgetExceeded(secs) => {
                Self::InternalError(format!("time budget exceeded: {secs:.1}s"))
            }
            OptimizationError::Internal(msg) => Self::InternalError(msg),

            // exit 4 — solver-level errors (defense-in-depth; should be caught by
            // validate_request upstream, but mapped here for robustness)
            OptimizationError::Solver(se) => map_solver_error(se),
        }
    }
}

/// Maps a `SolverError` to a `CliError::InternalError` (exit 4).
///
/// Per design §4: most solver errors should be caught by `validate_request`
/// before reaching the optimizer. Reaching this function indicates a validator
/// bug (for `InsufficientTerminals`, `BuildFailed("unknown source_type")`), or
/// a genuine engine failure for the remaining variants.
fn map_solver_error(e: SolverError) -> CliError {
    match e {
        // Should have been caught by validate_request (§2 rule 3).
        SolverError::NoFeasibleSolution => CliError::NoFeasibleSolution,
        // All other solver errors are treated as internal engine failures.
        SolverError::InsufficientTerminals => CliError::InternalError(
            "insufficient terminal nodes — validate_request should have caught this".into(),
        ),
        SolverError::BuildFailed(msg) => {
            CliError::InternalError(format!("solver build failed: {msg}"))
        }
        SolverError::NoOutlet => {
            CliError::InternalError("solver: no outlet found in network".into())
        }
        SolverError::NoSource => {
            CliError::InternalError("solver: no source found in network".into())
        }
        SolverError::NoRoute => {
            CliError::InternalError("solver: no route found in terrain graph".into())
        }
        SolverError::SteinerFailed(msg) => {
            CliError::InternalError(format!("solver: Steiner tree failed: {msg}"))
        }
    }
}

// ── Public API stubs ────────────────────────────────────────────────────────
//
// Bodies are `todo!()` for WU-1. Signatures are locked per REQ-001.
// Implementation grows across WU-2 (validate_request) through WU-8 (audit_hash).

/// Run the full optimization pipeline for the given `DesignRequest`.
///
/// # Parameters
/// - `req` — fully-deserialised design request; all fields present per the
///   hydro-types schema.
/// - `seed_override` — when `Some(s)`, overrides `req.seed` before passing to
///   the optimizer. Used by `--seed` CLI flag and deterministic test fixtures.
///
/// # Returns
/// - `Ok(DesignResult)` on success — `solutions` non-empty, `success == true`.
/// - `Err(CliError)` — see [`CliError`] exit-code table.
///
/// # Implementation note (WU-1)
/// Body is a stub (`todo!()`). Dispatch, terrain build, config mapping, and
/// audit-hash computation are added in WU-3 through WU-8.
pub fn run(_req: DesignRequest, _seed_override: Option<u64>) -> Result<DesignResult, CliError> {
    todo!("run() is implemented in WU-4 (sewer dispatch) through WU-5 (remaining solvers)")
}

/// Validate a `DesignRequest` without running the optimizer.
///
/// Calls `req.validate()` from hydro-types, then performs per-`project_type`
/// presence checks on required `Option<_>` fields (design §2 table).
///
/// # Returns
/// - `Ok(())` — all checks pass.
/// - `Err(CliError::ValidationError(_))` — first failing check.
///
/// # Implementation note (WU-1)
/// Body is a stub (`todo!()`). Full implementation lands in WU-2.
pub fn validate_request(_req: &DesignRequest) -> Result<(), CliError> {
    todo!("validate_request() is implemented in WU-2")
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    #[test]
    fn lib_skeleton_compiles() {
        // Verifies the crate compiles with the [lib] target present.
        // The test itself is a no-op; compilation success is the assertion.
    }
}
