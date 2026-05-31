//! DistributionSolver — closed-loop (mesh) water distribution network design.
//!
//! Faithful port of `hydro_engine/solvers/distribution.py` (DistributionSolver class).
//!
//! ## Algorithm
//! 1. Build terrain graph from topography (grid).
//! 2. Snap source and demand points to nearest nodes.
//! 3. Compute MST as backbone.
//! 4. Add non-tree edges to form closed loops (mesh).
//! 5. Create nodes from combined MST+loop graph.
//! 6. Accumulate flow: single-source Dijkstra from source, add demand_per_node per hop.
//! 7. Size each pipe: required_diameter_pressure + velocity upgrade.
//! 8. Balance flows: BFS from source computing pressure at each node.
//! 9. Place valves (every N-th junction/service) and hydrants (cumulative distance).
//! 10. Evaluate: cost = 1.0*L + 1.5*excav + 150.0*viol + 5.0*std + 500.0*valves + 800.0*hydrants.

use std::collections::{BinaryHeap, HashMap, HashSet, VecDeque};

use hydro_hydraulics::hazen_williams::{
    head_loss, hw_coefficient, required_diameter_pressure, velocity,
};
use hydro_terrain::{CostFunction, CostWeights, TerrainGraph, TerrainModel};
use hydro_types::{
    response::SolutionMetadata, DesignConstraints, NetworkNode, NetworkPipe, NodeId, NodeType,
    PipeId, PipeMaterial, PipeNetwork, Solution, SolutionScore,
};
use serde_json::Value;

use crate::error::SolverError;
use crate::solver::{Solver, SolverParams};

// ── DistributionSolver ────────────────────────────────────────────────────────

/// Solver for closed-loop (mesh) water distribution networks.
///
/// Faithful port of Python `DistributionSolver`. Uses MST + loop edges to form
/// a mesh network, accumulates flows via shortest-path routing, and dimensions
/// pipes with Hazen-Williams.
pub struct DistributionSolver {
    pub terrain: TerrainModel,
    pub constraints: DesignConstraints,
}

impl DistributionSolver {
    /// Create a new DistributionSolver with the given terrain and design constraints.
    pub fn new(terrain: TerrainModel, constraints: DesignConstraints) -> Self {
        DistributionSolver {
            terrain,
            constraints,
        }
    }

