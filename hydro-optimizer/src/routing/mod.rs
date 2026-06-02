//! Routing module — PathSmoother (REQ-012).
//!
//! Provides `PathSmoother`: visibility-graph A* with RDP simplification.
//! Only instantiated when `config.enable_path_smoothing = true` (REQ-012).
//!
//! # Design §8 decisions
//! - Obstacle inflation is implemented as a **clearance check during
//!   visibility-graph construction** (not a geometric buffer), because `geo 0.28`
//!   does not expose Shapely-mitre-join `Polygon.buffer`.
//! - A candidate visibility edge is rejected if it passes within
//!   `clearance_distance` of any forbidden polygon edge.
//! - RDP epsilon defaults to `0.5` metres (configurable via `PathSmoother::new`).
//!
//! # Module layout
//! ```
//! routing/
//! ├── mod.rs          — PathSmoother entry point (this file)
//! ├── visibility.rs   — visibility graph construction
//! ├── astar.rs        — A* shortest path
//! └── rdp.rs          — Ramer-Douglas-Peucker simplification
//! ```

pub(crate) mod astar;
pub(crate) mod rdp;
pub(crate) mod visibility;

use crate::config::ForbiddenZone;
use astar::astar;
use rdp::rdp_simplify;
use visibility::{build_visibility_graph, euclidean};

// ── PathSmoother ──────────────────────────────────────────────────────────────

/// Converts raw gene-space pipe endpoints into smoothed, obstacle-free paths.
///
/// REQ-012: opt-in via `config.enable_path_smoothing`. Default `false`.
///
/// # Algorithm
/// 1. Build a visibility graph from forbidden-zone polygon vertices.
/// 2. Run A* from `start` to `end` on that graph.
/// 3. Apply RDP simplification to reduce waypoint count.
///
/// If A* finds no path (disconnected graph), the direct segment `[start, end]`
/// is returned as a fallback (rather than erroring — the constraint checker
/// will subsequently flag forbidden-zone violations on the straight segment).
pub(crate) struct PathSmoother {
    /// Minimum clearance from polygon edges used during visibility graph build.
    clearance: f64,
    /// RDP simplification epsilon (metres).
    rdp_epsilon: f64,
}

impl PathSmoother {
    /// Construct a `PathSmoother` with the given clearance and RDP epsilon.
    ///
    /// # Parameters
    /// - `clearance` — minimum gap between a visibility edge and any polygon edge.
    /// - `rdp_epsilon` — maximum deviation tolerance for RDP (metres).
    pub(crate) fn new(clearance: f64, rdp_epsilon: f64) -> Self {
        PathSmoother {
            clearance,
            rdp_epsilon,
        }
    }

    /// Smooth a route from `start` to `end` avoiding `forbidden_zones`.
    ///
    /// Returns an ordered list of 2-D waypoints `[x, y]` representing the
    /// smoothed, obstacle-free path.
    ///
    /// # Parameters
    /// - `start` — route start point `[x, y]`.
    /// - `end` — route end point `[x, y]`.
    /// - `forbidden_zones` — polygons that the path must not intersect or
    ///   come within `clearance` of.
    pub(crate) fn smooth(
        &self,
        start: [f64; 2],
        end: [f64; 2],
        forbidden_zones: &[ForbiddenZone],
    ) -> Vec<[f64; 2]> {
        todo!("implement PathSmoother::smooth")
    }

    /// Total Euclidean length of a polyline.
    pub(crate) fn path_length(path: &[[f64; 2]]) -> f64 {
        todo!("implement path_length")
    }
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

    // ── REQ-012 Scenario: PathSmoother skipped when disabled ──────────────────
    // (The optimizer-level skip is tested in optimizer.rs tests.)

    // ── REQ-012 Scenario: Smoothed path avoids forbidden zones ───────────────

    /// Direct path blocked by obstacle: smoother must route around it.
    #[test]
    fn test_smooth_avoids_obstacle() {
        // start=(0,5), end=(20,5), square obstacle at (10,5) radius=3
        // Direct segment passes through the obstacle.
        let smoother = PathSmoother::new(0.1, 0.5);
        let zone = square_zone(10.0, 5.0, 3.0);
        let path = smoother.smooth([0.0, 5.0], [20.0, 5.0], &[zone.clone()]);

        // Path must not be empty
        assert!(!path.is_empty(), "smoother must return a non-empty path");
        // Endpoints preserved
        assert_eq!(path[0], [0.0, 5.0], "start must be preserved");
        assert_eq!(path[path.len() - 1], [20.0, 5.0], "end must be preserved");

        // No segment of the path may pass through the obstacle's interior.
        // Verify that every waypoint is outside the obstacle (crude but sufficient).
        for wp in &path {
            let inside_x = wp[0] > 7.0 && wp[0] < 13.0;
            let inside_y = wp[1] > 2.0 && wp[1] < 8.0;
            assert!(
                !(inside_x && inside_y),
                "waypoint {wp:?} is inside the obstacle"
            );
        }
    }

    /// No obstacles: straight path is returned (length = Euclidean distance).
    #[test]
    fn test_smooth_no_obstacles_returns_direct() {
        let smoother = PathSmoother::new(0.5, 0.5);
        let path = smoother.smooth([0.0, 0.0], [10.0, 0.0], &[]);
        // With no obstacles, the path should be the direct two-point segment.
        assert!(path.len() >= 2, "path must have at least 2 points");
        assert_eq!(path[0], [0.0, 0.0]);
        assert_eq!(path[path.len() - 1], [10.0, 0.0]);
        let length = PathSmoother::path_length(&path);
        assert!(
            (length - 10.0).abs() < 0.01,
            "direct path length must be ~10 m, got {length}"
        );
    }

    /// Start equals end returns a trivial single-point path.
    #[test]
    fn test_smooth_start_equals_end() {
        let smoother = PathSmoother::new(0.5, 0.5);
        let path = smoother.smooth([5.0, 5.0], [5.0, 5.0], &[]);
        // At minimum the start point is in the path
        assert!(!path.is_empty());
    }

    // ── path_length tests ─────────────────────────────────────────────────────

    /// L-shaped path: 3→4→0 (unit segments 3+4=5 by Pythagoras or literal).
    #[test]
    fn test_path_length_l_shape() {
        let path = [[0.0, 0.0], [3.0, 0.0], [3.0, 4.0]];
        let len = PathSmoother::path_length(&path);
        assert!((len - 7.0).abs() < 1e-10, "expected 7.0, got {len}");
    }

    /// Single point has zero length.
    #[test]
    fn test_path_length_single_point() {
        let len = PathSmoother::path_length(&[[1.0, 2.0]]);
        assert_eq!(len, 0.0);
    }

    /// Empty path has zero length.
    #[test]
    fn test_path_length_empty() {
        let len = PathSmoother::path_length(&[]);
        assert_eq!(len, 0.0);
    }

    /// Straight path of known length.
    #[test]
    fn test_path_length_straight() {
        let path = [[0.0, 0.0], [10.0, 0.0]];
        let len = PathSmoother::path_length(&path);
        assert!((len - 10.0).abs() < 1e-10);
    }
}
