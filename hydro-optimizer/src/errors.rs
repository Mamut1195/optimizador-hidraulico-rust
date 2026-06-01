//! OptimizationError — typed errors for the hydro-optimizer crate.
//!
//! Mirrors Python exceptions from `hydro_engine/optimization/` and
//! `hydro_engine/solvers/base.py`. Variants cover: config validation,
//! solver failures, norm violations, and algorithmic edge cases.

use thiserror::Error;

/// Errors produced by the NSGA-III genetic optimizer.
///
/// Design §10: Most solver/objective failures become penalty fitness `[1e12; 5]`.
/// Errors here are reserved for setup-time problems and non-recoverable failures.
#[derive(Debug, Error)]
pub enum OptimizationError {
    /// Configuration is invalid (bad bounds, population_size=0, negative weights, etc.)
    #[error("config invalid: {0}")]
    InvalidConfig(String),

    /// No feasible individual found after all generations.
    #[error("no feasible individuals found after optimization")]
    AllInfeasible,

    /// Solver `.solve()` returned an error during evaluation.
    #[error("evaluator failure: {0}")]
    EvaluatorFailure(String),

    /// Norm validation failed in strict compliance mode.
    #[error("norm validation failure: {0}")]
    NormValidationFailure(String),

    /// Solver crate error (solver infrastructure failure — not per-solution failure).
    #[error("solver error: {0}")]
    Solver(#[from] hydro_solvers::SolverError),

    /// Wall-clock budget exceeded before the first generation could complete.
    #[error("wall-clock budget exceeded: {0:.1}s")]
    TimeBudgetExceeded(f64),

    /// Internal invariant violated (reference-point degeneracy not caught by fallback, etc.).
    #[error("internal invariant violated: {0}")]
    Internal(String),
}