    /// Solve the distribution network with explicit demand/source parameters.
    ///
    /// Mirrors Python `DistributionSolver.solve()`.
    #[allow(clippy::too_many_arguments)]
    pub fn solve_distribution(
        &mut self,
        demand_points: &[(f64, f64)],
        source: (f64, f64),
        source_head: f64,
        network_name: &str,
        demand_per_node: f64,
        material: PipeMaterial,
        num_alternatives: u32,
        mesh_density: f64,
        diameter_offset: i32,
        valve_spacing: f64,
        hydrant_spacing: f64,
        params: &SolverParams,
    ) -> Result<Vec<Solution>, SolverError> {
        let cost_fn = CostFunction::new(
            CostWeights {
                w_length: 1.0,
                w_excavation: 1.5,
                w_slope_penalty: 0.5,
                w_pump_penalty: 20.0,
                w_crossing: 10.0,
            },
            self.constraints.min_slope,
            self.constraints.max_slope,
            self.constraints.min_cover,
        );

        let mut graph = TerrainGraph::new(cost_fn);
        graph.from_grid(params.grid_resolution, &self.terrain);

        // Snap source and demand points to nearest graph nodes
        let source_node = graph
            .nearest_node(source.0, source.1)
            .map_err(|_| SolverError::NoOutlet)?;

        let demand_nodes: Vec<String> = demand_points
            .iter()
            .map(|&(x, y)| {
                graph
                    .nearest_node(x, y)
                    .map_err(|_| SolverError::InsufficientTerminals)
            })
            .collect::<Result<Vec<_>, _>>()?;

        // Validate reachability
        let reachable = graph
            .connected_component(&source_node)
            .map_err(|e| SolverError::BuildFailed(e.to_string()))?;

        let unreachable: Vec<&String> = demand_nodes
            .iter()
            .filter(|n| !reachable.contains(*n))
            .collect();

        if !unreachable.is_empty() {
            return Err(SolverError::BuildFailed(format!(
                "{} demand node(s) not reachable from source",
                unreachable.len()
            )));
        }

        // Convert mesh_density → max_loops_divisor
        // Python: max(2, int(round(5.0 * (1.0 - mesh_density) + 1.0)))
        // Python's round() is banker's rounding but for 0.5 density: 5.0*0.5+1.0=3.5 → round=4
        let max_loops_div = {
            let raw = 5.0 * (1.0 - mesh_density) + 1.0;
            // Rust f64::round() uses round-half-away-from-zero which differs from Python banker's
            // rounding for x.5 values. Python round(3.5)=4 (banker's), Rust 3.5_f64.round()=4.
            // They agree here. Python round(2.5)=2, Rust 2.5_f64.round()=3 — but only relevant
            // at mesh_density = 0.7 where raw=2.5; Python → 2, Rust → 3. Use Python semantics.
            let rounded = python_round(raw);
            rounded.max(2) as u32
        };

        let hw_c = hw_coefficient(material.as_str());

        let mut solutions: Vec<Solution> = Vec::new();
        let mut alternative_errors: Vec<String> = Vec::new();

        // Valve spacing: convert float to int divisor exactly as Python does
        // Python: int(max(1, valve_spacing / 100.0))
        let valve_spacing_int = ((valve_spacing / 100.0).max(1.0) as u32).max(1);

        // Alternative 1: primary mesh with max_loops_div
        match self.build_mesh_network(
            &graph,
            &source_node,
            &demand_nodes,
            network_name,
            demand_per_node,
            material,
            hw_c,
            source_head,
            max_loops_div,
            diameter_offset,
        ) {
            Ok(mut network) => {
                self.place_valves_and_hydrants(&mut network, valve_spacing_int, hydrant_spacing);
                let score = self.evaluate_distribution(&network)?;
                solutions.push(Solution {
                    rank: 1,
                    network,
                    score,
                    metadata: SolutionMetadata::default(),
                });
            }
            Err(e) => {
                return Err(SolverError::BuildFailed(format!(
                    "Failed to build primary distribution mesh solution: {e}"
                )));
            }
        }

        // Alternative 2+: different loop densities
        if num_alternatives > 1 {
            for alt_idx in 2..=(num_alternatives) {
                let loop_factor = (max_loops_div + alt_idx - 1).max(2);
                let alt_name = format!("{network_name}_alt{alt_idx}");
                match self.build_mesh_network(
                    &graph,
                    &source_node,
                    &demand_nodes,
                    &alt_name,
                    demand_per_node,
                    material,
                    hw_c,
                    source_head,
                    loop_factor,
                    diameter_offset,
                ) {
                    Ok(mut network) => {
                        self.place_valves_and_hydrants(
                            &mut network,
                            valve_spacing_int,
                            hydrant_spacing,
                        );
                        let score = self.evaluate_distribution(&network)?;
                        solutions.push(Solution {
                            rank: alt_idx as i64,
                            network,
                            score,
                            metadata: SolutionMetadata::default(),
                        });
                    }
                    Err(e) => {
                        alternative_errors.push(format!(
                            "Alternative distribution mesh {alt_idx} failed: {e}"
                        ));
                    }
                }
            }
        }

        // Sort by total_cost ascending, re-rank
        solutions.sort_by(|a, b| {
            a.score
                .total_cost
                .partial_cmp(&b.score.total_cost)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        for (i, sol) in solutions.iter_mut().enumerate() {
            sol.rank = (i + 1) as i64;
            // Store alternative errors in the solution's genetic_params metadata (if any)
            // Python: sol.metadata["alternative_errors"] = alternative_errors
            // Rust: store in SolutionMetadata.genetic_params (free-form Value field)
            if !alternative_errors.is_empty() {
                let errs: Vec<Value> = alternative_errors
                    .iter()
                    .map(|s| Value::String(s.clone()))
                    .collect();
                sol.metadata.genetic_params = serde_json::json!({
                    "alternative_errors": Value::Array(errs)
                });
            }
        }

        Ok(solutions)
    }

    // ── Mesh network builder ──────────────────────────────────────────────────

    /// Build a looped (mesh) network from the terrain graph.
    ///
    /// Faithful port of Python `_build_mesh_network()`.
    /// Does NOT use source-outward BFS orientation (unlike WaterSupply).
    #[allow(clippy::too_many_arguments)]
    fn build_mesh_network(
        &self,
        graph: &TerrainGraph,
        source_node: &str,
        demand_nodes: &[String],
        name: &str,
        demand_per_node: f64,
        material: PipeMaterial,
        hw_c: f64,
        source_head: f64,
        max_loops_divisor: u32,
        diameter_offset: i32,
    ) -> Result<PipeNetwork, String> {
        let demand_set: HashSet<String> = demand_nodes.iter().cloned().collect();

        // Step 1 — get all graph edges (in petgraph insertion order = from_grid order)
        // This is used for both MST construction (Kruskal) and loop candidate selection.
        let all_graph_edges = graph.all_edges(); // (u, v, weight, length) in insertion order

        // All component nodes = all graph nodes (grid is fully connected)
        let all_node_ids_vec = graph.node_ids();
        let mst_nodes: HashSet<String> = all_node_ids_vec.iter().cloned().collect();

        // Step 2 — Build MST using Kruskal to match Python networkx's MST exactly.
        //
        // Python networkx `minimum_spanning_tree` uses Kruskal:
        //   - Sort edges by weight (stable sort by weight, preserving original position for ties)
        //   - Greedy union-find: add edge if endpoints in different components
        //
        // KEY: the edge order from all_graph_edges (= from_grid insertion order) matches
        // Python's G.edges() iteration order (same grid construction, same direction order).
        // After stable sort by weight, edges with identical weight keep their G.edges() order.
        // This gives the SAME MST as networkx for this terrain.
        //
        // NOTE on float precision: edge costs computed via CostFunction.call(z_start, z_end)
        // where z values have accumulated float errors (e.g., 0.006*40 ≠ 0.24 exactly).
        // This causes x-edges from higher-x nodes to be SLIGHTLY cheaper, which determines
        // the loop edge selection order.

        // Stable sort all edges by weight (preserving insertion order for ties)
        let mut sorted_all_edges: Vec<(String, String, f64)> = all_graph_edges
            .iter()
            .map(|(u, v, w, _)| (u.clone(), v.clone(), *w))
            .collect();
        sorted_all_edges.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));

        // Kruskal MST using iterative union-find (path-halving)
        let mut parent: HashMap<String, String> = HashMap::new();

        let uf_find = |parent: &mut HashMap<String, String>, x: &str| -> String {
            let mut cur = x.to_string();
            loop {
                let p = parent.get(&cur).cloned().unwrap_or_else(|| cur.clone());
                if p == cur {
                    break cur;
                }
                let gp = parent.get(&p).cloned().unwrap_or_else(|| p.clone());
                parent.insert(cur.clone(), gp.clone());
                cur = p;
            }
        };

        let mst_edge_list: Vec<(String, String, f64)> = {
            let mut mst_edges = Vec::new();
            for (u, v, w) in &sorted_all_edges {
                let ru = uf_find(&mut parent, u);
                let rv = uf_find(&mut parent, v);
                if ru != rv {
                    parent.insert(ru, rv);
                    mst_edges.push((u.clone(), v.clone(), *w));
                }
            }
            mst_edges
        };

        // Build mst_edges_set (both directions) for candidate filtering
        let mut mst_edges_set: HashSet<(String, String)> = HashSet::new();
        for (u, v, _) in &mst_edge_list {
            mst_edges_set.insert((u.clone(), v.clone()));
            mst_edges_set.insert((v.clone(), u.clone()));
        }

        // max_loops = max(3, len(tree_nodes) // max_loops_divisor)
        let max_loops = (mst_nodes.len() / max_loops_divisor as usize).max(3);

        // Candidates: all graph edges NOT in MST, between tree nodes, sorted by weight
        // The sort is stable: edges are already in all_graph_edges (from_grid) insertion order.
        // After filtering, we re-sort by weight (stable) to match Python's candidates.sort().
        let mut candidates: Vec<(String, String, f64)> = all_graph_edges
            .iter()
            .filter_map(|(u, v, w, _)| {
                if mst_nodes.contains(u)
                    && mst_nodes.contains(v)
                    && !mst_edges_set.contains(&(u.clone(), v.clone()))
                {
                    Some((u.clone(), v.clone(), *w))
                } else {
                    None
                }
            })
            .collect();
        candidates.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));

        // Take first max_loops as loop_edges
        let loop_edges: Vec<(String, String)> = candidates
            .into_iter()
            .take(max_loops)
            .map(|(u, v, _)| (u, v))
            .collect();

        // Step 3 — build combined graph adjacency (MST + loop edges) in networkx order.
        // Replicates networkx: nx.Graph(mst) + combined.add_edges_from(loop_edges).
        // Node insertion order = order nodes first appear in mst_edge_list (Kruskal order).
        // Neighbor insertion order = MST edges added first (Kruskal order), then loop edges.
        // combined.edges() yields (u,v) for each u in node order, for each neighbor v not yet seen.

        // Build combined_adj: for each node, neighbors in insertion order.
        // MST neighbors first (in Kruskal-sorted edge order), then loop edges.
        let mut combined_adj: HashMap<String, Vec<String>> = HashMap::new();
        let mut combined_node_order: Vec<String> = Vec::new();
        let mut combined_node_set: HashSet<String> = HashSet::new();

        // Helper to add node in order
        let add_node = |node: &str,
                        order: &mut Vec<String>,
                        set: &mut HashSet<String>,
                        adj: &mut HashMap<String, Vec<String>>| {
            if !set.contains(node) {
                order.push(node.to_string());
                set.insert(node.to_string());
                adj.entry(node.to_string()).or_default();
            }
        };

        // Add MST nodes in Kruskal edge order (networkx MST node insertion order)
        for (u, v, _) in &mst_edge_list {
            add_node(
                u,
                &mut combined_node_order,
                &mut combined_node_set,
                &mut combined_adj,
            );
            add_node(
                v,
                &mut combined_node_order,
                &mut combined_node_set,
                &mut combined_adj,
            );
            // Add neighbors in MST adjacency order (Kruskal order → u gets v as neighbor, v gets u)
            combined_adj.entry(u.clone()).or_default().push(v.clone());
            combined_adj.entry(v.clone()).or_default().push(u.clone());
        }

        // Add loop edges: add_edges_from(loop_edges)
        // Nodes already in combined (since loop_edges connect tree nodes).
        // Neighbor insertion order: loop edges are added in their sorted order.
        for (u, v) in &loop_edges {
            // Ensure nodes exist (they should — they're mst_nodes)
            add_node(
                u,
                &mut combined_node_order,
                &mut combined_node_set,
                &mut combined_adj,
            );
            add_node(
                v,
                &mut combined_node_order,
                &mut combined_node_set,
                &mut combined_adj,
            );
            combined_adj.entry(u.clone()).or_default().push(v.clone());
            combined_adj.entry(v.clone()).or_default().push(u.clone());
        }

        // Step 4 — create NetworkNodes for all nodes in combined
        // Node order in combined (for valve/hydrant placement) = combined_node_order
        let mut nodes: Vec<NetworkNode> = Vec::new();
        let mut node_map: HashMap<String, usize> = HashMap::new();

        for nid in &combined_node_order {
            let coord = graph
                .get_node_coord(nid)
                .ok_or_else(|| format!("node {nid} not found in graph"))?;
            let z = graph
                .get_node_elevation(nid)
                .ok_or_else(|| format!("elevation for {nid} not found"))?;

            let ntype = if nid == source_node {
                NodeType::Tank
            } else if demand_set.contains(nid) {
                NodeType::Service
            } else {
                NodeType::Junction
            };

            let demand = if demand_set.contains(nid) {
                demand_per_node * 1000.0
            } else {
                0.0
            };

            let idx = nodes.len();
            node_map.insert(nid.clone(), idx);
            let mut node = NetworkNode::new(nid.as_str(), coord.0, coord.1, z, ntype);
            node.rim_elevation = z;
            node.demand = demand;
            nodes.push(node);
        }

        // Step 5 — flow accumulation via single-source Dijkstra from source
        // Build routing graph (length weights) over combined edges
        // Python: routing_graph.add_edge(u, v, length=max(dist, 0.1))
        //         all_paths = nx.single_source_dijkstra_path(routing_graph, source_node, weight="length")
        let routing_adj = build_routing_adj(&combined_adj, &nodes, &node_map);

        let all_paths = dijkstra_all_paths(source_node, &routing_adj);

        // Accumulate flow per edge
        let mut edge_flow: HashMap<(String, String), f64> = HashMap::new();
        for dnode in demand_nodes {
            if let Some(path) = all_paths.get(dnode.as_str()) {
                for i in 0..path.len().saturating_sub(1) {
                    let key = sorted_pair(path[i].as_str(), path[i + 1].as_str());
                    *edge_flow.entry(key).or_insert(0.0) += demand_per_node;
                }
            }
        }

        // Step 6 — iterate combined.edges() in networkx order, create pipes
        // networkx combined.edges() iteration:
        //   for u in combined._adj (node insertion order):
        //     for v in combined._adj[u] (neighbor insertion order):
        //       if v not in nodes_seen_so_far:
        //         yield (u, v)
        //     nodes_seen.add(u)
        let mut available: Vec<f64> = self.constraints.available_diameters.clone();
        available.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        let mut pipe_counter = 0usize;
        let mut pipes: Vec<NetworkPipe> = Vec::new();
        let mut running_total_length = 0.0_f64;
        let mut nodes_seen: HashSet<String> = HashSet::new();

        for u in &combined_node_order {
            let nbrs = combined_adj.get(u).cloned().unwrap_or_default();
            for v in &nbrs {
                if !nodes_seen.contains(v.as_str()) {
                    // This edge (u, v) is yielded by combined.edges()
                    let start_idx = match node_map.get(u) {
                        Some(&i) => i,
                        None => continue,
                    };
                    let end_idx = match node_map.get(v.as_str()) {
                        Some(&i) => i,
                        None => continue,
                    };

                    let length = {
                        let sn = &nodes[start_idx];
                        let en = &nodes[end_idx];
                        ((sn.x - en.x).powi(2) + (sn.y - en.y).powi(2)).sqrt()
                    };

                    if length < 0.1 {
                        continue;
                    }

                    pipe_counter += 1;

                    let key = sorted_pair(u.as_str(), v.as_str());
                    let flow = edge_flow
                        .get(&key)
                        .copied()
                        .unwrap_or(demand_per_node)
                        .max(demand_per_node);

                    // available_head is dynamic: uses running_total_length SO FAR
                    let available_head =
                        (source_head * (length / running_total_length.max(length))).max(1.0);

                    let diameter =
                        required_diameter_pressure(flow, length, available_head, hw_c, &available);

                    // diameter_offset: bump up by offset steps
                    let diameter = if diameter_offset > 0 {
                        if let Some(idx) =
                            available.iter().position(|&d| (d - diameter).abs() < 1e-12)
                        {
                            let new_idx = (idx as i32 + diameter_offset)
                                .min((available.len() - 1) as i32)
                                as usize;
                            available[new_idx]
                        } else {
                            diameter
                        }
                    } else {
                        diameter
                    };

                    // Velocity upgrade: if v_flow > max_velocity, find next larger diameter
                    let v_flow = velocity(flow, diameter);
                    let diameter = if v_flow > self.constraints.max_velocity {
                        let mut upgraded = diameter;
                        for &d in &available {
                            if d > diameter && velocity(flow, d) <= self.constraints.max_velocity {
                                upgraded = d;
                                break;
                            }
                        }
                        upgraded
                    } else {
                        diameter
                    };

                    let start_z = nodes[start_idx].z;
                    let end_z = nodes[end_idx].z;
                    let start_invert = start_z - self.constraints.min_cover;
                    let end_invert = end_z - self.constraints.min_cover;

                    running_total_length += length;

                    pipes.push(NetworkPipe {
                        id: PipeId::new(format!("Pipe - ({pipe_counter})")),
                        start_node_id: NodeId::new(u.as_str()),
                        end_node_id: NodeId::new(v.as_str()),
                        length,
                        effective_length: length,
                        diameter,
                        material,
                        roughness: material.manning_n(),
                        start_invert,
                        end_invert,
                        slope: if length > 0.0 {
                            (start_invert - end_invert) / length
                        } else {
                            0.0
                        },
                        design_flow: flow,
                        waypoints: vec![],
                        metadata: HashMap::new(),
                    });
                }
            }
            nodes_seen.insert(u.clone());
        }

        let mut network = PipeNetwork::new(name, nodes, pipes);

        // Step 7 — balance flows / compute pressures (BFS from source)
        self.balance_flows(&mut network, source_node, source_head, hw_c);

        Ok(network)
    }

    // ── Pressure BFS ──────────────────────────────────────────────────────────

    /// Calculate pressure at each node via BFS energy balance from source.
    ///
    /// Faithful port of Python `_balance_flows()`.
    fn balance_flows(
        &self,
        network: &mut PipeNetwork,
        source_node: &str,
        source_head: f64,
        hw_c: f64,
    ) {
        let source_id = NodeId::new(source_node);

        let src_idx = match network.nodes.iter().position(|n| n.id == source_id) {
            Some(i) => i,
            None => return,
        };

        network.nodes[src_idx]
            .metadata
            .insert("pressure_mca".to_string(), Value::from(source_head));

        let mut pressures: HashMap<String, f64> = HashMap::new();
        pressures.insert(source_node.to_string(), source_head);

        let mut visited: HashSet<String> = HashSet::new();
        visited.insert(source_node.to_string());
        let mut queue: VecDeque<String> = VecDeque::new();
        queue.push_back(source_node.to_string());

        while let Some(current_id_str) = queue.pop_front() {
            let current_id = NodeId::new(current_id_str.as_str());
            let current_pressure = match pressures.get(&current_id_str) {
                Some(&p) => p,
                None => continue,
            };

            let current_z = match network.nodes.iter().find(|n| n.id == current_id) {
                Some(n) => n.z,
                None => continue,
            };

            // pipes_at_node: iterate adjacency
            let adjacency = network
                .adjacency
                .get(&current_id)
                .cloned()
                .unwrap_or_default();

            for (neighbor_id, pipe_idx) in adjacency {
                let neighbor_id_str = neighbor_id.0.clone();
                if visited.contains(&neighbor_id_str) {
                    continue;
                }

                let pipe = &network.pipes[pipe_idx];

                // Python: flow = max(neighbor.demand, 1.0) / 1000.0
                // NOTE: 1.0 L/s floor (NOT 0.001 m3/s like WaterSupply)
                let neighbor_demand = network
                    .nodes
                    .iter()
                    .find(|n| n.id == neighbor_id)
                    .map(|n| n.demand)
                    .unwrap_or(0.0);

                let flow = neighbor_demand.max(1.0) / 1000.0;

                let hf = head_loss(flow, pipe.diameter, pipe.length, hw_c);

                let neighbor_z = match network.nodes.iter().find(|n| n.id == neighbor_id) {
                    Some(n) => n.z,
                    None => continue,
                };

                let dz = neighbor_z - current_z;
                let pressure = current_pressure - hf - dz;

                pressures.insert(neighbor_id_str.clone(), pressure);

                if let Some(n_idx) = network.nodes.iter().position(|n| n.id == neighbor_id) {
                    network.nodes[n_idx]
                        .metadata
                        .insert("pressure_mca".to_string(), Value::from(pressure));
                }

                visited.insert(neighbor_id_str.clone());
                queue.push_back(neighbor_id_str);
            }
        }
    }

    // ── Valve and hydrant placement ───────────────────────────────────────────

    /// Place valves at every valve_spacing-th junction/service, hydrants along pipes.
    ///
    /// Faithful port of Python `_place_valves_and_hydrants()`.
    /// Valves FIRST (JUNCTION/SERVICE nodes in order), THEN hydrants (pipes in order).
    fn place_valves_and_hydrants(
        &self,
        network: &mut PipeNetwork,
        valve_spacing: u32,
        hydrant_spacing: f64,
    ) {
        // Valves: iterate nodes in order, count JUNCTION and SERVICE nodes
        let mut junction_counter = 0u32;
        for node in &mut network.nodes {
            if matches!(node.node_type, NodeType::Junction | NodeType::Service) {
                junction_counter += 1;
                if junction_counter.is_multiple_of(valve_spacing) {
                    node.node_type = NodeType::Valve;
                    node.metadata.insert(
                        "accessory".to_string(),
                        Value::String("gate_valve".to_string()),
                    );
                }
            }
        }

        // Hydrants: accumulate length along pipes; when >= hydrant_spacing, mark end node
        // Only if end_node is still JUNCTION (not already converted to valve)
        let mut cumulative = 0.0_f64;
        let pipe_count = network.pipes.len();
        for i in 0..pipe_count {
            cumulative += network.pipes[i].length;
            if cumulative >= hydrant_spacing {
                let end_node_id = network.pipes[i].end_node_id.clone();
                if let Some(n) = network.nodes.iter_mut().find(|n| n.id == end_node_id) {
                    if n.node_type == NodeType::Junction {
                        n.node_type = NodeType::Hydrant;
                        n.metadata.insert(
                            "accessory".to_string(),
                            Value::String("hydrant".to_string()),
                        );
                    }
                }
                cumulative = 0.0;
            }
        }
    }

    // ── Evaluate ─────────────────────────────────────────────────────────────

    /// Evaluate a distribution network design.
    ///
    /// Faithful port of Python `evaluate()`.
    /// Cost = 1.0*L + 1.5*excav + 150.0*viol + 5.0*pressure_std + 500.0*valve + 800.0*hydrant
    fn evaluate_distribution(&self, network: &PipeNetwork) -> Result<SolutionScore, SolverError> {
        let total_length = network.total_length;
        let mut total_excavation = 0.0_f64;
        let mut violations = 0i64;

        for pipe in &network.pipes {
            let start_node = network.get_node(&pipe.start_node_id);
            let end_node = network.get_node(&pipe.end_node_id);

            if let (Some(sn), Some(en)) = (start_node, end_node) {
                // Excavation depth: average of (terrain_elev - invert) at each end
                let depth_start = self.terrain.elevation_at(sn.x, sn.y) - pipe.start_invert;
                let depth_end = self.terrain.elevation_at(en.x, en.y) - pipe.end_invert;
                let avg_depth = ((depth_start + depth_end) / 2.0).max(0.0);
                total_excavation += avg_depth * pipe.length;

                // Velocity check: Python uses max(start_node.demand, 1.0) / 1000.0
                // NOTE: 1.0 L/s floor, NOT 0.001 m3/s
                let flow = sn.demand.max(1.0) / 1000.0;
                let v = velocity(flow, pipe.diameter);
                if v < self.constraints.min_velocity {
                    violations += 1;
                }
                if v > self.constraints.max_velocity {
                    violations += 1;
                }
            }
        }

        // Pressure check: ALL nodes including TANK (Distribution does NOT skip TANK/Reservoir)
        let mut pressures: Vec<f64> = Vec::new();
        for node in &network.nodes {
            let p = node
                .metadata
                .get("pressure_mca")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            pressures.push(p);
            if p < self.constraints.min_pressure {
                violations += 1;
            }
            if p > self.constraints.max_pressure {
                violations += 1;
            }
        }

        // Population std dev (ddof=0): sqrt(sum((p - mean)^2) / N)
        let pressure_std = population_std(&pressures);

        // Count accessories
        let valve_count = network
            .nodes
            .iter()
            .filter(|n| n.node_type == NodeType::Valve)
            .count() as i64;
        let hydrant_count = network
            .nodes
            .iter()
            .filter(|n| n.node_type == NodeType::Hydrant)
            .count() as i64;

        let cost = 1.0 * total_length
            + 1.5 * total_excavation
            + 150.0 * violations as f64
            + 5.0 * pressure_std
            + 500.0 * valve_count as f64
            + 800.0 * hydrant_count as f64;

        let avg_excavation = total_excavation / total_length.max(1.0);

        Ok(SolutionScore {
            total_cost: cost,
            total_length,
            total_excavation,
            norm_violations: violations,
            pump_count: 0,
            details: serde_json::json!({
                "pipe_count": network.pipe_count(),
                "node_count": network.node_count(),
                "valve_count": valve_count,
                "hydrant_count": hydrant_count,
                "pressure_std": round2(pressure_std),
                "avg_excavation": avg_excavation,
            }),
        })
    }
}

