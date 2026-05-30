//! TerrainGraph — weighted 4-neighbor grid graph for pathfinding.
//!
//! Mirrors Python `hydro_engine.core.graph.TerrainGraph` exactly:
//! - Nodes: `g{counter}` IDs, counter increments i-outer, j-inner loop.
//! - Edges: 4-neighbor connections via `(0,1)` and `(1,0)` direction pairs only.
//! - Weights: `CostFunction::call(length, z_start, z_end)`.
//! - Dijkstra: petgraph `astar` with zero heuristic (exact Dijkstra).
//! - nearest_node: kiddo KDTree nearest_one on node XY.

use std::collections::HashMap;

use kiddo::float::kdtree::KdTree;
use kiddo::SquaredEuclidean;
use petgraph::algo::astar;
use petgraph::graph::{NodeIndex, UnGraph};
use petgraph::visit::EdgeRef;

use crate::cost::CostFunction;
use crate::error::TerrainError;
use crate::terrain::TerrainModel;

// ── Node and edge data ────────────────────────────────────────────────────────

/// Data stored at each graph node.
#[derive(Debug, Clone)]
pub struct NodeData {
    /// Oracle node ID (e.g., "g0", "g42").
    pub id: String,
    /// Easting coordinate.
    pub x: f64,
    /// Northing coordinate.
    pub y: f64,
    /// Terrain elevation at this node.
    pub z: f64,
}

/// Data stored on each graph edge.
#[derive(Debug, Clone)]
pub struct EdgeData {
    /// Weighted cost (from CostFunction).
    pub weight: f64,
    /// Horizontal length between the two nodes.
    pub length: f64,
}

// ── TerrainGraph ──────────────────────────────────────────────────────────────

/// Weighted undirected graph over terrain grid nodes.
///
/// Mirrors Python `TerrainGraph` using petgraph + kiddo instead of networkx + scipy.
pub struct TerrainGraph {
    /// The underlying petgraph undirected graph.
    graph: UnGraph<NodeData, EdgeData, u32>,
    /// Map from node ID string → petgraph NodeIndex for O(1) lookup.
    node_index_map: HashMap<String, NodeIndex<u32>>,
    /// KDTree on node XY for nearest_node queries.
    /// Bucket size 64 handles 51 collinear nodes in a 51x51 grid.
    kdtree: KdTree<f64, u64, 2, 64, u32>,
    /// Parallel array to KDTree items: maps u64 item → NodeIndex
    kdtree_items: Vec<NodeIndex<u32>>,
    /// Cost function for edge weight computation.
    cost_fn: CostFunction,
}

impl TerrainGraph {
    /// Create a new empty `TerrainGraph`.
    pub fn new(cost_fn: CostFunction) -> Self {
        Self {
            graph: UnGraph::new_undirected(),
            node_index_map: HashMap::new(),
            kdtree: KdTree::<f64, u64, 2, 64, u32>::new(),
            kdtree_items: Vec::new(),
            cost_fn,
        }
    }

    /// Add a node with terrain-derived elevation.
    ///
    /// Does not rebuild the KDTree immediately — `from_grid` adds all nodes first.
    fn add_node_internal(&mut self, id: &str, x: f64, y: f64, z: f64) -> NodeIndex<u32> {
        let ni = self.graph.add_node(NodeData {
            id: id.to_string(),
            x,
            y,
            z,
        });
        self.node_index_map.insert(id.to_string(), ni);

        let item = self.kdtree_items.len() as u64;
        self.kdtree.add(&[x, y], item);
        self.kdtree_items.push(ni);

        ni
    }

