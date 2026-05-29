//! hydro-terrain — Digital Elevation Model, KDTree, terrain graph, and Steiner tree.
//!
//! Depends on `kiddo` for KD-tree nearest-neighbor queries (replacing scipy.spatial.KDTree)
//! and `petgraph` for Dijkstra and MST (replacing networkx).
//!
//! Approximation note: off-grid elevation interpolation uses inverse-distance weighting
//! seeded by kiddo NN, then bilinear on a regular grid. This differs from Python's
//! scipy.griddata (Delaunay linear) at off-grid build points; tolerance is STATISTICAL
//! (p95 abs error ≤ 0.1 m per spec R-04). On-grid and nearest-neighbor queries are EXACT-match.

pub mod cost;
pub mod error;
pub mod graph;
pub mod terrain;

// Re-export the primary public types at crate root for ergonomic use in tests.
pub use cost::{CostFunction, CostWeights};
pub use error::TerrainError;
pub use graph::TerrainGraph;
pub use terrain::TerrainModel;
