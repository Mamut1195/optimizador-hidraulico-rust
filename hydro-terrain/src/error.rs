//! Domain errors for hydro-terrain.

use thiserror::Error;

/// Errors produced by hydro-terrain operations.
#[derive(Debug, Error)]
pub enum TerrainError {
    /// No points were provided.
    #[error("terrain must have at least 3 points, got 0")]
    EmptyTerrain,

    /// Not enough points for interpolation.
    #[error("terrain requires at least {need} points, got {got}")]
    InsufficientPoints { got: usize, need: usize },

    /// `elevation_at` bilinear path called but grid was never built.
    #[error("regular grid has not been built — call build_grid() first")]
    GridNotBuilt,

    /// Node ID not found in the graph.
    #[error("node '{id}' not found in terrain graph")]
    NodeNotFound { id: String },

    /// Graph has no nodes.
    #[error("terrain graph is empty — call from_grid() or add nodes first")]
    EmptyGraph,

    /// Not enough terminals for Steiner tree.
    #[error("Steiner tree requires at least 2 terminals, got {got}")]
    InsufficientTerminals { got: usize },

    /// No path exists between source and target.
    #[error("no path exists from '{from}' to '{to}'")]
    NoPath { from: String, to: String },
}