    /// Add a weighted edge between two existing nodes.
    ///
    /// Mirrors Python `add_edge`: computes Euclidean length + CostFunction weight.
    pub fn add_edge(&mut self, from_id: &str, to_id: &str) -> Result<(), TerrainError> {
        let &ni_from =
            self.node_index_map
                .get(from_id)
                .ok_or_else(|| TerrainError::NodeNotFound {
                    id: from_id.to_string(),
                })?;
        let &ni_to = self
            .node_index_map
            .get(to_id)
            .ok_or_else(|| TerrainError::NodeNotFound {
                id: to_id.to_string(),
            })?;

        let n1 = &self.graph[ni_from];
        let n2 = &self.graph[ni_to];
        let length = ((n1.x - n2.x).powi(2) + (n1.y - n2.y).powi(2)).sqrt();
        let weight = self.cost_fn.call(length, n1.z, n2.z);

        self.graph
            .add_edge(ni_from, ni_to, EdgeData { weight, length });
        Ok(())
    }

    /// Build graph as a regular grid over the terrain bounds.
    ///
    /// Mirrors Python `from_grid`:
    /// - xs = arange(x_min, x_max + resolution, resolution)
    /// - ys = arange(y_min, y_max + resolution, resolution)
    /// - Node IDs: `g{counter}`, counter in (i-outer, j-inner) loop order.
    /// - Edges: only `(di, dj)` in `[(0,1), (1,0)]` (4-neighbor, forward only).
    pub fn from_grid(&mut self, resolution: f64, terrain: &TerrainModel) {
        let (x_min, x_max, y_min, y_max) = terrain.bounds();
        let xs = crate::arange(x_min, x_max, resolution);
        let ys = crate::arange(y_min, y_max, resolution);

        let nx = xs.len();
        let ny = ys.len();

        // grid_ids[(i, j)] → node ID string
        let mut grid_ids: Vec<Vec<String>> = vec![vec![String::new(); ny]; nx];
        let mut counter = 0usize;

        for (i, &x) in xs.iter().enumerate() {
            for (j, &y) in ys.iter().enumerate() {
                let nid = format!("g{counter}");
                let z = terrain.elevation_at(x, y);
                self.add_node_internal(&nid, x, y, z);
                grid_ids[i][j] = nid;
                counter += 1;
            }
        }

        // Connect 4-neighbors: only (0,1) and (1,0) forward pairs
        for i in 0..nx {
            for j in 0..ny {
                for (di, dj) in [(0usize, 1usize), (1, 0)] {
                    let ni2 = i + di;
                    let nj2 = j + dj;
                    if ni2 < nx && nj2 < ny {
                        let from_id = &grid_ids[i][j];
                        let to_id = &grid_ids[ni2][nj2];
                        // Ignore error: both nodes exist by construction
                        let _ = self.add_edge(from_id, to_id);
                    }
                }
            }
        }
    }

    /// Find the graph node nearest to (x, y).
    ///
    /// Uses kiddo `nearest_one::<SquaredEuclidean>`.
    ///
    /// # Errors
    ///
    /// Returns `TerrainError::EmptyGraph` if the graph has no nodes.
    pub fn nearest_node(&self, x: f64, y: f64) -> Result<String, TerrainError> {
        if self.node_index_map.is_empty() {
            return Err(TerrainError::EmptyGraph);
        }
        let nn = self.kdtree.nearest_one::<SquaredEuclidean>(&[x, y]);
        let ni = self.kdtree_items[nn.item as usize]; // item is u64
        Ok(self.graph[ni].id.clone())
    }

    /// Dijkstra shortest path between two nodes (path node ID list).
    ///
    /// Uses `petgraph::algo::astar` with zero heuristic (= exact Dijkstra).
    ///
    /// # Errors
    ///
    /// Returns `TerrainError::NodeNotFound` if source or target is missing.
    /// Returns `TerrainError::NoPath` if no path exists.
    pub fn shortest_path(&self, source: &str, target: &str) -> Result<Vec<String>, TerrainError> {
        let &ni_src =
            self.node_index_map
                .get(source)
                .ok_or_else(|| TerrainError::NodeNotFound {
                    id: source.to_string(),
                })?;
        let &ni_tgt =
            self.node_index_map
                .get(target)
                .ok_or_else(|| TerrainError::NodeNotFound {
                    id: target.to_string(),
                })?;

        let result = astar(
            &self.graph,
            ni_src,
            |n| n == ni_tgt,
            |e| e.weight().weight,
            |_| 0.0, // zero heuristic → exact Dijkstra
        );

        match result {
            Some((_cost, path_indices)) => Ok(path_indices
                .iter()
                .map(|&ni| self.graph[ni].id.clone())
                .collect()),
            None => Err(TerrainError::NoPath {
                from: source.to_string(),
                to: target.to_string(),
            }),
        }
    }

