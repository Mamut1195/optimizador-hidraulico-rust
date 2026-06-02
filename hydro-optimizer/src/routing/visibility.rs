//! Visibility graph construction for path smoothing (REQ-012).
//!
//! Builds a graph over:
//! - start and end points,
//! - polygon vertices (corners of forbidden zones),
//!
//! and connects any two nodes whose direct connecting segment does not
//! intersect any forbidden polygon AND stays at least `clearance` metres
//! away from every polygon edge.
//!
//! Port reference: Python `routing/path_smoother.py` — `_build_visibility_graph`.

use crate::config::ForbiddenZone;

/// A node in the visibility graph.
///
/// Indices 0 and 1 are always `start` and `end`.
/// Remaining indices map to polygon vertex order (zone 0 verts, zone 1 verts, …).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct VNode {
    pub pos: [f64; 2],
}

/// An undirected edge in the visibility graph: `(from_idx, to_idx, weight)`.
pub(crate) type VEdge = (usize, usize, f64);

/// Build the visibility graph.
///
/// Nodes: `start`, `end`, and all polygon vertices whose host polygon is not
/// convex-hull-only (we include all vertices so A* can route around concavities).
///
/// Edges: every pair `(i, j)` for which the segment `nodes[i]` → `nodes[j]` is
/// visible — meaning it does not intersect any forbidden polygon and stays
/// at least `clearance` metres from every polygon edge.
///
/// # Parameters
/// - `start` / `end` — route endpoints (always nodes 0 and 1).
/// - `forbidden` — list of forbidden-zone polygons.
/// - `clearance` — minimum clearance distance from polygon edges (metres).
///
/// Returns `(nodes, edges)`.
pub(crate) fn build_visibility_graph(
    start: [f64; 2],
    end: [f64; 2],
    forbidden: &[ForbiddenZone],
    clearance: f64,
) -> (Vec<VNode>, Vec<VEdge>) {
    todo!("implement build_visibility_graph")
}

/// Return true iff segment `a→b` intersects the polygon edge `p→q` (proper
/// intersection only — touching at a shared endpoint is not an intersection).
pub(crate) fn segments_intersect(
    a: [f64; 2],
    b: [f64; 2],
    p: [f64; 2],
    q: [f64; 2],
) -> bool {
    todo!("implement segments_intersect")
}

/// Return true iff segment `a→b` passes within `d` of any edge of `polygon`.
pub(crate) fn segment_too_close(
    a: [f64; 2],
    b: [f64; 2],
    polygon: &[[f64; 2]],
    d: f64,
) -> bool {
    todo!("implement segment_too_close")
}

/// Euclidean distance between two 2-D points.
#[inline]
pub(crate) fn euclidean(a: [f64; 2], b: [f64; 2]) -> f64 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    (dx * dx + dy * dy).sqrt()
}

// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ForbiddenZone;

    fn square_zone(cx: f64, cy: f64, half: f64) -> ForbiddenZone {
        ForbiddenZone {
            vertices: vec![
                [cx - half, cy - half],
                [cx + half, cy - half],
                [cx + half, cy + half],
                [cx - half, cy + half],
            ],
            label: None,
        }
    }

    // ── segments_intersect ────────────────────────────────────────────────────

    /// Classic X-crossing: two diagonals of a unit square.
    #[test]
    fn test_segments_intersect_crossing() {
        // (0,0)→(2,2) crosses (2,0)→(0,2)
        assert!(segments_intersect(
            [0.0, 0.0],
            [2.0, 2.0],
            [2.0, 0.0],
            [0.0, 2.0]
        ));
    }

    /// Parallel horizontal segments — no intersection.
    #[test]
    fn test_segments_intersect_parallel_no_cross() {
        assert!(!segments_intersect(
            [0.0, 0.0],
            [5.0, 0.0],
            [0.0, 1.0],
            [5.0, 1.0]
        ));
    }

    /// T-shape: one segment's endpoint touches the other's interior.
    /// Design treats touching at a shared endpoint as NOT a blocking intersection
    /// so that visibility edges can share polygon vertices.
    #[test]
    fn test_segments_touch_at_endpoint_not_intersection() {
        // segment (0,0)→(0,5) and (0,5)→(5,5): share endpoint (0,5)
        // This should NOT be counted as a blocking cross.
        assert!(!segments_intersect(
            [0.0, 0.0],
            [0.0, 5.0],
            [0.0, 5.0],
            [5.0, 5.0]
        ));
    }

    // ── segment_too_close ─────────────────────────────────────────────────────

    /// Segment running exactly along a polygon edge (distance 0) is too close
    /// for any positive clearance.
    #[test]
    fn test_segment_too_close_coincident() {
        let polygon = vec![
            [0.0, 0.0],
            [10.0, 0.0],
            [10.0, 10.0],
            [0.0, 10.0],
        ];
        // Segment along bottom edge y=0
        assert!(segment_too_close(
            [0.0, 0.0],
            [10.0, 0.0],
            &polygon,
            0.1
        ));
    }

    /// Segment far from all polygon edges is NOT too close.
    #[test]
    fn test_segment_not_too_close_distant() {
        let polygon = vec![
            [0.0, 0.0],
            [5.0, 0.0],
            [5.0, 5.0],
            [0.0, 5.0],
        ];
        // Segment far away at y = 100
        assert!(!segment_too_close(
            [0.0, 100.0],
            [5.0, 100.0],
            &polygon,
            1.0
        ));
    }

    // ── build_visibility_graph ────────────────────────────────────────────────

    /// With no forbidden zones, start and end are directly connected.
    #[test]
    fn test_visibility_graph_no_obstacles() {
        let start = [0.0, 0.0];
        let end = [10.0, 0.0];
        let (nodes, edges) = build_visibility_graph(start, end, &[], 0.5);
        assert_eq!(nodes.len(), 2, "no vertices besides start/end when no zones");
        // Direct edge must exist
        let has_direct = edges.iter().any(|&(a, b, _)| (a == 0 && b == 1) || (a == 1 && b == 0));
        assert!(has_direct, "start→end edge must exist when no obstacles");
    }

    /// With a blocking square, the graph must include polygon vertex nodes.
    #[test]
    fn test_visibility_graph_with_obstacle_has_vertex_nodes() {
        let start = [0.0, 5.0];
        let end = [20.0, 5.0];
        let zone = square_zone(10.0, 5.0, 3.0);
        let (nodes, _edges) = build_visibility_graph(start, end, &[zone], 0.1);
        // 2 (start/end) + 4 (square vertices) = 6
        assert_eq!(nodes.len(), 6, "zone vertices must be added to graph");
    }

    /// No edge through the obstacle interior should exist.
    #[test]
    fn test_visibility_graph_no_edge_through_obstacle() {
        let start = [0.0, 5.0];
        let end = [20.0, 5.0];
        let zone = square_zone(10.0, 5.0, 3.0);
        let (nodes, edges) = build_visibility_graph(start, end, &[zone], 0.1);
        // The direct start(0)→end(1) edge passes through the centre — must NOT exist
        let _ = nodes;
        let has_direct = edges.iter().any(|&(a, b, _)| (a == 0 && b == 1) || (a == 1 && b == 0));
        assert!(!has_direct, "no direct edge through obstacle must exist");
    }

    // ── euclidean ─────────────────────────────────────────────────────────────

    #[test]
    fn test_euclidean_known_values() {
        assert!((euclidean([0.0, 0.0], [3.0, 4.0]) - 5.0).abs() < 1e-10);
        assert!((euclidean([1.0, 1.0], [1.0, 1.0])).abs() < 1e-10);
    }
}