// ── Solver trait impl ─────────────────────────────────────────────────────────

impl Solver for DistributionSolver {
    fn solve(&mut self, _params: &SolverParams) -> Result<Vec<Solution>, SolverError> {
        Err(SolverError::BuildFailed(
            "Use DistributionSolver::solve_distribution() with explicit source and demand_points"
                .to_string(),
        ))
    }

    fn evaluate(&self, network: &PipeNetwork) -> Result<SolutionScore, SolverError> {
        self.evaluate_distribution(network)
    }
}

// ── Private helpers ───────────────────────────────────────────────────────────

/// Population standard deviation (ddof=0) — mirrors Python `np.std(pressures)`.
///
/// Formula: sqrt(sum((p - mean)^2) / N)
fn population_std(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let n = values.len() as f64;
    let mean = values.iter().sum::<f64>() / n;
    let variance = values.iter().map(|&v| (v - mean).powi(2)).sum::<f64>() / n;
    variance.sqrt()
}

/// Round to 2 decimal places (mirrors Python `round(x, 2)`).
fn round2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

/// Python-style banker's rounding (round-half-to-even) for f64 → usize.
///
/// Python's `round()` uses banker's rounding for x.5 values.
/// Rust's `f64::round()` uses round-half-away-from-zero.
/// They differ only at exactly x.5 where x is an integer ± 0.5.
fn python_round(x: f64) -> usize {
    let floor = x.floor() as i64;
    let frac = x - x.floor();
    if (frac - 0.5).abs() < 1e-10 {
        // Exactly x.5: round to even
        if floor % 2 == 0 {
            floor as usize
        } else {
            (floor + 1) as usize
        }
    } else {
        x.round() as usize
    }
}

