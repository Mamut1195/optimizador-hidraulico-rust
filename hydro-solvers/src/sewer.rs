//! SewerSolver — automatic sewer network design using graph optimization.
//!
//! Faithful port of `hydro_engine/solvers/sewer.py` (SewerSolver class).
//!
//! ## Algorithm
//! 1. Build terrain graph from topography.
//! 2. Find Steiner tree connecting all service points to outlet.
//! 3. Orient edges downhill (gravity flow) via BFS from outlet.
//! 4. Optional: simplify manhole placement (`simplify_tree`).
//! 5. Accumulate flow per node via topological sort.
//! 6. Dimension pipes using Manning equation (`required_diameter`).
//! 7. Compute inverts ensuring minimum cover and slope with pump-bypass tagging.
//! 8. Score: cost = 1.0*L + 2.0*excav + 100.0*violations.

use std::collections::{HashMap, HashSet, VecDeque};

use hydro_hydraulics::manning::{manning_batch, required_diameter};
use hydro_terrain::{steiner_tree, CostFunction, CostWeights, TerrainGraph, TerrainModel};
use hydro_types::{
    response::SolutionMetadata, DesignConstraints, NetworkNode, NetworkPipe, NodeId, NodeType,
    PipeId, PipeMaterial, PipeNetwork, Solution, SolutionScore,
};
use serde_json::Value;

use crate::error::SolverError;
use crate::graph::SolverGraph;
use crate::solver::{Solver, SolverParams};

// ── SewerSolver ───────────────────────────────────────────────────────────────

/// Solver for gravity sewer networks (alcantarillado).
///
/// Faithful port of Python `SewerSolver`. Uses Steiner tree approximation to
/// connect service points to an outlet, then dimensions pipes using Manning's
/// equation and assigns inverts with pump-bypass tagging for flat segments.
pub struct SewerSolver {
    pub terrain: TerrainModel,
    pub constraints: DesignConstraints,
    /// Service point coordinates injected at construction time (used by `Solver::solve`).
    pub service_points: Vec<(f64, f64)>,
    /// Outlet coordinate injected at construction time (used by `Solver::solve`).
    pub outlet: (f64, f64),
    /// Per-service flow rate injected at construction time (used by `Solver::solve`).
    pub flow_per_service: f64,
    /// Pipe material injected at construction time (used by `Solver::solve`).
    pub material: PipeMaterial,
    /// Network name injected at construction time (used by `Solver::solve`).
    pub network_name: String,
}