    /// Total cost of the Dijkstra shortest path between two nodes.
    ///
    /// # Errors
    ///
    /// Returns `TerrainError::NodeNotFound` if source or target is missing.
    /// Returns `TerrainError::NoPath` if no path exists.
    pub fn shortest_path_cost(&self, source: &str, target: &str) -> Result<f64, TerrainError> {
        let &ni_src =
            self.node_index_map
                .get(source)
                .ok_or_else(|| TerrainError::NodeNotFound {
                    id: source.to_string(),
                })?;
        let &ni_tgt =
            self.node_index_map
                .get(target)
                .ok_or_else(|| TerrainError::NodeNotFound {
                    id: target.to_string(),
                })?;

        let result = astar(
            &self.graph,
            ni_src,
            |n| n == ni_tgt,
            |e| e.weight().weight,
            |_| 0.0,
        );

        match result {
            Some((cost, _)) => Ok(cost),
            None => Err(TerrainError::NoPath {
                from: source.to_string(),
                to: target.to_string(),
            }),
        }
    }

    /// Apply random weight perturbation seeded by `variant`.
    ///
    /// Mirrors Python `_perturb_graph_weights(variant)`:
    /// - seed = `variant * 42 + 7`
    /// - strength = `variant * 0.05`
    /// - noise in `[-strength, +strength]` per edge
    /// - `weight = max(0.1, weight * (1.0 + noise))`
    ///
    /// Statistical parity only (Rust SmallRng vs Python numpy.RandomState differ).
    /// Correctness tested in T-4.3 / PR-5.
    pub fn perturb_weights(&mut self, variant: u32) {
        use rand::rngs::SmallRng;
        use rand::{Rng, SeedableRng};

        if variant == 0 {
            return;
        }

        let mut rng = SmallRng::seed_from_u64((variant as u64) * 42 + 7);
        let strength = (variant as f64) * 0.05;

        for edge in self.graph.edge_weights_mut() {
            // Match the Python oracle exactly: noise = 1 + uniform(-s, s), then
            // weight *= max(0.1, noise). The clamp applies to the MULTIPLIER, not
            // the product (Python: `weight *= max(0.1, noise)`).
            let noise: f64 = rng.gen_range(-strength..=strength);
            edge.weight *= f64::max(0.1, 1.0 + noise);
        }
    }

    /// Number of nodes in the graph.
    pub fn node_count(&self) -> usize {
        self.graph.node_count()
    }

    /// Number of edges in the graph.
    pub fn edge_count(&self) -> usize {
        self.graph.edge_count()
    }

    /// Return (x, y) coordinate of the node with the given ID.
    ///
    /// Returns None if the node ID is not found in the graph.
    pub fn get_node_coord(&self, node_id: &str) -> Option<(f64, f64)> {
        self.node_index_map
            .get(node_id)
            .map(|&ni| (self.graph[ni].x, self.graph[ni].y))
    }

    /// Return the terrain elevation of the node with the given ID.
    ///
    /// Returns None if the node ID is not found in the graph.
    pub fn get_node_elevation(&self, node_id: &str) -> Option<f64> {
        self.node_index_map.get(node_id).map(|&ni| self.graph[ni].z)
    }

    /// Return all node IDs in the graph (in petgraph internal order).
    pub fn node_ids(&self) -> Vec<String> {
        self.graph
            .node_indices()
            .map(|ni| self.graph[ni].id.clone())
            .collect()
    }