/// Sort a pair of strings into canonical (min, max) order.
#[inline]
fn sorted_pair(a: &str, b: &str) -> (String, String) {
    if a <= b {
        (a.to_string(), b.to_string())
    } else {
        (b.to_string(), a.to_string())
    }
}

/// Build routing adjacency for Dijkstra flow accumulation.
///
/// Edge weight = max(euclidean_distance, 0.1), mirroring Python's routing_graph.
fn build_routing_adj(
    combined_adj: &HashMap<String, Vec<String>>,
    nodes: &[NetworkNode],
    node_map: &HashMap<String, usize>,
) -> HashMap<String, Vec<(String, f64)>> {
    let mut routing: HashMap<String, Vec<(String, f64)>> = HashMap::new();

    for (u, nbrs) in combined_adj {
        let ui = match node_map.get(u) {
            Some(&i) => i,
            None => continue,
        };
        let un = &nodes[ui];
        for v in nbrs {
            let vi = match node_map.get(v.as_str()) {
                Some(&i) => i,
                None => continue,
            };
            let vn = &nodes[vi];
            let dist = ((un.x - vn.x).powi(2) + (un.y - vn.y).powi(2))
                .sqrt()
                .max(0.1);
            routing
                .entry(u.clone())
                .or_default()
                .push((v.clone(), dist));
        }
    }

    routing
}

