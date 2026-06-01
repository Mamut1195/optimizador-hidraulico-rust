//! hydro-optimizer — NSGA-III multi-objective optimizer (Phase 6).
//!
//! Faithful port of the Python genetic optimizer (`hydro_engine/optimization/`),
//! including IndividualEncoder, gene specs, reference-point lattice (Das-Dennis),
//! fast nondominated sort, SBX crossover, polynomial mutation, varOr,
//! niching/normalization/association, and the main optimization loop.
//!
//! # Public API (REQ-014)
//! - `OptimizationConfig` — hyperparameters and constraint flags.
//! - `OptimizationError` — typed errors.
//! - `ProgressEvent` — per-generation reporting.
//!
//! All other items are `pub(crate)` or private.
//!
//! # Determinism
//! Uses `ChaCha20Rng` seeded from `config.seed`. No `HashMap`, no
//! `thread_rng`. See `rng` module for child-RNG derivation strategy.

// ── Internal modules ──────────────────────────────────────────────────────────
pub(crate) mod config;
pub(crate) mod encoding;
pub(crate) mod errors;
pub(crate) mod progress;
pub(crate) mod rng;

// ── Public re-exports (REQ-014) ───────────────────────────────────────────────
pub use config::OptimizationConfig;
pub use errors::OptimizationError;
pub use progress::ProgressEvent;

// ── Integration-test surface (types consumed by tests/pr8a_*.rs) ─────────────
// These re-exports make internal types reachable at the crate root for
// integration tests without exposing full module paths as part of the
// public API surface.
pub use config::{ExistingNetwork, ForbiddenZone, MandatoryRoute};
pub use encoding::{gene_specs, GeneSpec, GeneType, Individual, IndividualEncoder, SolverType};
pub use rng::{child_rng, root_rng};

#[cfg(test)]
mod tests {
    #[test]
    fn workspace_member_compiles() {
        // Confirms the crate compiles and the workspace is healthy.
    }
}
