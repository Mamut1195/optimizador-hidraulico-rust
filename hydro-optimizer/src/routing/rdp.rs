//! Ramer-Douglas-Peucker (RDP) polyline simplification (REQ-012).
//!
//! Reduces the number of waypoints in a smoothed path while preserving the
//! overall shape within an `epsilon` tolerance.
//!
//! Port reference: Python `routing/path_smoother.py` — `_rdp_simplify` helper.

/// Simplify a polyline using the Ramer-Douglas-Peucker algorithm.
///
/// # Parameters
/// - `points` — ordered 2-D waypoints `[x, y]`.
/// - `epsilon` — maximum perpendicular distance tolerance (metres).
///
/// Returns the simplified polyline (subset of the input points).
/// If fewer than 2 points are provided, returns the input unchanged.
pub(crate) fn rdp_simplify(points: &[[f64; 2]], epsilon: f64) -> Vec<[f64; 2]> {
    todo!("implement RDP simplification")
}

/// Perpendicular distance from point `p` to the line through `a` and `b`.
///
/// Returns 0.0 if `a == b` (degenerate segment).
pub(crate) fn perp_distance(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    todo!("implement perp_distance")
}

// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;

    // ── rdp_simplify tests ────────────────────────────────────────────────────

    /// Collinear points are collapsed to endpoints.
    ///
    /// A straight line [A, B, C] where B lies exactly on AC → only [A, C] remain.
    #[test]
    fn test_rdp_collinear_points_collapsed() {
        let points = [[0.0, 0.0], [5.0, 0.0], [10.0, 0.0]];
        let result = rdp_simplify(&points, 0.01);
        // Interior point is exactly on the line — must be removed.
        assert_eq!(result.len(), 2, "collinear interior points must be removed");
        assert_eq!(result[0], [0.0, 0.0]);
        assert_eq!(result[result.len() - 1], [10.0, 0.0]);
    }

    /// Significant deviation is retained.
    ///
    /// L-shaped path [A, B, C] where B is far off the direct A→C line.
    #[test]
    fn test_rdp_significant_deviation_retained() {
        // A=(0,0), B=(5,5), C=(10,0): B is 3.54 m off the AC line.
        let points = [[0.0, 0.0], [5.0, 5.0], [10.0, 0.0]];
        let result = rdp_simplify(&points, 0.5);
        // B is too far from AC to be dropped.
        assert_eq!(result.len(), 3, "significant bend must be retained");
    }

    /// Empty input returns empty output.
    #[test]
    fn test_rdp_empty_input() {
        let result = rdp_simplify(&[], 0.5);
        assert!(result.is_empty());
    }

    /// Single point returns single point.
    #[test]
    fn test_rdp_single_point() {
        let result = rdp_simplify(&[[1.0, 2.0]], 0.5);
        assert_eq!(result, vec![[1.0, 2.0]]);
    }

    /// Two-point path always returns both endpoints.
    #[test]
    fn test_rdp_two_points_unchanged() {
        let pts = [[0.0, 0.0], [10.0, 10.0]];
        let result = rdp_simplify(&pts, 0.5);
        assert_eq!(result.len(), 2);
    }

    /// Large epsilon collapses everything to two endpoints.
    #[test]
    fn test_rdp_large_epsilon_collapses_to_endpoints() {
        // Zigzag path — with large enough epsilon every middle point is dropped.
        let points = [[0.0, 0.0], [1.0, 1.0], [2.0, 0.5], [3.0, 1.0], [4.0, 0.0]];
        let result = rdp_simplify(&points, 10.0);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], points[0]);
        assert_eq!(result[result.len() - 1], *points.last().unwrap());
    }

    /// Endpoints are always preserved.
    #[test]
    fn test_rdp_endpoints_always_preserved() {
        let points = [[3.0, 7.0], [5.0, 0.0], [8.0, 12.0]];
        let result = rdp_simplify(&points, 0.001);
        assert_eq!(result[0], points[0]);
        assert_eq!(result[result.len() - 1], *points.last().unwrap());
    }

    // ── perp_distance tests ───────────────────────────────────────────────────

    /// Point on the line has zero distance.
    #[test]
    fn test_perp_distance_on_line() {
        let d = perp_distance([5.0, 0.0], [0.0, 0.0], [10.0, 0.0]);
        assert!(d.abs() < 1e-12, "point on line must have 0 distance, got {d}");
    }

    /// Perpendicular case matches Euclidean geometry.
    #[test]
    fn test_perp_distance_perpendicular() {
        // Point (5, 3) to horizontal line y=0 from (0,0) to (10,0): dist = 3.
        let d = perp_distance([5.0, 3.0], [0.0, 0.0], [10.0, 0.0]);
        assert!((d - 3.0).abs() < 1e-9, "expected 3.0, got {d}");
    }

    /// Degenerate segment (a == b) returns 0.
    #[test]
    fn test_perp_distance_degenerate_segment() {
        let d = perp_distance([1.0, 2.0], [3.0, 4.0], [3.0, 4.0]);
        assert_eq!(d, 0.0, "degenerate segment must return 0");
    }
}
