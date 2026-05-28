//! hydro-terrain — Digital Elevation Model, KDTree, terrain graph, and Steiner tree.
//!
//! Depends on `kiddo` for KD-tree nearest-neighbor queries (replacing scipy.spatial.KDTree)
//! and `petgraph` for Dijkstra and MST (replacing networkx).
//!
//! Approximation note: off-grid elevation interpolation uses inverse-distance weighting
//! seeded by kiddo NN, then bilinear on a regular grid. This differs from Python's
//! scipy.griddata (Delaunay linear) at off-grid build points; tolerance is STATISTICAL
//! (p95 abs error ≤ 0.1 m). On-grid and nearest-neighbor queries are EXACT-match.

#[cfg(test)]
mod tests {
    #[test]
    fn workspace_member_compiles() {
        // The test passing confirms compilation succeeded.
    }
}
