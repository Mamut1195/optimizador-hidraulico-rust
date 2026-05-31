//! TerrainGraph — weighted 4-neighbor grid graph for pathfinding.
//!
//! Mirrors Python `hydro_engine.core.graph.TerrainGraph` exactly:
//! - Nodes: `g{counter}` IDs, counter increments i-outer, j-inner loop.
//! - Edges: 4-neighbor connections via `(0,1)` and `(1,0)` direction pairs only.
//! - Weights: `CostFunction::call(length, z_start, z_end)`.
//! - Dijkstra: petgraph `astar` with zero heuristic (exact Dijkstra).
//! - nearest_node: kiddo KDTree nearest_one on node XY.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet, VecDeque};

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
                // Use nearest-neighbor z for grid nodes to avoid bilinear interpolation
                // floating-point errors at exact grid points. For a terrain built from the
                // same grid coordinates, NN gives the exact stored z-value, matching Python's
                // oracle which computes z directly (100 + slope_x*x + slope_y*y).
                let z = terrain.elevation_at_nn(x, y);
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

    /// K shortest simple paths from source to target.
    ///
    /// Faithful port of Python `nx.shortest_simple_paths(G, source, target, weight="weight")`,
    /// which implements Yen's loopless k-shortest simple paths algorithm with a
    /// `PathBuffer` min-heap for deterministic tie-breaking (FIFO among equal-cost
    /// candidates, mirroring networkx `itertools.count()`).
    ///
    /// Returns up to `k` paths as `Vec<Vec<String>>` (node-ID lists), ordered by
    /// ascending path cost. If fewer than `k` simple paths exist, returns all of them.
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
        // Validate source/target exist (returns NodeNotFound early if not)
        self.node_index_map
            .get(source)
            .ok_or_else(|| TerrainError::NodeNotFound {
                id: source.to_string(),
            })?;
        self.node_index_map
            .get(target)
            .ok_or_else(|| TerrainError::NodeNotFound {
                id: target.to_string(),
            })?;

        if k == 0 {
            return Ok(Vec::new());
        }

        // PathBuffer: min-heap keyed by (cost_bits, counter, path).
        // `cost_bits` uses f64::to_bits with a total order via f64_total_order_bits.
        // `counter` is monotonically increasing — mirrors networkx itertools.count().
        // `seen` prevents pushing duplicate paths.
        struct PathBuffer {
            heap: BinaryHeap<Reverse<(u64, usize, Vec<String>)>>,
            seen: HashSet<Vec<String>>,
            counter: usize,
        }

        impl PathBuffer {
            fn new() -> Self {
                Self {
                    heap: BinaryHeap::new(),
                    seen: HashSet::new(),
                    counter: 0,
                }
            }

            fn push(&mut self, cost: f64, path: Vec<String>) {
                if self.seen.contains(&path) {
                    return;
                }
                self.seen.insert(path.clone());
                let key = f64_total_order_bits(cost);
                self.heap.push(Reverse((key, self.counter, path)));
                self.counter += 1;
            }

            fn pop(&mut self) -> Option<(f64, Vec<String>)> {
                if let Some(Reverse((key, _, path))) = self.heap.pop() {
                    // Remove from seen so the same path can't be re-pushed
                    self.seen.remove(&path);
                    let cost = f64_from_total_order_bits(key);
                    Some((cost, path))
                } else {
                    None
                }
            }
        }

        /// Convert f64 to a bit pattern that sorts correctly as u64.
        /// Handles NaN/inf like IEEE 754 total order: positives map to high values,
        /// negatives (which shouldn't appear in costs) map to low values.
        fn f64_total_order_bits(v: f64) -> u64 {
            let bits = v.to_bits();
            if bits >> 63 == 0 {
                // positive: set the sign bit so positive > negative
                bits | (1u64 << 63)
            } else {
                // negative: flip all bits
                !bits
            }
        }

        fn f64_from_total_order_bits(key: u64) -> f64 {
            // Reverse of f64_total_order_bits
            let bits = if key >> 63 != 0 {
                // was positive: clear the sign bit we set
                key & !(1u64 << 63)
            } else {
                // was negative: flip all bits back
                !key
            };
            f64::from_bits(bits)
        }

        let mut list_a: Vec<Vec<String>> = Vec::new(); // accepted paths in output order
        let mut buf: PathBuffer = PathBuffer::new();
        let mut prev_path: Option<Vec<String>> = None;

        loop {
            if list_a.len() == k as usize {
                break;
            }

            if let Some(p) = &prev_path {
                // Yen's spur phase: generate candidates from the last accepted path
                let p_len = p.len();
                let mut ignore_nodes: HashSet<String> = HashSet::new();

                // Spur index i: 1-based over prev_path (spur node = p[i-1], 0-indexed)
                // root = p[..i] (i nodes, last = p[i-1])
                // root_cost = cost of p[..i] prefix
                for i in 1..p_len {
                    let spur_node = &p[i - 1]; // 0-indexed: p[i-1] is the i-th node
                    let root = &p[..i];

                    // root_cost: sum of edges along root prefix
                    let root_cost = self.path_cost(root);

                    // Build ignore_edges: for each accepted path sharing root prefix,
                    // block the edge that diverges at position i
                    let mut ignore_edges: HashSet<(String, String)> = HashSet::new();
                    for accepted in &list_a {
                        if accepted.len() > i && accepted[..i] == *root {
                            // Block edge (accepted[i-1], accepted[i])
                            ignore_edges.insert((accepted[i - 1].clone(), accepted[i].clone()));
                        }
                    }

                    match self.dijkstra_spur(spur_node, target, &ignore_nodes, &ignore_edges) {
                        Ok((spur_cost, spur)) => {
                            // candidate = root[..i-1] + spur  (root[-1] == spur[0] == spur_node)
                            let mut candidate = root[..i - 1].to_vec();
                            candidate.extend(spur);
                            buf.push(root_cost + spur_cost, candidate);
                        }
                        Err(TerrainError::NoPath { .. }) => {}
                        Err(e) => return Err(e),
                    }

                    // Exclude spur_node for subsequent spur indices
                    ignore_nodes.insert(spur_node.clone());
                }
            } else {
                // First iteration: compute initial shortest path and seed the buffer
                match self.dijkstra_spur(source, target, &HashSet::new(), &HashSet::new()) {
                    Ok((cost, path)) => buf.push(cost, path),
                    Err(TerrainError::NoPath { .. }) => break,
                    Err(e) => return Err(e),
                }
            }

            // Pop lowest-cost candidate from PathBuffer
            if let Some((_cost, path)) = buf.pop() {
                prev_path = Some(path.clone());
                list_a.push(path);
            } else {
                break;
            }
        }

        if list_a.is_empty() {
            return Err(TerrainError::NoPath {
                from: source.to_string(),
                to: target.to_string(),
            });
        }

        Ok(list_a)
    }

    /// Compute the total `weight` cost of a path (sequence of node IDs).
    ///
    /// Panics if adjacent nodes have no edge (should not happen on a valid path).
    fn path_cost(&self, path: &[String]) -> f64 {
        path.windows(2)
            .map(|w| {
                self.edge_weight(&w[0], &w[1])
                    .unwrap_or_else(|| panic!("path_cost: no edge {} -> {}", w[0], w[1]))
            })
            .sum()
    }

    /// Dijkstra from `spur_node` to `target`, skipping `ignore_nodes` and `ignore_edges`.
    ///
    /// - `ignore_nodes`: nodes that must not be settled (expanded), except `spur_node`
    ///   and `target` which are always allowed.
    /// - `ignore_edges`: directed edges `(u, v)` to skip. Because the graph is
    ///   undirected, both `(u,v)` and `(v,u)` are blocked.
    ///
    /// Returns `(total_cost, path_ids)` or `TerrainError::NoPath`.
    fn dijkstra_spur(
        &self,
        spur_node: &str,
        target: &str,
        ignore_nodes: &HashSet<String>,
        ignore_edges: &HashSet<(String, String)>,
    ) -> Result<(f64, Vec<String>), TerrainError> {
        let &ni_src =
            self.node_index_map
                .get(spur_node)
                .ok_or_else(|| TerrainError::NodeNotFound {
                    id: spur_node.to_string(),
                })?;
        let &ni_tgt =
            self.node_index_map
                .get(target)
                .ok_or_else(|| TerrainError::NodeNotFound {
                    id: target.to_string(),
                })?;

        // Use petgraph astar with:
        // - node filter: skip ignored nodes (not source, not target)
        // - edge filter: skip ignored edges (both directions)
        // petgraph astar cannot filter nodes directly, so we encode ignored nodes as
        // infinite-cost edges: any edge to/from an ignored node gets cost +inf.
        // But astar still routes through them if there's no alternative using finite costs.
        // Instead, we implement a manual Dijkstra with explicit node/edge filtering.

        // Manual Dijkstra: dist[ni] = best known cost; heap = (Reverse(cost_bits), ni)
        let node_count = self.graph.node_count();
        let mut dist: HashMap<NodeIndex<u32>, f64> = HashMap::with_capacity(node_count);
        let mut prev: HashMap<NodeIndex<u32>, NodeIndex<u32>> = HashMap::with_capacity(node_count);
        // Min-heap: Reverse((cost_bits, NodeIndex raw))
        let mut heap: BinaryHeap<Reverse<(u64, u32)>> = BinaryHeap::new();

        dist.insert(ni_src, 0.0);
        heap.push(Reverse((0u64, ni_src.index() as u32)));

        while let Some(Reverse((cost_bits, u_raw))) = heap.pop() {
            let u = NodeIndex::new(u_raw as usize);
            let u_cost = f64::from_bits(cost_bits);

            if u == ni_tgt {
                // Reconstruct path
                let mut path: Vec<String> = Vec::new();
                let mut cur = ni_tgt;
                loop {
                    path.push(self.graph[cur].id.clone());
                    match prev.get(&cur) {
                        Some(&p) => cur = p,
                        None => break,
                    }
                }
                path.reverse();
                return Ok((u_cost, path));
            }

            // Skip if we already found a better path to u
            if let Some(&best) = dist.get(&u) {
                if u_cost > best + 1e-12 {
                    continue;
                }
            }

            let u_id = &self.graph[u].id;

            // Skip ignored nodes (but never skip spur_node itself, which is the source)
            if u != ni_src && ignore_nodes.contains(u_id.as_str()) {
                continue;
            }

            for e in self.graph.edges(u) {
                let v = if e.source() == u {
                    e.target()
                } else {
                    e.source()
                };
                let v_id = &self.graph[v].id;

                // Skip ignored nodes as neighbors
                if v != ni_src && v != ni_tgt && ignore_nodes.contains(v_id.as_str()) {
                    continue;
                }

                // Skip ignored edges (undirected: block both (u,v) and (v,u))
                let u_str = u_id.as_str();
                let v_str = v_id.as_str();
                if ignore_edges.contains(&(u_str.to_string(), v_str.to_string()))
                    || ignore_edges.contains(&(v_str.to_string(), u_str.to_string()))
                {
                    continue;
                }

                let edge_cost = e.weight().weight;
                let new_cost = u_cost + edge_cost;

                let better = dist.get(&v).is_none_or(|&d| new_cost < d - 1e-12);
                if better {
                    dist.insert(v, new_cost);
                    prev.insert(v, u);
                    heap.push(Reverse((new_cost.to_bits(), v.index() as u32)));
                }
            }
        }

        Err(TerrainError::NoPath {
            from: spur_node.to_string(),
            to: target.to_string(),
        })
    }

    /// Return the edge weight between two nodes, or `None` if no edge exists.
    ///
    /// Searches all edges incident to `u` for one connecting to `v`.
    /// For a 4-neighbor grid graph this is O(degree) = O(4).
    pub fn edge_weight(&self, u: &str, v: &str) -> Option<f64> {
        let &ni_u = self.node_index_map.get(u)?;
        let &ni_v = self.node_index_map.get(v)?;
        self.graph
            .edges(ni_u)
            .find(|e| e.target() == ni_v || e.source() == ni_v)
            .map(|e| e.weight().weight)
    }

    /// Expose the inner petgraph `UnGraph` for algorithm access (e.g., Steiner).
    pub(crate) fn inner_graph(&self) -> &UnGraph<NodeData, EdgeData, u32> {
        &self.graph
    }

    /// Compute the minimum spanning tree of the given node subset.
    ///
    /// Mirrors Python `nx.minimum_spanning_tree(graph.nx_graph.subgraph(reachable), weight="weight")`.
    ///
    /// Returns the MST as an undirected adjacency map:
    ///   `node_id → [(neighbor_id, edge_weight)]`
    ///
    /// The subset is the set of all nodes reachable from `source` in the graph
    /// (Python uses `nx.node_connected_component(graph.nx_graph, source_node)`).
    /// Since `TerrainGraph` is always connected (all grid edges are added), the
    /// subset is all graph nodes. Callers may pass the full node set.
    ///
    /// Uses Prim's algorithm for correctness and determinism:
    ///   - Tie-breaking by (weight, node_id) ensures the same MST as Python's
    ///     NetworkX (which uses a Fibonacci-heap but is deterministic on integers).
    ///
    /// # Errors
    ///
    /// Returns `TerrainError::NodeNotFound` if `source` is not in the graph.
    /// Returns `TerrainError::EmptyGraph` if the graph has no nodes.
    pub fn minimum_spanning_tree(
        &self,
        source: &str,
    ) -> Result<HashMap<String, Vec<(String, f64)>>, TerrainError> {
        if self.node_index_map.is_empty() {
            return Err(TerrainError::EmptyGraph);
        }
        let &ni_src =
            self.node_index_map
                .get(source)
                .ok_or_else(|| TerrainError::NodeNotFound {
                    id: source.to_string(),
                })?;

        // Prim's algorithm with a min-heap keyed by (weight_bits, node_id, from_id)
        // tie-break by (weight, node_id) for determinism matching networkx
        let mut in_mst: HashSet<NodeIndex<u32>> = HashSet::new();
        let mut adj: HashMap<String, Vec<(String, f64)>> = HashMap::new();

        // Initialize all nodes in the adjacency map
        for ni in self.graph.node_indices() {
            adj.entry(self.graph[ni].id.clone()).or_default();
        }

        // heap entries: Reverse((weight_bits, to_id_str, from_id_str, to_ni, from_ni))
        #[allow(clippy::type_complexity)]
        let mut heap: BinaryHeap<Reverse<(u64, String, String, u32, u32)>> = BinaryHeap::new();

        in_mst.insert(ni_src);
        // Seed with all edges from source
        for e in self.graph.edges(ni_src) {
            let v = if e.source() == ni_src {
                e.target()
            } else {
                e.source()
            };
            let w = e.weight().weight;
            heap.push(Reverse((
                w.to_bits(),
                self.graph[v].id.clone(),
                self.graph[ni_src].id.clone(),
                v.index() as u32,
                ni_src.index() as u32,
            )));
        }

        while let Some(Reverse((w_bits, to_id, from_id, to_raw, _from_raw))) = heap.pop() {
            let v = NodeIndex::new(to_raw as usize);
            if in_mst.contains(&v) {
                continue;
            }
            let w = f64::from_bits(w_bits);
            in_mst.insert(v);

            // Record undirected MST edge
            adj.entry(from_id.clone())
                .or_default()
                .push((to_id.clone(), w));
            adj.entry(to_id.clone())
                .or_default()
                .push((from_id.clone(), w));

            // Expand new MST node
            for e in self.graph.edges(v) {
                let u = if e.source() == v {
                    e.target()
                } else {
                    e.source()
                };
                if !in_mst.contains(&u) {
                    let ew = e.weight().weight;
                    heap.push(Reverse((
                        ew.to_bits(),
                        self.graph[u].id.clone(),
                        self.graph[v].id.clone(),
                        u.index() as u32,
                        v.index() as u32,
                    )));
                }
            }
        }

        Ok(adj)
    }

    /// Return all node IDs reachable from `source` in the undirected graph
    /// (BFS connected component).
    ///
    /// Mirrors Python `nx.node_connected_component(graph.nx_graph, source_node)`.
    pub fn connected_component(&self, source: &str) -> Result<HashSet<String>, TerrainError> {
        let &ni_src =
            self.node_index_map
                .get(source)
                .ok_or_else(|| TerrainError::NodeNotFound {
                    id: source.to_string(),
                })?;

        let mut visited: HashSet<NodeIndex<u32>> = HashSet::new();
        let mut queue: VecDeque<NodeIndex<u32>> = VecDeque::new();
        queue.push_back(ni_src);
        visited.insert(ni_src);

        while let Some(u) = queue.pop_front() {
            for e in self.graph.edges(u) {
                let v = if e.source() == u {
                    e.target()
                } else {
                    e.source()
                };
                if !visited.contains(&v) {
                    visited.insert(v);
                    queue.push_back(v);
                }
            }
        }

        Ok(visited
            .into_iter()
            .map(|ni| self.graph[ni].id.clone())
            .collect())
    }

    /// Return all graph edges as (from_id, to_id, weight, length) tuples.
    ///
    /// Used by WaterSupplySolver to find loop candidates (edges not in MST).
    pub fn all_edges(&self) -> Vec<(String, String, f64, f64)> {
        self.graph
            .edge_indices()
            .map(|ei| {
                let (a, b) = self.graph.edge_endpoints(ei).unwrap();
                let ed = &self.graph[ei];
                (
                    self.graph[a].id.clone(),
                    self.graph[b].id.clone(),
                    ed.weight,
                    ed.length,
                )
            })
            .collect()
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
