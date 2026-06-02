//! A* shortest-path search over a visibility graph (REQ-012).
//!
//! Port reference: Python `routing/path_smoother.py` — `_astar_path`.
//!
//! Uses Euclidean distance as both edge weight and admissible heuristic
//! (straight-line distance to goal never over-estimates actual path cost).

use crate::routing::visibility::VNode;

/// Run A* from `start_idx` to `end_idx` on the visibility graph `(nodes, adj)`.
///
/// `adj[i]` is a list of `(neighbour_idx, edge_weight)` pairs for node `i`.
///
/// Returns `Some(path_indices)` — ordered list of node indices from start to end —
/// or `None` if no path exists.
pub(crate) fn astar(
    nodes: &[VNode],
    adj: &[Vec<(usize, f64)>],
    start_idx: usize,
    end_idx: usize,
) -> Option<Vec<usize>> {
    use std::collections::BTreeMap;

    let n = nodes.len();
    if n == 0 || start_idx >= n || end_idx >= n {
        return None;
    }

    // Trivial case: start == end
    if start_idx == end_idx {
        return Some(vec![start_idx]);
    }

    // g_score[i] = best known cost from start to i
    // Using BTreeMap for determinism (design §12: no HashMap).
    let mut g_score: BTreeMap<usize, f64> = BTreeMap::new();
    g_score.insert(start_idx, 0.0);

    // came_from[i] = predecessor of i on the best path
    let mut came_from: BTreeMap<usize, usize> = BTreeMap::new();

    // Open set as Vec of (f_score_bits, idx) — use u64 bits of f64 for BTree ordering.
    // Sorting by f64::to_bits() is valid only for non-negative f64 (NaN-free path costs).
    let mut open: Vec<(u64, usize)> = Vec::new();

    let h = |idx: usize| -> f64 {
        let a = nodes[idx].pos;
        let b = nodes[end_idx].pos;
        let dx = a[0] - b[0];
        let dy = a[1] - b[1];
        (dx * dx + dy * dy).sqrt()
    };

    open.push((h(start_idx).to_bits(), start_idx));

    while !open.is_empty() {
        // Pop the node with the lowest f-score
        open.sort_unstable_by_key(|&(f, _)| f);
        let (_, current) = open.remove(0);

        if current == end_idx {
            // Reconstruct path
            let mut path = vec![end_idx];
            let mut cur = end_idx;
            while let Some(&prev) = came_from.get(&cur) {
                path.push(prev);
                cur = prev;
            }
            path.reverse();
            return Some(path);
        }

        let g_current = *g_score.get(&current).unwrap_or(&f64::INFINITY);

        for &(neighbour, weight) in &adj[current] {
            if neighbour >= n {
                continue;
            }
            let tentative_g = g_current + weight;
            let neighbour_g = *g_score.get(&neighbour).unwrap_or(&f64::INFINITY);
            if tentative_g < neighbour_g {
                came_from.insert(neighbour, current);
                g_score.insert(neighbour, tentative_g);
                let f = tentative_g + h(neighbour);
                // Remove stale open entry for this neighbour, add fresh one
                open.retain(|&(_, idx)| idx != neighbour);
                open.push((f.to_bits(), neighbour));
            }
        }
    }

    None // No path found
}

// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use crate::routing::visibility::VNode;

    fn node(x: f64, y: f64) -> VNode {
        VNode { pos: [x, y] }
    }

    /// Trivial 2-node graph: direct path start→end.
    #[test]
    fn test_astar_direct_path() {
        let nodes = vec![node(0.0, 0.0), node(10.0, 0.0)];
        let adj = vec![vec![(1, 10.0)], vec![(0, 10.0)]];
        let path = astar(&nodes, &adj, 0, 1).expect("path must exist");
        assert_eq!(path, vec![0, 1]);
    }

    /// 3-node graph with direct edge blocked: must route through middle.
    #[test]
    fn test_astar_routes_through_waypoint() {
        // 0--(5)--1--(5)--2,  no direct 0→2 edge
        let nodes = vec![node(0.0, 0.0), node(5.0, 0.0), node(10.0, 0.0)];
        let adj = vec![
            vec![(1, 5.0)],           // 0→1
            vec![(0, 5.0), (2, 5.0)], // 1↔0, 1→2
            vec![(1, 5.0)],           // 2→1
        ];
        let path = astar(&nodes, &adj, 0, 2).expect("path must exist");
        assert_eq!(path, vec![0, 1, 2]);
    }

    /// Disconnected graph: no path exists.
    #[test]
    fn test_astar_no_path_returns_none() {
        let nodes = vec![node(0.0, 0.0), node(10.0, 0.0)];
        let adj = vec![vec![], vec![]]; // no edges
        let path = astar(&nodes, &adj, 0, 1);
        assert!(path.is_none(), "disconnected graph must return None");
    }

    /// Start == end returns path of length 1.
    #[test]
    fn test_astar_start_equals_end() {
        let nodes = vec![node(1.0, 2.0)];
        let adj = vec![vec![]];
        let path = astar(&nodes, &adj, 0, 0).expect("trivial path must exist");
        assert_eq!(path, vec![0]);
    }

    /// A* picks the shorter of two competing routes.
    #[test]
    fn test_astar_picks_shortest_path() {
        // Four nodes: 0=(0,0), 1=(1,0), 2=(0,10), 3=(10,0)
        // End = 3.  Two paths: 0→1→3 (cost 1+9=10) vs 0→2→3 (cost 10+14.14≈24.14)
        let nodes = vec![
            node(0.0, 0.0),
            node(1.0, 0.0),
            node(0.0, 10.0),
            node(10.0, 0.0),
        ];
        let adj = vec![
            vec![(1, 1.0), (2, 10.0)],   // 0→1, 0→2
            vec![(0, 1.0), (3, 9.0)],    // 1→0, 1→3
            vec![(0, 10.0), (3, 14.14)], // 2→0, 2→3
            vec![(1, 9.0), (2, 14.14)],  // 3→1, 3→2
        ];
        let path = astar(&nodes, &adj, 0, 3).expect("path must exist");
        // Optimal: 0→1→3
        assert_eq!(path, vec![0, 1, 3], "A* must pick the shortest path");
    }
}
