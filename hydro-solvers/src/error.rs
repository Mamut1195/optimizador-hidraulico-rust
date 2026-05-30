//! SolverError — domain errors for all hydro-solvers.
//!
//! Mirrors Python exceptions raised by BaseSolver and concrete solver classes.
//! Uses `thiserror` for typed, composable error handling (ADR-10).

use thiserror::Error;

/// Errors produced by hydraulic solver computations.
///
/// Variants mirror the Python oracle's exception taxonomy in
/// `hydro_engine/solvers/` (ValueError / RuntimeError hierarchy).
#[derive(Debug, Error)]
pub enum SolverError {
    /// No outlet (discharge point) was provided or found in the network.
    #[error("no outlet (discharge point) provided — SewerSolver requires an outlet")]
    NoOutlet,

    /// No source node found in the network.
    #[error("no source node found in the network")]
    NoSource,

    /// Fewer than 2 terminal nodes — cannot build a Steiner tree.
    #[error("need at least 2 terminal nodes (service + outlet)")]
    InsufficientTerminals,

    /// No feasible route was found between two nodes.
    #[error("no route found in terrain graph")]
    NoRoute,

    /// Solver could not produce any feasible solution.
    #[error("no feasible solution could be generated")]
    NoFeasibleSolution,

    /// Steiner tree construction failed (wraps the underlying message).
    #[error("Steiner tree failed: {0}")]
    SteinerFailed(String),

    /// Network build step failed (wraps the underlying message).
    #[error("network build failed: {0}")]
    BuildFailed(String),
}