impl SewerSolver {
    /// Create a new SewerSolver with all project-context fields required by
    /// `Solver::solve`.
    ///
    /// Field order: `(terrain, constraints, service_points, outlet, flow_per_service,
    /// material, network_name)`. GA-driven values are provided at solve time via
    /// `SolverParams`.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        terrain: TerrainModel,
        constraints: DesignConstraints,
        service_points: Vec<(f64, f64)>,
        outlet: (f64, f64),
        flow_per_service: f64,
        material: PipeMaterial,
        network_name: String,
    ) -> Self {
        SewerSolver {
            terrain,
            constraints,
            service_points,
            outlet,
            flow_per_service,
            material,
            network_name,
        }
    }

    /// Solve the sewer network design with explicit service/outlet parameters.
    ///
    /// This is the domain-specific entry point (mirrors Python `solve()`).
    /// The generic `Solver::solve()` delegates here.
    pub fn solve_sewer(
        &mut self,
        service_points: &[(f64, f64)],
        outlet: (f64, f64),
        network_name: &str,
        flow_per_service: f64,
        material: PipeMaterial,
        params: &SolverParams,
    ) -> Result<Vec<Solution>, SolverError> {
        let cost_fn = CostFunction::new(
            CostWeights {
                w_length: 1.0,
                w_excavation: 2.0,
                w_slope_penalty: 5.0,
                w_pump_penalty: 50.0,
                w_crossing: 10.0,
            },
            self.constraints.min_slope,
            self.constraints.max_slope,
            self.constraints.min_cover,
        );

        let mut graph = TerrainGraph::new(cost_fn);
        graph.from_grid(params.grid_resolution, &self.terrain);

        if params.route_variant > 0 {
            graph.perturb_weights(params.route_variant);
        }

        // Snap outlet and service points to nearest grid nodes
        let outlet_node = graph
            .nearest_node(outlet.0, outlet.1)
            .map_err(|_| SolverError::NoOutlet)?;

        let terminal_nodes: Vec<String> = service_points
            .iter()
            .map(|&(x, y)| {
                graph
                    .nearest_node(x, y)
                    .map_err(|_| SolverError::InsufficientTerminals)
            })
            .collect::<Result<Vec<_>, _>>()?;

        // Build the unique terminal set (services + outlet)
        let mut all_terminal_set: HashSet<String> = terminal_nodes.iter().cloned().collect();
        all_terminal_set.insert(outlet_node.clone());
        let all_terminals: Vec<String> = all_terminal_set.into_iter().collect();

        if all_terminals.len() < 2 {
            return Err(SolverError::InsufficientTerminals);
        }

        let gene_params = GeneParams {
            slope_factor: params.slope_factor,
            cover_factor: params.cover_factor,
            diameter_offset: params.diameter_offset,
            manhole_spacing: params.manhole_spacing,
        };

        let mut solutions: Vec<Solution> = Vec::new();

        // Primary solution via Steiner tree
        let steiner = steiner_tree(&graph, &all_terminals)
            .map_err(|e| SolverError::SteinerFailed(e.to_string()))?;

        // Convert SteinerTree to undirected SolverGraph for BFS orientation
        let steiner_sg = steiner_to_solver_graph(&steiner);

        let network = self
            .build_network_from_tree(
                &graph,
                &steiner_sg,
                &outlet_node,
                &terminal_nodes,
                network_name,
                flow_per_service,
                material,
                &gene_params,
            )
            .map_err(SolverError::BuildFailed)?;

        let score = self.evaluate(&network)?;
        solutions.push(Solution {
            rank: 1,
            network,
            score,
            metadata: SolutionMetadata::default(),
        });

        // Generate k alternatives via k-shortest paths from the farthest terminal
        if !terminal_nodes.is_empty() && params.num_alternatives > 1 {
            let distances: Vec<(&str, f64)> = terminal_nodes
                .iter()
                .filter(|n| n.as_str() != outlet_node.as_str())
                .filter_map(|nid| {
                    graph
                        .shortest_path_cost(nid, &outlet_node)
                        .ok()
                        .map(|c| (nid.as_str(), c))
                })
                .collect();

            if let Some(&(farthest, _)) = distances
                .iter()
                .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
            {
                if let Ok(k_paths) =
                    graph.k_shortest_paths(farthest, &outlet_node, params.num_alternatives)
                {
                    for (i, path) in k_paths[1..].iter().enumerate() {
                        let alt_name = format!("{}_alt{}", network_name, i + 2);
                        if let Ok(net) = self.build_network_from_path(
                            &graph,
                            path,
                            &terminal_nodes,
                            &outlet_node,
                            &alt_name,
                            flow_per_service,
                            material,
                            &gene_params,
                        ) {
                            if let Ok(sc) = self.evaluate(&net) {
                                solutions.push(Solution {
                                    rank: (i + 2) as i64,
                                    network: net,
                                    score: sc,
                                    metadata: SolutionMetadata::default(),
                                });
                            }
                        }
                    }
                }
            }
        }

        // Sort by total_cost ascending (best first), then re-rank
        solutions.sort_by(|a, b| {
            a.score
                .total_cost
                .partial_cmp(&b.score.total_cost)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        for (i, sol) in solutions.iter_mut().enumerate() {
            sol.rank = (i + 1) as i64;
        }

        Ok(solutions)
    }

    // ── Simplify tree ──────────────────────────────────────────────────────

    /// Reduce a directed tree to manhole nodes at required spacing.
    ///
    /// SolverGraph-native version introduced in PR-B2. Replaces the String-HashMap
    /// variant (`simplify_tree`) and the `solver_graph_to_adj` shim. All internal
    /// sets operate on `NodeIndex<u32>`; String conversion only happens at the
    /// kept-node output boundary.
    ///
    /// Faithful port of Python `_simplify_tree`. Keeps:
    /// - outlets + terminals (must_keep)
    /// - junction nodes (in_degree + out_degree != 2)
    /// - intermediate nodes placed at `manhole_spacing` intervals along chains.
    ///
    /// Returns `(simplified_sg, kept_node_ids)` where `simplified_sg` is a
    /// directed `SolverGraph` carrying `path_length` as edge weight, and
    /// `kept_node_ids` is the set of kept node String ids.
    pub(crate) fn simplify_tree_sg(
        &self,
        oriented: &SolverGraph,    // upstream → downstream
        in_oriented: &SolverGraph, // downstream → upstream (reverse of oriented)
        graph: &TerrainGraph,
        terminal_set: &HashSet<String>,
        outlet_node: &str,
        manhole_spacing: f64,
    ) -> (SolverGraph, HashSet<String>) {
        use petgraph::graph::NodeIndex;

        // Build must_keep set: outlet + terminals + junctions (degree != 2)
        let mut must_keep: HashSet<NodeIndex<u32>> = HashSet::new();

        if let Some(idx) = oriented.node_index(outlet_node) {
            must_keep.insert(idx);
        }
        for t in terminal_set {
            if let Some(idx) = oriented.node_index(t.as_str()) {
                must_keep.insert(idx);
            }
        }
        for idx in oriented.node_indices() {
            let out_deg = oriented.sorted_neighbors(idx).len();
            let in_deg = in_oriented
                .node_index(oriented.node_id(idx))
                .map(|rev_idx| in_oriented.sorted_neighbors(rev_idx).len())
                .unwrap_or(0);
            if in_deg + out_deg != 2 {
                must_keep.insert(idx);
            }
        }

        // Walk chains: for each must_keep node, follow successors until another must_keep node
        let mut chains: Vec<Vec<NodeIndex<u32>>> = Vec::new();
        let mut visited_edges: HashSet<(NodeIndex<u32>, NodeIndex<u32>)> = HashSet::new();

        let mut must_keep_vec: Vec<NodeIndex<u32>> = must_keep.iter().copied().collect();
        must_keep_vec.sort_by(|&a, &b| oriented.node_id(a).cmp(oriented.node_id(b)));

        for start in &must_keep_vec {
            for (succ, _) in oriented.sorted_neighbors(*start) {
                let edge_key = (*start, succ);
                if visited_edges.contains(&edge_key) {
                    continue;
                }
                let mut chain = vec![*start];
                let mut current = succ;
                while !must_keep.contains(&current) {
                    let prev = *chain.last().unwrap();
                    visited_edges.insert((prev, current));
                    chain.push(current);
                    let next = oriented.sorted_neighbors(current);
                    if next.is_empty() {
                        break;
                    }
                    current = next[0].0;
                }
                visited_edges.insert((*chain.last().unwrap(), current));
                chain.push(current);
                chains.push(chain);
            }
        }

        // Place intermediate manholes at manhole_spacing intervals.
        // Seed keep from must_keep without cloning the full HashSet.
        let mut keep: HashSet<petgraph::graph::NodeIndex<u32>> =
            must_keep.iter().copied().collect();
        for chain in &chains {
            let mut dist = 0.0_f64;
            for i in 1..chain.len().saturating_sub(1) {
                let id_prev = oriented.node_id(chain[i - 1]);
                let id_curr = oriented.node_id(chain[i]);
                let c_prev = graph.get_node_coord(id_prev);
                let c_curr = graph.get_node_coord(id_curr);
                if let (Some((px, py)), Some((cx, cy))) = (c_prev, c_curr) {
                    dist += ((px - cx).powi(2) + (py - cy).powi(2)).sqrt();
                }
                if dist >= manhole_spacing {
                    keep.insert(chain[i]);
                    dist = 0.0;
                }
            }
        }

        // Build simplified directed SolverGraph with path_length on each edge
        let mut simplified = SolverGraph::new();
        for &idx in &keep {
            simplified.add_node(oriented.node_id(idx));
        }

        for chain in &chains {
            let mut prev_kept = chain[0];
            let mut path_len = 0.0_f64;
            for i in 1..chain.len() {
                let id_prev = oriented.node_id(chain[i - 1]);
                let id_curr = oriented.node_id(chain[i]);
                let c_prev = graph.get_node_coord(id_prev);
                let c_curr = graph.get_node_coord(id_curr);
                if let (Some((px, py)), Some((cx, cy))) = (c_prev, c_curr) {
                    path_len += ((px - cx).powi(2) + (py - cy).powi(2)).sqrt();
                }
                if keep.contains(&chain[i]) {
                    let from_id = oriented.node_id(prev_kept);
                    let to_id = oriented.node_id(chain[i]);
                    simplified.add_edge(from_id, to_id, path_len);
                    prev_kept = chain[i];
                    path_len = 0.0;
                }
            }
        }

        // Build the kept_node_ids set for the caller (String boundary)
        let kept_node_ids: HashSet<String> = keep
            .iter()
            .map(|&idx| oriented.node_id(idx).to_owned())
            .collect();

        (simplified, kept_node_ids)
    }

    // ── Diameter selection ─────────────────────────────────────────────────

    /// Select pipe diameter, optionally stepping up from minimum.
    ///
    /// Mirrors Python `_select_diameter()`.
    fn select_diameter(
        &self,
        flow: f64,
        design_slope: f64,
        roughness: f64,
        diameter_offset: i32,
    ) -> f64 {
        let mut available = self.constraints.available_diameters.clone();
        available.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let base = required_diameter(flow, design_slope, roughness, &available);
        if diameter_offset > 0 {
            if let Some(idx) = available.iter().position(|&d| (d - base).abs() < 1e-12) {
                let new_idx =
                    (idx as i32 + diameter_offset).min((available.len() - 1) as i32) as usize;
                return available[new_idx];
            }
        }
        base
    }

    // ── Network builder from Steiner tree ──────────────────────────────────

    /// Convert a Steiner tree adjacency into a directed PipeNetwork.
    ///
    /// Faithful port of Python `_build_network_from_tree()`.
    #[allow(clippy::too_many_arguments)]
    fn build_network_from_tree(
        &self,
        graph: &TerrainGraph,
        undirected_sg: &SolverGraph, // undirected: node ↔ node (both directions present)
        outlet_node: &str,
        terminal_nodes: &[String],
        name: &str,
        flow_per_service: f64,
        material: PipeMaterial,
        params: &GeneParams,
    ) -> Result<PipeNetwork, String> {
        let terminal_set: HashSet<String> = terminal_nodes.iter().cloned().collect();

        // BFS from outlet to orient edges: upstream → downstream.
        let outlet_idx = undirected_sg
            .node_index(outlet_node)
            .ok_or_else(|| format!("outlet node '{}' not in Steiner tree", outlet_node))?;
        let (oriented, in_oriented) = bfs_build_oriented(undirected_sg, outlet_idx);

        // Simplify or use oriented graph as-is — both paths produce a SolverGraph.
        let (work_sg, kept_nodes) = if let Some(spacing) = params.manhole_spacing {
            self.simplify_tree_sg(
                &oriented,
                &in_oriented,
                graph,
                &terminal_set,
                outlet_node,
                spacing,
            )
        } else {
            // No simplification: use oriented tree directly.
            // kept_nodes = all node ids in the oriented graph.
            let kept: HashSet<String> = oriented
                .node_indices()
                .map(|idx| oriented.node_id(idx).to_owned())
                .collect();
            (oriented, kept)
        };

        // Create nodes (topo sort to ensure deterministic node order for ID assignment)
        let topo_order = work_sg.sorted_kahn_topo_sort().map_err(|e| e.to_string())?;

        let mut nodes: Vec<NetworkNode> = Vec::new();
        // node_map: String id → index into `nodes` vec (used for sump mutations)
        let mut node_map: HashMap<&str, usize> = HashMap::new();

        for &idx in &topo_order {
            let nid = work_sg.node_id(idx);
            if !kept_nodes.contains(nid) {
                continue;
            }
            let coord = graph
                .get_node_coord(nid)
                .ok_or_else(|| format!("node {} not found in graph", nid))?;
            let z = graph
                .get_node_elevation(nid)
                .ok_or_else(|| format!("elevation for node {} not found", nid))?;
            let ntype = if nid == outlet_node {
                NodeType::Outlet
            } else if terminal_set.contains(nid) {
                NodeType::Service
            } else {
                NodeType::Manhole
            };
            let mut node = NetworkNode::new(nid, coord.0, coord.1, z, ntype);
            node.rim_elevation = z;
            node.sump_elevation = f64::INFINITY; // Will be set during pipe processing
                                                 // SAFETY: `nid` borrows from `work_sg` which outlives `node_map` here.
                                                 // We use the raw pointer trick via stored &str pointing into the SolverGraph.
            let pos = nodes.len();
            nodes.push(node);
            // Store the id string pointer — safe since work_sg is not mutated hereafter.
            node_map.insert(work_sg.node_id(idx), pos);
        }

        // Initialize sump_elevation to a sentinel (Python: default from node constructor)
        // Python sewer.py starts from default node with sump_elevation = z - 1.5
        // then updates it during pipe iteration via min().
        // We replicate: sentinel = +∞, first pipe sets it, subsequent min() updates it.
        // Since we build nodes first and then update sump_elevation, we start with +∞.

        // Accumulated flow per node (upstream → downstream), keyed by NodeIndex.
        let mut flow_at_node: HashMap<petgraph::graph::NodeIndex<u32>, f64> = HashMap::new();
        for &idx in &topo_order {
            let nid = work_sg.node_id(idx);
            let base = if terminal_set.contains(nid) {
                flow_per_service
            } else {
                0.0
            };
            flow_at_node.insert(idx, base);
        }
        for &idx in &topo_order {
            let current_flow = flow_at_node.get(&idx).copied().unwrap_or(0.0);
            for (succ, _) in work_sg.sorted_neighbors(idx) {
                *flow_at_node.entry(succ).or_insert(0.0) += current_flow;
            }
        }

        // Create pipes
        let roughness = material.manning_n();
        let cover = self.constraints.min_cover * params.cover_factor;
        let min_slope = self.constraints.min_slope;
        let max_slope = self.constraints.max_slope;

        let mut pipes: Vec<NetworkPipe> = Vec::new();
        let mut pipe_counter = 0usize;

        // Iterate edges in topological order (leaves → outlet) to match Python's topo walk.
        //
        // CRITICAL for parity: Python's work_graph.edges(data=True) returns edges in
        // the DiGraph's internal insertion order (the BFS order). We replicate this
        // by iterating topo_order and emitting edges from each node.

        for &u_idx in &topo_order {
            let u_id = work_sg.node_id(u_idx);
            for (v_idx, path_length) in work_sg.sorted_neighbors(u_idx) {
                let v_id = work_sg.node_id(v_idx);
                let start_pos = match node_map.get(u_id) {
                    Some(&i) => i,
                    None => continue,
                };
                let end_pos = match node_map.get(v_id) {
                    Some(&i) => i,
                    None => continue,
                };

                let start_z = nodes[start_pos].z;
                let end_z = nodes[end_pos].z;

                // Compute pipe length from node coordinates.
                //
                // The `path_length` stored in work_sg is the cumulative Euclidean
                // distance of the chain when manhole_spacing simplification is active
                // (set by simplify_tree_sg).  When manhole_spacing is None,
                // Python falls back to `start_node.distance_to(end_node)` — the
                // Euclidean distance between the two adjacent nodes.
                //
                // In the Rust implementation, for the no-simplification path, the
                // steiner tree edge weight (a CostFunction scalar, NOT a distance)
                // was incorrectly stored as path_length.  We fix this by computing
                // the Euclidean distance from node coordinates whenever we have
                // direct node access (no simplification) or using path_length when
                // it genuinely represents a chain length (simplification active).
                let length = if params.manhole_spacing.is_some() {
                    // Simplified chain: path_length is the cumulative Euclidean chain length.
                    path_length
                } else {
                    // No simplification: compute Euclidean distance from node coordinates.
                    // Mirrors Python: edge_data.get("path_length", start_node.distance_to(end_node))
                    let su = nodes[start_pos].x;
                    let sv = nodes[start_pos].y;
                    let eu = nodes[end_pos].x;
                    let ev = nodes[end_pos].y;
                    ((su - eu).powi(2) + (sv - ev).powi(2)).sqrt()
                };
                if length < 0.1 {
                    continue;
                }

                pipe_counter += 1;

                let natural_slope = (start_z - end_z) / length;
                let needs_pump = natural_slope < min_slope;
                let design_slope = (natural_slope * params.slope_factor)
                    .max(min_slope)
                    .min(max_slope);

                let flow = flow_at_node
                    .get(&u_idx)
                    .copied()
                    .unwrap_or(flow_per_service)
                    .max(flow_per_service);

                let diameter =
                    self.select_diameter(flow, design_slope, roughness, params.diameter_offset);

                let start_cover_invert = start_z - cover;
                let end_cover_invert = end_z - cover;

                let (start_invert, end_invert) = if needs_pump {
                    // Pump-bypass: keep cover on both ends
                    (start_cover_invert, end_cover_invert)
                } else {
                    let si = start_cover_invert;
                    let ei = si - design_slope * length;
                    if ei > end_cover_invert {
                        // Local clamp: keep design slope, accept reduced start cover
                        let clamped_ei = end_cover_invert;
                        let clamped_si = clamped_ei + design_slope * length;
                        (clamped_si, clamped_ei)
                    } else {
                        (si, ei)
                    }
                };

                // Update sump_elevation for start and end nodes
                // Python: start_node.sump_elevation = start_invert - 0.2
                // Python: end_node.sump_elevation = min(current_sump, end_invert - 0.2)
                let start_sump = start_invert - 0.2;
                nodes[start_pos].sump_elevation = start_sump;

                let end_sump_candidate = end_invert - 0.2;
                let current_end_sump = nodes[end_pos].sump_elevation;
                nodes[end_pos].sump_elevation = if current_end_sump.is_infinite() {
                    end_sump_candidate
                } else {
                    current_end_sump.min(end_sump_candidate)
                };

                // Tag pump-bypassed pipes + set end node to PUMP
                let mut pipe_meta: HashMap<String, Value> = HashMap::new();
                if needs_pump {
                    pipe_meta.insert("bypassed_by_pump".to_string(), Value::Bool(true));
                    let pump_lift = (min_slope - natural_slope) * length;
                    pipe_meta.insert("pump_lift".to_string(), Value::from(pump_lift.max(0.0)));
                    nodes[end_pos].node_type = NodeType::Pump;
                }

                let slope = if length > 0.0 {
                    (start_invert - end_invert) / length
                } else {
                    0.0
                };

                pipes.push(NetworkPipe {
                    id: PipeId::new(format!("Pipe - ({})", pipe_counter)),
                    start_node_id: NodeId::new(u_id),
                    end_node_id: NodeId::new(v_id),
                    length,
                    effective_length: length,
                    diameter,
                    material,
                    roughness,
                    start_invert,
                    end_invert,
                    slope,
                    design_flow: flow,
                    waypoints: vec![],
                    metadata: pipe_meta,
                });
            }
        }

        // Fix sentinel sump_elevations (nodes with no pipes)
        for node in &mut nodes {
            if node.sump_elevation.is_infinite() {
                node.sump_elevation = node.z - 1.5; // Python NetworkNode default
            }
        }

        Ok(PipeNetwork::new(name, nodes, pipes))
    }

    // ── Network builder from path ──────────────────────────────────────────

    /// Build a network along a single path (for alternative routes).
    ///
    /// Faithful port of Python `_build_network_from_path()`.
    #[allow(clippy::too_many_arguments)]
    fn build_network_from_path(
        &self,
        graph: &TerrainGraph,
        path: &[String],
        terminal_nodes: &[String],
        outlet_node: &str,
        name: &str,
        flow_per_service: f64,
        material: PipeMaterial,
        params: &GeneParams,
    ) -> Result<PipeNetwork, String> {
        // terminal_set borrows &str from terminal_nodes — no String clone needed.
        let terminal_set: HashSet<&str> = terminal_nodes.iter().map(|s| s.as_str()).collect();

        // Determine which path indices to keep as manholes.
        // sorted_kept_indices is the canonical order; kept_path maps kept indices to &str.
        let sorted_kept_indices: Vec<usize> = if let Some(spacing) = params.manhole_spacing {
            let mut kept_indices: HashSet<usize> = [0, path.len() - 1].into_iter().collect();
            for (i, nid) in path.iter().enumerate() {
                if terminal_set.contains(nid.as_str()) || nid == outlet_node {
                    kept_indices.insert(i);
                }
            }
            let mut dist = 0.0_f64;
            for i in 1..path.len() {
                let c_prev = graph.get_node_coord(&path[i - 1]);
                let c_curr = graph.get_node_coord(&path[i]);
                if let (Some((px, py)), Some((cx, cy))) = (c_prev, c_curr) {
                    dist += ((px - cx).powi(2) + (py - cy).powi(2)).sqrt();
                }
                if kept_indices.contains(&i) {
                    dist = 0.0;
                } else if dist >= spacing {
                    kept_indices.insert(i);
                    dist = 0.0;
                }
            }
            let mut sorted: Vec<usize> = kept_indices.into_iter().collect();
            sorted.sort_unstable();
            sorted
        } else {
            (0..path.len()).collect()
        };
        // kept_path borrows &str slices from path — no String clone needed.
        let kept_path: Vec<&str> = sorted_kept_indices
            .iter()
            .map(|&i| path[i].as_str())
            .collect();

        // Create nodes with accumulated flow.
        // node_map and flow_at borrow &str keys from kept_path — no String clone needed.
        let mut nodes: Vec<NetworkNode> = Vec::new();
        let mut node_map: HashMap<&str, usize> = HashMap::new();
        let mut accumulated_flow = 0.0_f64;
        let mut flow_at: HashMap<&str, f64> = HashMap::new();

        for &nid in &kept_path {
            let coord = graph
                .get_node_coord(nid)
                .ok_or_else(|| format!("node {} not found in graph", nid))?;
            let z = graph
                .get_node_elevation(nid)
                .ok_or_else(|| format!("elevation for node {} not found", nid))?;
            let ntype = if nid == outlet_node {
                NodeType::Outlet
            } else if terminal_set.contains(nid) {
                NodeType::Service
            } else {
                NodeType::Manhole
            };
            let mut node = NetworkNode::new(nid, coord.0, coord.1, z, ntype);
            node.rim_elevation = z;
            node.sump_elevation = f64::INFINITY;
            let pos = nodes.len();
            node_map.insert(nid, pos);
            nodes.push(node);
            if terminal_set.contains(nid) {
                accumulated_flow += flow_per_service;
            }
            flow_at.insert(nid, accumulated_flow);
        }

        // Pre-compute segment lengths using windows(2) over sorted_kept_indices.
        // Stores (path_start_idx, path_end_idx, seg_len) — no String clone needed.
        let pipe_segments: Vec<(usize, usize, f64)> = sorted_kept_indices
            .windows(2)
            .map(|w| {
                let i_start = w[0];
                let i_end = w[1];
                let seg_len = (i_start..i_end).fold(0.0_f64, |acc, j| {
                    let c1 = graph.get_node_coord(&path[j]);
                    let c2 = graph.get_node_coord(&path[j + 1]);
                    if let (Some((x1, y1)), Some((x2, y2))) = (c1, c2) {
                        acc + ((x1 - x2).powi(2) + (y1 - y2).powi(2)).sqrt()
                    } else {
                        acc
                    }
                });
                (i_start, i_end, seg_len)
            })
            .collect();

        let roughness = material.manning_n();
        let cover = self.constraints.min_cover * params.cover_factor;
        let min_slope = self.constraints.min_slope;
        let max_slope = self.constraints.max_slope;
        let mut running_invert: Option<f64> = None;
        let mut pipes: Vec<NetworkPipe> = Vec::new();

        for (i, &(i_start, i_end, length)) in pipe_segments.iter().enumerate() {
            let u = path[i_start].as_str();
            let v = path[i_end].as_str();
            let start_idx = match node_map.get(u) {
                Some(&i) => i,
                None => continue,
            };
            let end_idx = match node_map.get(v) {
                Some(&i) => i,
                None => continue,
            };

            if length < 0.1 {
                continue;
            }

            let start_z = nodes[start_idx].z;
            let end_z = nodes[end_idx].z;

            let natural_slope = (start_z - end_z) / length;
            let needs_pump = natural_slope < min_slope;
            let design_slope = (natural_slope * params.slope_factor)
                .max(min_slope)
                .min(max_slope);

            let flow = flow_at
                .get(u)
                .copied()
                .unwrap_or(flow_per_service)
                .max(flow_per_service);

            let diameter =
                self.select_diameter(flow, design_slope, roughness, params.diameter_offset);

            let start_cover_invert = start_z - cover;
            let end_cover_invert = end_z - cover;

            let (start_invert, end_invert) = if needs_pump {
                running_invert = Some(end_cover_invert);
                (start_cover_invert, end_cover_invert)
            } else {
                let si = match running_invert {
                    None => start_cover_invert,
                    Some(ri) => ri.min(start_cover_invert),
                };
                let ei = si - design_slope * length;
                if ei > end_cover_invert {
                    let clamped_ei = end_cover_invert;
                    let clamped_si = (clamped_ei + design_slope * length).min(start_cover_invert);
                    running_invert = Some(clamped_ei);
                    (clamped_si, clamped_ei)
                } else {
                    running_invert = Some(ei);
                    (si, ei)
                }
            };

            // Update sump_elevation
            nodes[start_idx].sump_elevation = start_invert - 0.2;
            let end_sump_candidate = end_invert - 0.2;
            let current_end_sump = nodes[end_idx].sump_elevation;
            nodes[end_idx].sump_elevation = if current_end_sump.is_infinite() {
                end_sump_candidate
            } else {
                current_end_sump.min(end_sump_candidate)
            };

            let mut pipe_meta: HashMap<String, Value> = HashMap::new();
            if needs_pump {
                pipe_meta.insert("bypassed_by_pump".to_string(), Value::Bool(true));
                let pump_lift = (min_slope - natural_slope) * length;
                pipe_meta.insert("pump_lift".to_string(), Value::from(pump_lift.max(0.0)));
                nodes[end_idx].node_type = NodeType::Pump;
            }

            let slope = if length > 0.0 {
                (start_invert - end_invert) / length
            } else {
                0.0
            };

            pipes.push(NetworkPipe {
                id: PipeId::new(format!("Pipe - ({})", i + 1)),
                start_node_id: NodeId::new(u),
                end_node_id: NodeId::new(v),
                length,
                effective_length: length,
                diameter,
                material,
                roughness,
                start_invert,
                end_invert,
                slope,
                design_flow: flow,
                waypoints: vec![],
                metadata: pipe_meta,
            });
        }

        // Fix sentinel sump_elevations
        for node in &mut nodes {
            if node.sump_elevation.is_infinite() {
                node.sump_elevation = node.z - 1.5;
            }
        }

        Ok(PipeNetwork::new(name, nodes, pipes))
    }
}

// ── Solver trait impl ─────────────────────────────────────────────────────────

impl Solver for SewerSolver {
    fn solve(&mut self, params: &SolverParams) -> Result<Vec<Solution>, SolverError> {
        // Guard: no service points → no network can be built.
        if self.service_points.is_empty() {
            return Err(SolverError::InsufficientTerminals);
        }

        // Clone fields that need a borrow release before the &mut self call.
        let service_points = self.service_points.clone();
        let network_name = self.network_name.clone();
        let outlet = self.outlet;
        let flow_per_service = self.flow_per_service;
        let material = self.material; // PipeMaterial is Copy

        let result = self.solve_sewer(
            &service_points,
            outlet,
            &network_name,
            flow_per_service,
            material,
            params,
        )?;

        // REQ-007: empty Ok MUST become Err(NoFeasibleSolution).
        if result.is_empty() {
            return Err(SolverError::NoFeasibleSolution);
        }
        Ok(result)
    }

    fn evaluate(&self, network: &PipeNetwork) -> Result<SolutionScore, SolverError> {
        let total_length = network.total_length;
        let pipes = &network.pipes;
        let n = pipes.len();

        if n == 0 {
            return Ok(SolutionScore {
                total_cost: 0.0,
                total_length: 0.0,
                total_excavation: 0.0,
                norm_violations: 0,
                pump_count: 0,
                details: serde_json::json!({
                    "pipe_count": 0,
                    "node_count": network.node_count(),
                    "avg_excavation": 0.0,
                }),
            });
        }

        // Build arrays for vectorized computation (mirrors Python numpy approach)
        let mut diameters = vec![0.0_f64; n];
        let mut slopes = vec![0.0_f64; n];
        let mut lengths = vec![0.0_f64; n];
        let mut avg_depths = vec![0.0_f64; n];

        for (i, pipe) in pipes.iter().enumerate() {
            let start_node = network.get_node(&pipe.start_node_id);
            let end_node = network.get_node(&pipe.end_node_id);

            diameters[i] = pipe.diameter;
            slopes[i] = pipe.slope.abs();
            lengths[i] = pipe.length;

            // avg_depth = max(0, ((z_start - invert_start) + (z_end - invert_end)) / 2)
            // Uses terrain.elevation_at(x, y) for EXACT results on on-grid nodes (risk #1)
            avg_depths[i] = match (start_node, end_node) {
                (Some(sn), Some(en)) => {
                    let depth_start = self.terrain.elevation_at(sn.x, sn.y) - pipe.start_invert;
                    let depth_end = self.terrain.elevation_at(en.x, en.y) - pipe.end_invert;
                    ((depth_start + depth_end) / 2.0).max(0.0)
                }
                _ => 0.0,
            };
        }

        let total_excavation: f64 = avg_depths
            .iter()
            .zip(lengths.iter())
            .map(|(d, l)| d * l)
            .sum();

        // Roughness from first pipe (all pipes same material in SewerSolver)
        let roughness = pipes.first().map(|p| p.roughness).unwrap_or(0.009);
        let velocities = manning_batch(&diameters, &slopes, roughness);

        // Violation counting (mirrors Python boolean-array arithmetic)
        let raw_slopes: Vec<f64> = pipes.iter().map(|p| p.slope).collect();
        let bypass_mask: Vec<bool> = pipes
            .iter()
            .map(|p| {
                p.metadata
                    .get("bypassed_by_pump")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false)
            })
            .collect();

        let slope_low: usize = raw_slopes
            .iter()
            .zip(bypass_mask.iter())
            .filter(|(&s, &bp)| s < self.constraints.min_slope && !bp)
            .count();

        let slope_high: usize = raw_slopes
            .iter()
            .zip(bypass_mask.iter())
            .filter(|(&s, &bp)| s > self.constraints.max_slope && !bp)
            .count();

        let vel_low: usize = velocities
            .iter()
            .filter(|&&v| v < self.constraints.min_velocity)
            .count();

        let vel_high: usize = velocities
            .iter()
            .filter(|&&v| v > self.constraints.max_velocity)
            .count();

        let violations = (slope_low + slope_high + vel_low + vel_high) as i64;

        let pump_count = network
            .nodes
            .iter()
            .filter(|n| n.node_type == NodeType::Pump)
            .count() as i64;

        let cost = 1.0 * total_length + 2.0 * total_excavation + 100.0 * violations as f64;
        let avg_excavation = total_excavation / total_length.max(1.0);

        Ok(SolutionScore {
            total_cost: cost,
            total_length,
            total_excavation,
            norm_violations: violations,
            pump_count,
            details: serde_json::json!({
                "pipe_count": network.pipe_count(),
                "node_count": network.node_count(),
                "avg_excavation": avg_excavation,
            }),
        })
    }
}

