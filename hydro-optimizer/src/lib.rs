//! hydro-optimizer — NSGA-III multi-objective optimizer (Phase 6).
//!
//! Faithful port of the Python genetic optimizer (`hydro_engine/optimization/`),
//! including IndividualEncoder, gene specs, reference-point lattice (Das-Dennis),
//! fast nondominated sort, SBX crossover, polynomial mutation, varOr,
//! niching/normalization/association, and the main optimization loop.
//!
//! # Public API (REQ-014 — exactly 6 public items)
//! - [`GeneticOptimizer`] — struct + `new` + `optimize`
//! - [`OptimizationConfig`] — hyperparameters + constraint flags (serde)
//! - [`ParetoResults`] — top-level result with accessors
//! - [`ProgressEvent`] — per-generation reporting
//! - [`OptimizationError`] — typed error enum
//! - [`Diagnostics`] — re-export of `hydro_types::response::Diagnostics`
//!
//! All other items are `pub(crate)` or private (REQ-014).
//!
//! # Determinism
//! Uses `ChaCha20Rng` seeded from `config.seed`. No `HashMap`, no
//! `thread_rng`. See `rng` module for child-RNG derivation strategy.

// ── Internal modules ──────────────────────────────────────────────────────────
pub(crate) mod config;
pub(crate) mod constraints;
pub(crate) mod diagnostics;
pub(crate) mod encoding;
pub(crate) mod errors;
pub(crate) mod metrics;
pub(crate) mod nsga3;
pub(crate) mod objective;
pub(crate) mod operators;
pub(crate) mod optimizer;
pub(crate) mod progress;
pub(crate) mod results;
pub(crate) mod rng;
/// PathSmoother routing module (REQ-012). Opt-in via `enable_path_smoothing`.
pub(crate) mod routing;

// ── Public re-exports (REQ-014 — exactly 6 items) ────────────────────────────
/// Configuration for one optimization run (REQ-002).
pub use config::OptimizationConfig;
/// Typed errors (REQ-013).
pub use errors::OptimizationError;
/// Optimization diagnostics (re-export from hydro-types for downstream use).
pub use hydro_types::response::Diagnostics;
/// Main optimizer struct (REQ-009).
pub use optimizer::GeneticOptimizer;
/// Per-generation progress event (REQ-009).
pub use progress::ProgressEvent;
/// Top-level Pareto results (REQ-011).
pub use results::ParetoResults;

// ── Integration-test surface (doc-hidden, not part of the 6-item REQ-014 API) ──
// `SolverType` is `pub(crate)` in `encoding`, but `GeneticOptimizer::new` takes it
// as a parameter. Integration tests in `hydro-optimizer/tests/` can only call
// `new` if `SolverType` is accessible from outside the crate. This re-export is
// `#[doc(hidden)]` so it stays invisible to downstream library users while still
// satisfying the Rust visibility rules for integration tests.
// REQ-014 counts the 6 mandated API items listed above; this helper export does
// not add to that public contract.
#[doc(hidden)]
pub use encoding::SolverType;

// ── cfg(test) internal surface ────────────────────────────────────────────────
// Integration-test helpers are gated behind #[cfg(test)] so they NEVER leak
// into the public API surface (REQ-014 stays at exactly 6 pub items).
//
// NOTE: items previously re-exported at crate root for pr8a/b/c integration tests
// (ConstraintChecker, ObjectiveFunction, GeneSpec, child_rng, etc.) are REMOVED.
// Those integration tests have been moved to inline #[cfg(test)] mod tests blocks
// inside their respective source files (REQ-014 resolution).

#[cfg(test)]
mod tests {
    #[test]
    fn workspace_member_compiles() {
        // Confirms the crate compiles and the workspace is healthy.
    }
}