    /// Compute k shortest paths from source to target using a node-exclusion variant.
    ///
    /// Returns up to `k` paths as Vec<Vec<String>> (node ID lists). The first path
    /// is the standard Dijkstra shortest path. Subsequent paths are generated by
    /// perturbing intermediate node selections (simplified for practical use:
    /// excludes each intermediate node of the prior best path and re-runs Dijkstra).
    ///
    /// This mirrors the Python SewerSolver alternative generation pattern which
    /// uses networkx k_shortest_paths for route diversity; exact cross-language
    /// path parity is STATISTICAL (topology differs due to RNG and edge ordering).
    ///
    /// # Errors
    ///
    /// Returns `TerrainError::NodeNotFound` if source or target is missing.
    /// Returns `TerrainError::NoPath` if no path exists at all.
    pub fn k_shortest_paths(
        &self,
        source: &str,
        target: &str,
        k: u32,
    ) -> Result<Vec<Vec<String>>, TerrainError> {
        use std::collections::HashSet;

        // Validate source and target exist
        let &ni_src =
            self.node_index_map
                .get(source)
                .ok_or_else(|| TerrainError::NodeNotFound {
                    id: source.to_string(),
                })?;
        let &ni_tgt =
            self.node_index_map
                .get(target)
                .ok_or_else(|| TerrainError::NodeNotFound {
                    id: target.to_string(),
                })?;

        let _ = (ni_src, ni_tgt); // validated above; astar uses the methods below

        let mut results: Vec<Vec<String>> = Vec::new();
        let mut seen_intermediate_sets: Vec<HashSet<String>> = Vec::new();

        // First path: standard Dijkstra
        if let Ok(path) = self.shortest_path(source, target) {
            let int_set: HashSet<String> = path[1..path.len().saturating_sub(1)]
                .iter()
                .cloned()
                .collect();
            seen_intermediate_sets.push(int_set);
            results.push(path);
        } else {
            return Err(TerrainError::NoPath {
                from: source.to_string(),
                to: target.to_string(),
            });
        }

        // Generate k-1 more paths by excluding one intermediate node from prior paths
        let mut exclusions: HashSet<String> = HashSet::new();
        while results.len() < k as usize {
            // Try excluding each intermediate node of the last result
            let last = results.last().unwrap().clone();
            let intermediates: Vec<String> = last[1..last.len().saturating_sub(1)]
                .iter()
                .filter(|n| !exclusions.contains(n.as_str()))
                .cloned()
                .collect();

            let mut found = false;
            for excl in intermediates {
                exclusions.insert(excl.clone());

                // Build a modified graph excluding this node
                // We use a two-phase approach: try all exclusions until a distinct path is found
                let candidate = self.shortest_path_excluding(source, target, &exclusions);
                if let Ok(path) = candidate {
                    let int_set: HashSet<String> = path[1..path.len().saturating_sub(1)]
                        .iter()
                        .cloned()
                        .collect();
                    // Only add if distinct from all prior paths
                    let is_new = seen_intermediate_sets.iter().all(|s| *s != int_set);
                    if is_new {
                        seen_intermediate_sets.push(int_set);
                        results.push(path);
                        found = true;
                        break;
                    }
                }
            }
            if !found {
                break; // No more distinct paths
            }
        }

        Ok(results)
    }

    /// Shortest path excluding a set of intermediate nodes.
    ///
    /// Uses Dijkstra but assigns infinite cost to excluded nodes, forcing the
    /// path to route around them. Source and target are never excluded.
    fn shortest_path_excluding(
        &self,
        source: &str,
        target: &str,
        excluded: &std::collections::HashSet<String>,
    ) -> Result<Vec<String>, TerrainError> {
        let &ni_src =
            self.node_index_map
                .get(source)
                .ok_or_else(|| TerrainError::NodeNotFound {
                    id: source.to_string(),
                })?;
        let &ni_tgt =
            self.node_index_map
                .get(target)
                .ok_or_else(|| TerrainError::NodeNotFound {
                    id: target.to_string(),
                })?;

        let result = astar(
            &self.graph,
            ni_src,
            |n| n == ni_tgt,
            |e| {
                // Use a very high cost for edges that touch excluded nodes (but not source/target)
                let (u, v) = (e.source(), e.target());
                let u_id = &self.graph[u].id;
                let v_id = &self.graph[v].id;
                let u_excl = excluded.contains(u_id.as_str()) && u_id != source && u_id != target;
                let v_excl = excluded.contains(v_id.as_str()) && v_id != source && v_id != target;
                if u_excl || v_excl {
                    f64::INFINITY
                } else {
                    e.weight().weight
                }
            },
            |_| 0.0,
        );

        match result {
            Some((_cost, path_indices)) => {
                let path: Vec<String> = path_indices
                    .iter()
                    .map(|&ni| self.graph[ni].id.clone())
                    .collect();
                // Verify no excluded intermediate nodes ended up in the path
                let has_excluded = path[1..path.len().saturating_sub(1)]
                    .iter()
                    .any(|n| excluded.contains(n.as_str()) && n != source && n != target);
                if has_excluded {
                    Err(TerrainError::NoPath {
                        from: source.to_string(),
                        to: target.to_string(),
                    })
                } else {
                    Ok(path)
                }
            }
            None => Err(TerrainError::NoPath {
                from: source.to_string(),
                to: target.to_string(),
            }),
        }
    }