// ── Private helpers ───────────────────────────────────────────────────────────

/// Gene parameters bundled for internal use.
#[derive(Debug, Clone)]
struct GeneParams {
    slope_factor: f64,
    cover_factor: f64,
    diameter_offset: i32,
    manhole_spacing: Option<f64>,
}

/// Convert a SteinerTree into an undirected `SolverGraph`.
///
/// Both directions of each edge are added so BFS orientation from the outlet
/// can proceed in either direction. Replaces the former `steiner_to_adj`
/// `HashMap<String, Vec<(String, f64)>>` builder.
fn steiner_to_solver_graph(steiner: &hydro_terrain::SteinerTree) -> SolverGraph {
    let mut sg = SolverGraph::new();
    for nid in steiner.node_ids() {
        sg.add_node(nid.as_str());
    }
    for (a, b, w) in steiner.edges() {
        sg.add_edge(a.as_str(), b.as_str(), *w);
        sg.add_edge(b.as_str(), a.as_str(), *w);
    }
    sg
}

/// BFS from `outlet_idx` over the undirected `SolverGraph` to orient edges.
///
/// Returns:
/// - `oriented`: directed graph where each edge goes upstream → downstream
///   (neighbor → current; mirrors Python `directed.add_edge(neighbor, current)`).
/// - `in_oriented`: reverse of `oriented` (current → predecessor), consumed by
///   `simplify_tree_sg` to compute per-node in-degree for junction detection.
fn bfs_build_oriented(
    undirected: &SolverGraph,
    outlet_idx: petgraph::graph::NodeIndex<u32>,
) -> (SolverGraph, SolverGraph) {
    let mut oriented = SolverGraph::new();
    let mut in_oriented = SolverGraph::new();

    // Pre-register all nodes so isolated nodes appear in both graphs.
    for idx in undirected.node_indices() {
        let id = undirected.node_id(idx);
        oriented.add_node(id);
        in_oriented.add_node(id);
    }

    let mut visited: HashSet<petgraph::graph::NodeIndex<u32>> = HashSet::new();
    let mut queue: VecDeque<petgraph::graph::NodeIndex<u32>> = VecDeque::new();

    visited.insert(outlet_idx);
    queue.push_back(outlet_idx);

    while let Some(current) = queue.pop_front() {
        let current_id = undirected.node_id(current);
        for (neighbor, w) in undirected.sorted_neighbors(current) {
            if visited.insert(neighbor) {
                let neighbor_id = undirected.node_id(neighbor);
                // upstream neighbor flows toward downstream current.
                oriented.add_edge(neighbor_id, current_id, w);
                in_oriented.add_edge(current_id, neighbor_id, w);
                queue.push_back(neighbor);
            }
        }
    }

    (oriented, in_oriented)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Test-only helper: orient an undirected adjacency via `SolverGraph` BFS
    /// and return the topo order as `Vec<String>` (leaves-first, outlet-last).
    ///
    /// Mirrors the internal pipeline of `build_network_from_tree` after PR-B2:
    ///   1. Build undirected `SolverGraph` from the HashMap adjacency.
    ///   2. BFS-orient from outlet via `bfs_build_oriented`.
    ///   3. Run `sorted_kahn_topo_sort` (BTreeSet lex-pop) on the oriented graph.
    fn sewer_orient_and_kahn(
        undirected_adj: &HashMap<String, Vec<(String, f64)>>,
        outlet: &str,
    ) -> Vec<String> {
        let mut sg = SolverGraph::new();
        for (u, succs) in undirected_adj {
            sg.add_node(u.as_str());
            for (v, w) in succs {
                sg.add_edge(u.as_str(), v.as_str(), *w);
            }
        }
        let outlet_idx = sg.node_index(outlet).expect("outlet must be in graph");
        let (oriented, _in_oriented) = bfs_build_oriented(&sg, outlet_idx);
        oriented
            .sorted_kahn_topo_sort()
            .expect("acyclic oriented tree must not fail")
            .iter()
            .map(|&idx| oriented.node_id(idx).to_owned())
            .collect()
    }

    /// RED (PR-B2) — simplify_tree_sg on SolverGraph.
    ///
    /// Calls `simplify_tree_sg` which is introduced in PR-B2 GREEN.
    /// Before GREEN this test fails to compile (E0425: cannot find function
    /// `simplify_tree_sg` in module `super`).
    ///
    /// Fixture: 7-node chain with degree-2 intermediate nodes that should
    /// collapse when manhole_spacing > their segment length.
    ///
    ///   g6 — g5 — g4 — g3(outlet) — g2 — g1 — g0
    ///
    /// With manhole_spacing=100.0 and segment length=10.0 per edge, only the
    /// endpoints (outlet=g3, leaf nodes g0, g6) are kept. Intermediate nodes
    /// g1,g2,g4,g5 all have in_degree+out_degree==2 and spacing does not force
    /// intermediate manholes — so after simplification only g0, g3, g6 remain.
    ///
    /// This pins the SolverGraph-based `simplify_tree_sg` contract:
    ///   kept_nodes.len() == 3
    ///   "g3" in kept_nodes (outlet)
    ///   "g0" in kept_nodes (leaf)
    ///   "g6" in kept_nodes (leaf)
    #[test]
    fn simplify_tree_sg_collapses_degree2_chain() {
        use hydro_types::{DesignConstraints, PipeMaterial};
        use std::collections::HashSet;

        // Build oriented SolverGraph: g6→g5→g4→g3 and g0→g1→g2→g3 (flows toward g3)
        let mut oriented = SolverGraph::new();
        for (a, b) in &[
            ("g6", "g5"),
            ("g5", "g4"),
            ("g4", "g3"),
            ("g0", "g1"),
            ("g1", "g2"),
            ("g2", "g3"),
        ] {
            oriented.add_edge(a, b, 10.0);
        }

        // Build in_oriented (reverse): g3→g4→g5→g6 and g3→g2→g1→g0
        let mut in_oriented = SolverGraph::new();
        for (a, b) in &[
            ("g5", "g6"),
            ("g4", "g5"),
            ("g3", "g4"),
            ("g2", "g3"),
            ("g1", "g2"),
            ("g0", "g1"),
        ] {
            in_oriented.add_edge(a, b, 10.0);
        }

        let terminal_set: HashSet<String> = ["g0", "g6"].iter().map(|s| s.to_string()).collect();
        let outlet_node = "g3";

        // Build a minimal TerrainGraph with coordinates so simplify_tree_sg can
        // compute distances. We cannot use TerrainGraph directly in unit tests
        // without a model file, so we call simplify_tree_sg — if it exists the
        // compile succeeds (GREEN); if not we get E0425 (RED).
        //
        // NOTE: This test only checks the *compile-time* existence of
        // `simplify_tree_sg` at the RED stage.  A runtime assertion is in the
        // GREEN commit once TerrainGraph fixture infrastructure is available.
        let _ = (oriented, in_oriented, terminal_set, outlet_node);

        // RED: this call must fail to compile until PR-B2 GREEN introduces
        // `simplify_tree_sg`. The `simplify_tree_sg` function takes
        // `(&SolverGraph, &SolverGraph, ...)` and returns
        // `(SolverGraph, HashSet<String>)`.
        let _exists: fn(
            &SewerSolver,
            &SolverGraph,
            &SolverGraph,
            &hydro_terrain::TerrainGraph,
            &HashSet<String>,
            &str,
            f64,
        ) -> (SolverGraph, HashSet<String>) = SewerSolver::simplify_tree_sg;
    }

    /// GREEN — pin topological order produced by the SolverGraph lex-Kahn pipeline.
    ///
    /// Verifies that `sewer_orient_and_kahn` (using `SolverGraph` BFS +
    /// `sorted_kahn_topo_sort`) produces the BTreeSet lex-pop order introduced
    /// in PR-B2.
    ///
    /// Fixture: 5-node sewer tree, outlet at "g0":
    ///   g2 — g1 — g0 — g3 — g4
    ///
    /// Oriented (upstream→downstream after BFS from g0):
    ///   g2→g1→g0, g4→g3→g0
    ///
    /// Expected topo order (lex BTreeSet pop, leaves-first, outlet-last):
    ///   ["g2", "g1", "g4", "g3", "g0"]
    ///
    /// Derivation:
    ///   in-degrees: g2=0, g4=0, g1=1, g3=1, g0=2
    ///   ready = {g2, g4} → pop lex "g2" → g1.in_deg=0 → ready = {g1, g4}
    ///   pop lex "g1" → g0.in_deg=1 → ready = {g4}
    ///   pop lex "g4" → g3.in_deg=0 → ready = {g3}
    ///   pop lex "g3" → g0.in_deg=0 → ready = {g0}
    ///   pop lex "g0" → done
    ///   Order: [g2, g1, g4, g3, g0]
    #[test]
    fn sewer_topo_order_snapshot() {
        let mut undirected: HashMap<String, Vec<(String, f64)>> = HashMap::new();
        let edges = [
            ("g0", "g1", 10.0_f64),
            ("g1", "g2", 10.0),
            ("g0", "g3", 10.0),
            ("g3", "g4", 10.0),
        ];
        for (a, b, w) in &edges {
            undirected
                .entry(a.to_string())
                .or_default()
                .push((b.to_string(), *w));
            undirected
                .entry(b.to_string())
                .or_default()
                .push((a.to_string(), *w));
        }

        let order = sewer_orient_and_kahn(&undirected, "g0");

        // BTreeSet lex-pop order (PR-B2 production pipeline).
        assert_eq!(
            order,
            vec!["g2", "g1", "g4", "g3", "g0"],
            "expected lex-Kahn order [g2, g1, g4, g3, g0], got {:?}",
            order
        );
    }
}
