//! ProgressEvent and ProgressCallback for per-generation reporting.
//!
//! Design §2 exact struct signature. Emitted after each generation
//! via the optional callback in `OptimizationConfig`.

/// Per-generation progress snapshot emitted to the optional callback.
///
/// Design §2 `ProgressEvent` exact field list.
#[derive(Debug, Clone)]
pub struct ProgressEvent {
    /// Current generation number (1-indexed).
    pub generation: u32,
    /// Total number of individuals evaluated so far in this run.
    pub evaluations: u32,
    /// Best (minimum) fitness across all 5 objectives in this generation.
    pub min_fitness: [f64; 5],
    /// Average fitness across all evaluated individuals in this generation
    /// (feasible + penalty; excludes un-evaluated carry-overs).
    pub avg_fitness: [f64; 5],
    /// Wall-clock seconds elapsed since `optimize()` started.
    pub elapsed_seconds: f64,
}