    /// Expose the inner petgraph `UnGraph` for algorithm access (e.g., Steiner).
    pub(crate) fn inner_graph(&self) -> &UnGraph<NodeData, EdgeData, u32> {
        &self.graph
    }
}

impl std::fmt::Debug for TerrainGraph {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TerrainGraph")
            .field("node_count", &self.node_count())
            .field("edge_count", &self.edge_count())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terrain::TerrainModel;

    fn make_tiny_terrain() -> TerrainModel {
        // 4-point 2x2 flat terrain at z=100
        let pts: Vec<[f64; 3]> = vec![
            [0.0, 0.0, 100.0],
            [10.0, 0.0, 100.0],
            [0.0, 10.0, 100.0],
            [10.0, 10.0, 100.0],
        ];
        let mut m = TerrainModel::from_xyz_list(&pts).unwrap();
        m.build_grid(10.0).unwrap();
        m
    }

    #[test]
    fn tiny_grid_node_count() {
        let terrain = make_tiny_terrain();
        let mut g = TerrainGraph::new(CostFunction::default());
        g.from_grid(10.0, &terrain);
        // xs = arange(0, 10, 10) = [0, 10] → 2 values
        // ys = arange(0, 10, 10) = [0, 10] → 2 values
        // nodes = 2*2 = 4
        assert_eq!(g.node_count(), 4);
    }

    #[test]
    fn tiny_grid_edge_count() {
        let terrain = make_tiny_terrain();
        let mut g = TerrainGraph::new(CostFunction::default());
        g.from_grid(10.0, &terrain);
        // 4-neighbor forward: (0,1)+(1,0) = 1+2+1 = 4 edges
        // 2x2 grid: horizontal 2*(2-1)=2 + vertical (2-1)*2=2 = 4
        assert_eq!(g.edge_count(), 4);
    }

    #[test]
    fn node_ids_follow_counter_order() {
        let terrain = make_tiny_terrain();
        let mut g = TerrainGraph::new(CostFunction::default());
        g.from_grid(10.0, &terrain);
        assert!(g.node_index_map.contains_key("g0"));
        assert!(g.node_index_map.contains_key("g3"));
    }

    #[test]
    fn shortest_path_tiny() {
        let terrain = make_tiny_terrain();
        let mut g = TerrainGraph::new(CostFunction::default());
        g.from_grid(10.0, &terrain);
        let path = g.shortest_path("g0", "g3").unwrap();
        assert!(path.contains(&"g0".to_string()));
        assert!(path.contains(&"g3".to_string()));
    }

    #[test]
    fn perturb_variant_zero_noop() {
        let terrain = make_tiny_terrain();
        let mut g = TerrainGraph::new(CostFunction::default());
        g.from_grid(10.0, &terrain);
        let cost_before = g.shortest_path_cost("g0", "g3").unwrap();
        g.perturb_weights(0);
        let cost_after = g.shortest_path_cost("g0", "g3").unwrap();
        assert!((cost_before - cost_after).abs() < 1e-9);
    }
}
