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
}

impl SewerSolver {
    /// Create a new SewerSolver with the given terrain and design constraints.
    pub fn new(terrain: TerrainModel, constraints: DesignConstraints) -> Self {
        SewerSolver {
            terrain,
            constraints,
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

        // Convert SteinerTree to adjacency for BFS orientation
        let steiner_adj = steiner_to_adj(&steiner);

        let network = self
            .build_network_from_tree(
                &graph,
                &steiner_adj,
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
            let distances: Vec<(String, f64)> = terminal_nodes
                .iter()
                .filter(|n| *n != &outlet_node)
                .filter_map(|nid| {
                    graph
                        .shortest_path_cost(nid, &outlet_node)
                        .ok()
                        .map(|c| (nid.clone(), c))
                })
                .collect();

            if let Some((farthest, _)) = distances
                .iter()
                .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
            {
                let farthest = farthest.clone();
                if let Ok(k_paths) =
                    graph.k_shortest_paths(&farthest, &outlet_node, params.num_alternatives)
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
    /// Faithful port of Python `_simplify_tree`. Keeps:
    /// - outlets + terminals (must_keep)
    /// - junction nodes (in_degree + out_degree != 2)
    /// - intermediate nodes placed at `manhole_spacing` intervals along chains.
    ///
    /// Returns `(simplified_adj, kept_nodes)` where simplified_adj carries
    /// `path_length` as edge weight.
    #[allow(clippy::type_complexity)]
    fn simplify_tree(
        &self,
        directed: &HashMap<String, Vec<(String, f64)>>, // node → [(successor, len)]
        in_directed: &HashMap<String, Vec<(String, f64)>>, // node → [(predecessor, len)]
        graph: &TerrainGraph,
        terminal_set: &HashSet<String>,
        outlet_node: &str,
        manhole_spacing: f64,
    ) -> (HashMap<String, Vec<(String, f64)>>, HashSet<String>) {
        // Build must_keep set: outlet + terminals + junctions (degree != 2)
        let all_nodes: HashSet<String> = directed.keys().cloned().collect();
        let mut must_keep: HashSet<String> = HashSet::new();
        must_keep.insert(outlet_node.to_string());
        for t in terminal_set {
            must_keep.insert(t.clone());
        }
        for node in &all_nodes {
            let out_deg = directed.get(node).map(|v| v.len()).unwrap_or(0);
            let in_deg = in_directed.get(node).map(|v| v.len()).unwrap_or(0);
            if in_deg + out_deg != 2 {
                must_keep.insert(node.clone());
            }
        }

        // Walk chains: for each must_keep node, follow successors until another must_keep node
        let mut chains: Vec<Vec<String>> = Vec::new();
        let mut visited_edges: HashSet<(String, String)> = HashSet::new();

        for start in &must_keep {
            let succs: Vec<String> = directed
                .get(start)
                .map(|v| v.iter().map(|(s, _)| s.clone()).collect())
                .unwrap_or_default();
            for succ in succs {
                let edge_key = (start.clone(), succ.clone());
                if visited_edges.contains(&edge_key) {
                    continue;
                }
                let mut chain = vec![start.clone()];
                let mut current = succ.clone();
                while !must_keep.contains(&current) {
                    chain.push(current.clone());
                    let next_succs: Vec<String> = directed
                        .get(&current)
                        .map(|v| v.iter().map(|(s, _)| s.clone()).collect())
                        .unwrap_or_default();
                    if next_succs.is_empty() {
                        break;
                    }
                    visited_edges.insert((chain[chain.len() - 2].clone(), current.clone()));
                    current = next_succs[0].clone();
                }
                visited_edges.insert((chain.last().unwrap().clone(), current.clone()));
                chain.push(current);
                chains.push(chain);
            }
        }

        // Place intermediate manholes at manhole_spacing intervals
        let mut keep = must_keep.clone();
        for chain in &chains {
            let mut dist = 0.0_f64;
            for i in 1..chain.len().saturating_sub(1) {
                let c_prev = graph.get_node_coord(&chain[i - 1]);
                let c_curr = graph.get_node_coord(&chain[i]);
                if let (Some((px, py)), Some((cx, cy))) = (c_prev, c_curr) {
                    dist += ((px - cx).powi(2) + (py - cy).powi(2)).sqrt();
                }
                if dist >= manhole_spacing {
                    keep.insert(chain[i].clone());
                    dist = 0.0;
                }
            }
        }

        // Build simplified directed graph with path_length on each edge
        let mut simplified: HashMap<String, Vec<(String, f64)>> = HashMap::new();
        for node in &keep {
            simplified.entry(node.clone()).or_default();
        }

        for chain in &chains {
            let mut prev_kept = chain[0].clone();
            let mut path_len = 0.0_f64;
            for i in 1..chain.len() {
                let c_prev = graph.get_node_coord(&chain[i - 1]);
                let c_curr = graph.get_node_coord(&chain[i]);
                if let (Some((px, py)), Some((cx, cy))) = (c_prev, c_curr) {
                    path_len += ((px - cx).powi(2) + (py - cy).powi(2)).sqrt();
                }
                if keep.contains(&chain[i]) {
                    simplified
                        .entry(prev_kept.clone())
                        .or_default()
                        .push((chain[i].clone(), path_len));
                    prev_kept = chain[i].clone();
                    path_len = 0.0;
                }
            }
        }

        (simplified, keep)
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
        tree_adj: &HashMap<String, Vec<(String, f64)>>, // undirected: node → [(neighbor, length)]
        outlet_node: &str,
        terminal_nodes: &[String],
        name: &str,
        flow_per_service: f64,
        material: PipeMaterial,
        params: &GeneParams,
    ) -> Result<PipeNetwork, String> {
        let terminal_set: HashSet<String> = terminal_nodes.iter().cloned().collect();

        // BFS from outlet to orient edges: neighbor → current
        let (directed, in_directed) = bfs_orient(tree_adj, outlet_node)?;

        // All nodes in the directed tree
        let all_node_ids: HashSet<String> = directed
            .keys()
            .cloned()
            .chain(in_directed.keys().cloned())
            .collect();

        // Simplify or use as-is
        let (work_adj, kept_nodes) = if let Some(spacing) = params.manhole_spacing {
            self.simplify_tree(
                &directed,
                &in_directed,
                graph,
                &terminal_set,
                outlet_node,
                spacing,
            )
        } else {
            // Use directed tree as-is; convert to path_length-based adjacency
            let mut work = HashMap::new();
            for (u, succs) in &directed {
                let entry: Vec<(String, f64)> =
                    succs.iter().map(|(v, len)| (v.clone(), *len)).collect();
                work.insert(u.clone(), entry);
            }
            let kept: HashSet<String> = all_node_ids;
            (work, kept)
        };

        // Create nodes
        let mut nodes: Vec<NetworkNode> = Vec::new();
        for nid in &kept_nodes {
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
            let mut node = NetworkNode::new(nid.as_str(), coord.0, coord.1, z, ntype);
            node.rim_elevation = z;
            node.sump_elevation = f64::INFINITY; // Will be set during pipe processing
            nodes.push(node);
        }

        // Initialize sump_elevation to a sentinel (Python: default from node constructor)
        // Python sewer.py starts from default node with sump_elevation = z - 1.5
        // then updates it during pipe iteration via min().
        // We replicate: sentinel = +∞, first pipe sets it, subsequent min() updates it.
        // Since we build nodes first and then update sump_elevation, we start with +∞.

        // Topological sort of work_adj (BFS from outlet is reverse-topo, so reverse)
        let topo_order = topological_sort(&work_adj);

        // Accumulated flow per node (upstream → downstream)
        let mut flow_at_node: HashMap<String, f64> = HashMap::new();
        for nid in &kept_nodes {
            let base = if terminal_set.contains(nid) {
                flow_per_service
            } else {
                0.0
            };
            flow_at_node.insert(nid.clone(), base);
        }
        for nid in &topo_order {
            let succs = work_adj.get(nid).cloned().unwrap_or_default();
            let current_flow = flow_at_node.get(nid).copied().unwrap_or(0.0);
            for (successor, _) in succs {
                let entry = flow_at_node.entry(successor.clone()).or_insert(0.0);
                *entry += current_flow;
            }
        }

        // Create pipes
        let roughness = material.manning_n();
        let cover = self.constraints.min_cover * params.cover_factor;
        let min_slope = self.constraints.min_slope;
        let max_slope = self.constraints.max_slope;

        // Build a node lookup map for mutations
        let mut node_map: HashMap<String, usize> = HashMap::new();
        for (i, n) in nodes.iter().enumerate() {
            node_map.insert(n.id.0.clone(), i);
        }

        let mut pipes: Vec<NetworkPipe> = Vec::new();
        let mut pipe_counter = 0usize;

        // Iterate edges in work_adj (mirrors Python `for u, v, edge_data in work_graph.edges`)
        // Note: Python iterates in petgraph/networkx order (not deterministic cross-lang)
        // but since we're doing structural parity, the order matters for pipe IDs.
        // We iterate in topological order from leaves to outlet to match Python's topo walk.
        // Actually Python iterates work_graph.edges() in insertion order.
        // For simplify_tree=None case, edges come from directed (BFS orientation).
        // We replicate by iterating work_adj keys in some order.
        //
        // CRITICAL for parity: Python's work_graph.edges(data=True) returns edges in
        // the DiGraph's internal insertion order (the BFS order). We replicate this
        // by iterating topo_order and emitting edges from each node.

        for u in &topo_order {
            let succs = match work_adj.get(u) {
                Some(v) => v.clone(),
                None => continue,
            };
            for (v, path_length) in succs {
                let start_idx = match node_map.get(u) {
                    Some(&i) => i,
                    None => continue,
                };
                let end_idx = match node_map.get(&v) {
                    Some(&i) => i,
                    None => continue,
                };

                let start_z = nodes[start_idx].z;
                let end_z = nodes[end_idx].z;

                // Use path_length as the pipe length (accounts for simplified chains)
                let length = path_length;
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
                    .get(u)
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
                nodes[start_idx].sump_elevation = start_sump;

                let end_sump_candidate = end_invert - 0.2;
                let current_end_sump = nodes[end_idx].sump_elevation;
                nodes[end_idx].sump_elevation = if current_end_sump.is_infinite() {
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
                    nodes[end_idx].node_type = NodeType::Pump;
                }

                let slope = if length > 0.0 {
                    (start_invert - end_invert) / length
                } else {
                    0.0
                };

                pipes.push(NetworkPipe {
                    id: PipeId::new(format!("Pipe - ({})", pipe_counter)),
                    start_node_id: NodeId::new(u.as_str()),
                    end_node_id: NodeId::new(v.as_str()),
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
        let terminal_set: HashSet<String> = terminal_nodes.iter().cloned().collect();

        // Determine which path indices to keep as manholes
        let (kept_path, sorted_kept_indices) = if let Some(spacing) = params.manhole_spacing {
            let mut kept_indices: HashSet<usize> = [0, path.len() - 1].into_iter().collect();
            for (i, nid) in path.iter().enumerate() {
                if terminal_set.contains(nid) || nid == outlet_node {
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
            let kp: Vec<String> = sorted.iter().map(|&i| path[i].clone()).collect();
            (kp, sorted)
        } else {
            let kp = path.to_vec();
            let ki: Vec<usize> = (0..path.len()).collect();
            (kp, ki)
        };

        // Create nodes with accumulated flow
        let mut nodes: Vec<NetworkNode> = Vec::new();
        let mut node_map: HashMap<String, usize> = HashMap::new();
        let mut accumulated_flow = 0.0_f64;
        let mut flow_at: HashMap<String, f64> = HashMap::new();

        for nid in &kept_path {
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
            let mut node = NetworkNode::new(nid.as_str(), coord.0, coord.1, z, ntype);
            node.rim_elevation = z;
            node.sump_elevation = f64::INFINITY;
            let idx = nodes.len();
            node_map.insert(nid.clone(), idx);
            nodes.push(node);
            if terminal_set.contains(nid) {
                accumulated_flow += flow_per_service;
            }
            flow_at.insert(nid.clone(), accumulated_flow);
        }

        // Pre-compute segment lengths (sum intermediate edges for consolidated segments)
        let mut pipe_segments: Vec<(String, String, f64)> = Vec::new();
        for seg in 0..sorted_kept_indices.len().saturating_sub(1) {
            let i_start = sorted_kept_indices[seg];
            let i_end = sorted_kept_indices[seg + 1];
            let mut seg_len = 0.0_f64;
            for j in i_start..i_end {
                let c1 = graph.get_node_coord(&path[j]);
                let c2 = graph.get_node_coord(&path[j + 1]);
                if let (Some((x1, y1)), Some((x2, y2))) = (c1, c2) {
                    seg_len += ((x1 - x2).powi(2) + (y1 - y2).powi(2)).sqrt();
                }
            }
            pipe_segments.push((path[i_start].clone(), path[i_end].clone(), seg_len));
        }

        let roughness = material.manning_n();
        let cover = self.constraints.min_cover * params.cover_factor;
        let min_slope = self.constraints.min_slope;
        let max_slope = self.constraints.max_slope;
        let mut running_invert: Option<f64> = None;
        let mut pipes: Vec<NetworkPipe> = Vec::new();

        for (i, (u, v, length)) in pipe_segments.iter().enumerate() {
            let start_idx = match node_map.get(u) {
                Some(&i) => i,
                None => continue,
            };
            let end_idx = match node_map.get(v) {
                Some(&i) => i,
                None => continue,
            };

            if *length < 0.1 {
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

            let slope = if *length > 0.0 {
                (start_invert - end_invert) / length
            } else {
                0.0
            };

            pipes.push(NetworkPipe {
                id: PipeId::new(format!("Pipe - ({})", i + 1)),
                start_node_id: NodeId::new(u.as_str()),
                end_node_id: NodeId::new(v.as_str()),
                length: *length,
                effective_length: *length,
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
    fn solve(&mut self, _params: &SolverParams) -> Result<Vec<Solution>, SolverError> {
        // The generic solve() has no service_points/outlet — return error to guide callers.
        // Callers should use solve_sewer() directly for full functionality.
        // This enables the MockSolver test to work with the trait.
        Err(SolverError::BuildFailed(
            "Use SewerSolver::solve_sewer() with explicit outlet and service_points".to_string(),
        ))
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

/// Convert a SteinerTree into an undirected adjacency map.
///
/// Returns `node → [(neighbor, edge_length)]` where edge_length is the
/// Euclidean distance (weight) stored in the Steiner tree edge.
/// This is used to BFS-orient the tree toward the outlet.
fn steiner_to_adj(steiner: &hydro_terrain::SteinerTree) -> HashMap<String, Vec<(String, f64)>> {
    let mut adj: HashMap<String, Vec<(String, f64)>> = HashMap::new();
    // Initialize all nodes (including isolated leaf nodes)
    for nid in steiner.node_ids() {
        adj.entry(nid.clone()).or_default();
    }
    // Add undirected edges
    for (a, b, w) in steiner.edges() {
        adj.entry(a.clone()).or_default().push((b.clone(), *w));
        adj.entry(b.clone()).or_default().push((a.clone(), *w));
    }
    adj
}

/// BFS from outlet to orient undirected tree edges.
///
/// Returns:
/// - `directed`: node → [(successor, edge_length)] (flow direction: towards outlet)
/// - `in_directed`: node → [(predecessor, edge_length)] (for simplify_tree)
///
/// Mirrors Python:
/// ```python
/// directed.add_edge(neighbor, current)  # neighbor flows → current
/// ```
type DirectedAdj = HashMap<String, Vec<(String, f64)>>;

fn bfs_orient(
    tree_adj: &HashMap<String, Vec<(String, f64)>>,
    outlet_node: &str,
) -> Result<(DirectedAdj, DirectedAdj), String> {
    let mut directed: HashMap<String, Vec<(String, f64)>> = HashMap::new();
    let mut in_directed: HashMap<String, Vec<(String, f64)>> = HashMap::new();
    let mut visited: HashSet<String> = HashSet::new();
    let mut queue: VecDeque<String> = VecDeque::new();

    // Initialize all nodes in directed (even isolated ones)
    for nid in tree_adj.keys() {
        directed.entry(nid.clone()).or_default();
        in_directed.entry(nid.clone()).or_default();
    }

    queue.push_back(outlet_node.to_string());
    visited.insert(outlet_node.to_string());

    while let Some(current) = queue.pop_front() {
        let neighbors = tree_adj.get(&current).cloned().unwrap_or_default();
        for (neighbor, len) in neighbors {
            if !visited.contains(&neighbor) {
                // Python: directed.add_edge(neighbor, current)
                // neighbor → current (upstream → downstream)
                directed
                    .entry(neighbor.clone())
                    .or_default()
                    .push((current.clone(), len));
                in_directed
                    .entry(current.clone())
                    .or_default()
                    .push((neighbor.clone(), len));

                visited.insert(neighbor.clone());
                queue.push_back(neighbor);
            }
        }
    }

    Ok((directed, in_directed))
}

/// Topological sort of a directed adjacency map.
///
/// Returns nodes in topological order (leaves first, outlet last).
/// Mirrors Python `nx.topological_sort(work_graph)`.
fn topological_sort(adj: &HashMap<String, Vec<(String, f64)>>) -> Vec<String> {
    // Compute in-degrees
    let mut in_degree: HashMap<String, usize> = HashMap::new();
    for nid in adj.keys() {
        in_degree.entry(nid.clone()).or_insert(0);
    }
    for succs in adj.values() {
        for (v, _) in succs {
            *in_degree.entry(v.clone()).or_insert(0) += 1;
        }
    }

    // Kahn's algorithm (BFS-based topo sort)
    let mut queue: VecDeque<String> = in_degree
        .iter()
        .filter(|(_, &d)| d == 0)
        .map(|(k, _)| k.clone())
        .collect();

    // Sort for determinism
    let mut queue_vec: Vec<String> = queue.drain(..).collect();
    queue_vec.sort();
    queue.extend(queue_vec);

    let mut order: Vec<String> = Vec::new();
    while let Some(node) = queue.pop_front() {
        order.push(node.clone());
        let succs = adj.get(&node).cloned().unwrap_or_default();
        let mut new_sources: Vec<String> = Vec::new();
        for (v, _) in succs {
            let deg = in_degree.entry(v.clone()).or_insert(0);
            if *deg > 0 {
                *deg -= 1;
            }
            if *deg == 0 {
                new_sources.push(v);
            }
        }
        new_sources.sort(); // Deterministic
        queue.extend(new_sources);
    }

    order
}
