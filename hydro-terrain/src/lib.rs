//! hydro-terrain — Digital Elevation Model, KDTree, terrain graph, and Steiner tree.
//!
//! Depends on `kiddo` for KD-tree nearest-neighbor queries (replacing scipy.spatial.KDTree)
//! and `petgraph` for Dijkstra and MST (replacing networkx).
//!
//! Approximation note: off-grid elevation interpolation uses Delaunay-linear barycentric
//! interpolation via the `spade` crate, matching scipy.griddata(method="linear"). On-grid
//! and nearest-neighbor queries are EXACT-match. Off-grid queries are STATISTICAL
//! (p95 abs error ≤ 0.1 m per spec R-04).

pub mod cost;
pub mod error;
pub mod graph;
pub mod steiner;
pub mod terrain;

// Re-export the primary public types at crate root for ergonomic use in tests.
pub use cost::{CostFunction, CostWeights};
pub use error::TerrainError;
pub use graph::TerrainGraph;
pub use steiner::{steiner_tree, SteinerTree};
pub use terrain::TerrainModel;

/// Replicate `np.arange(start, stop + step * 0.5, step)` without float drift.
///
/// Returns values `start + i * step` while `<= stop + step * 0.5`.
/// This matches the Python oracle's `np.arange(x_min, x_max + resolution, resolution)`.
/// Consolidated here from terrain.rs and graph.rs to avoid duplication (S-SP-03).
pub(crate) fn arange(start: f64, stop: f64, step: f64) -> Vec<f64> {
    let mut result = Vec::new();
    let max_val = stop + step * 0.5;
    let mut i = 0usize;
    loop {
        let v = start + (i as f64) * step;
        if v > max_val {
            break;
        }
        result.push(v);
        i += 1;
    }
    result
}