/// Single-source Dijkstra shortest paths from source to all reachable nodes.
///
/// Returns a map: node_id → path (list of node IDs from source to that node).
/// Mirrors Python `nx.single_source_dijkstra_path(routing_graph, source_node, weight="length")`.
fn dijkstra_all_paths(
    source: &str,
    adj: &HashMap<String, Vec<(String, f64)>>,
) -> HashMap<String, Vec<String>> {
    // dist[node] = best cost from source
    let mut dist: HashMap<String, f64> = HashMap::new();
    // prev[node] = predecessor node ID on shortest path
    let mut prev: HashMap<String, String> = HashMap::new();

    dist.insert(source.to_string(), 0.0);

    // Min-heap: (Reverse(cost_bits), node_id)
    let mut heap: BinaryHeap<(std::cmp::Reverse<u64>, String)> = BinaryHeap::new();
    heap.push((std::cmp::Reverse(0u64), source.to_string()));

    while let Some((std::cmp::Reverse(cost_bits), u)) = heap.pop() {
        let u_cost = f64::from_bits(cost_bits);

        // Skip if stale
        if let Some(&best) = dist.get(&u) {
            if u_cost > best + 1e-12 {
                continue;
            }
        }

        if let Some(nbrs) = adj.get(&u) {
            for (v, w) in nbrs {
                let new_cost = u_cost + w;
                let better = dist.get(v.as_str()).is_none_or(|&d| new_cost < d - 1e-12);
                if better {
                    dist.insert(v.clone(), new_cost);
                    prev.insert(v.clone(), u.clone());
                    heap.push((std::cmp::Reverse(new_cost.to_bits()), v.clone()));
                }
            }
        }
    }

    // Reconstruct paths for all reachable nodes
    let mut paths: HashMap<String, Vec<String>> = HashMap::new();
    paths.insert(source.to_string(), vec![source.to_string()]);

    for node in dist.keys() {
        if node == source {
            continue;
        }
        let mut path: Vec<String> = Vec::new();
        let mut cur = node.as_str();
        loop {
            path.push(cur.to_string());
            match prev.get(cur) {
                Some(p) => cur = p.as_str(),
                None => break,
            }
        }
        path.reverse();
        paths.insert(node.clone(), path);
    }

    paths
}
