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
//! - `ProgressEvent` / `ProgressCallback` — per-generation reporting.
//!
//! All other items are `pub(crate)` or private.
//!
//! # Determinism
//! Uses `ChaCha20Rng` seeded from `config.seed`. No `HashMap`, no
//! `thread_rng`. See `rng` module for child-RNG derivation strategy.

// ── Internal modules ──────────────────────────────────────────────────────────
pub mod rng;

// PR-8a modules (public for tests)
pub mod config;
pub mod encoding;
pub mod errors;
pub mod progress;

// ── Public re-exports (REQ-014) ───────────────────────────────────────────────
pub use config::OptimizationConfig;
pub use errors::OptimizationError;
pub use progress::ProgressEvent;

#[cfg(test)]
mod tests {
    #[test]
    fn workspace_member_compiles() {
        // Confirms the crate compiles and the workspace is healthy.
    }
}
