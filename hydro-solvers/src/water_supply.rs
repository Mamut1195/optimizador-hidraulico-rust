//! WaterSupplySolver — pressurized water distribution network design.
//!
//! Faithful port of `hydro_engine/solvers/water_supply.py` (WaterSupplySolver class).
//!
//! ## Algorithm
//! 1. Build terrain graph from topography.
//! 2. Find MST connecting all demand nodes to source (tank/reservoir).
//! 3. BFS-orient edges source-outward.
//! 4. Dimension pipes using Hazen-Williams equation.
//! 5. Add cross-connections for loop redundancy.
//! 6. Check pressure at all nodes (BFS from source).
//! 7. Score: cost = 1.0*L + 1.5*excav + 150.0*violations.

use std::collections::{HashMap, HashSet, VecDeque};

use hydro_hydraulics::hazen_williams::{
    head_loss, hw_coefficient, required_diameter_pressure, velocity,
};
use hydro_terrain::{CostFunction, CostWeights, TerrainGraph, TerrainModel};
use hydro_types::{
    response::SolutionMetadata, DesignConstraints, NetworkNode, NetworkPipe, NodeId, NodeType,
    PipeId, PipeMaterial, PipeNetwork, Solution, SolutionScore,
};
use petgraph::graph::NodeIndex;
use serde_json::Value;

use crate::error::SolverError;
use crate::graph::SolverGraph;
use crate::solver::{Solver, SolverParams};

// ── WaterSupplySolver ─────────────────────────────────────────────────────────

/// Solver for pressurized water distribution networks (red de distribución).
///
/// Faithful port of Python `WaterSupplySolver`. Uses MST to connect demand nodes
/// to a source (tank/reservoir), then adds cross-connections and dimensions pipes
/// using Hazen-Williams.
pub struct WaterSupplySolver {
    pub terrain: TerrainModel,
    pub constraints: DesignConstraints,
    /// Demand point coordinates injected at construction time (used by `Solver::solve`).
    pub demand_points: Vec<(f64, f64)>,
    /// Source (tank/reservoir) coordinate injected at construction time (used by `Solver::solve`).
    pub source: (f64, f64),
    /// Per-node demand flow rate injected at construction time (used by `Solver::solve`).
    pub demand_per_node: f64,
    /// Pipe material injected at construction time (used by `Solver::solve`).
    pub material: PipeMaterial,
    /// Network name injected at construction time (used by `Solver::solve`).
    pub network_name: String,
}

impl WaterSupplySolver {
    /// Create a new WaterSupplySolver with all project-context fields required by
    /// `Solver::solve`.
    ///
    /// Field order: `(terrain, constraints, demand_points, source, demand_per_node,
    /// material, network_name)`. GA-driven values (`source_head`) are provided at
    /// solve time via `SolverParams`.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        terrain: TerrainModel,
        constraints: DesignConstraints,
        demand_points: Vec<(f64, f64)>,
        source: (f64, f64),
        demand_per_node: f64,
        material: PipeMaterial,
        network_name: String,
    ) -> Self {
        WaterSupplySolver {
            terrain,
            constraints,
            demand_points,
            source,
            demand_per_node,
            material,
            network_name,
        }
    }

    /// Solve the water supply network design with explicit demand/source parameters.
    ///
    /// This is the domain-specific entry point (mirrors Python `solve()`).
    /// The generic `Solver::solve()` delegates here.
    #[allow(clippy::too_many_arguments)]
    pub fn solve_water_supply(
        &mut self,
        demand_points: &[(f64, f64)],
        source: (f64, f64),
        source_head: f64,
        network_name: &str,
        demand_per_node: f64,
        material: PipeMaterial,
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

        // Validate: check that all demand nodes are reachable from source
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

        let all_terminal_set: HashSet<&str> = {
            let mut s: HashSet<&str> = demand_nodes.iter().map(|s| s.as_str()).collect();
            s.insert(source_node.as_str());
            s
        };

        if all_terminal_set.len() < 2 {
            return Err(SolverError::InsufficientTerminals);
        }

        // Get HW coefficient for material
        let hw_c = hw_coefficient(material.as_str());

        let mut solutions: Vec<Solution> = Vec::new();

        // Alternative 1: MST-based network with loop additions
        let mst_adj = graph
            .minimum_spanning_tree(&source_node)
            .map_err(|e| SolverError::BuildFailed(e.to_string()))?;

        let network = self
            .build_network_from_mst(
                &graph,
                &mst_adj,
                &source_node,
                &demand_nodes,
                network_name,
                demand_per_node,
                material,
                hw_c,
                source_head,
                params.diameter_offset,
                if params.network_type >= 1 {
                    params.loop_density
                } else {
                    0.0
                },
            )
            .map_err(SolverError::BuildFailed)?;

        let score = self.evaluate(&network)?;
        solutions.push(Solution {
            rank: 1,
            network,
            score,
            metadata: SolutionMetadata::default(),
        });

        // Alternative 2+: variations using k-shortest paths from farthest demand node
        if !demand_nodes.is_empty() && params.num_alternatives > 1 {
            let distances: Vec<(&str, f64)> = demand_nodes
                .iter()
                .filter(|n| n.as_str() != source_node.as_str())
                .filter_map(|nid| {
                    graph
                        .shortest_path_cost(nid, &source_node)
                        .ok()
                        .map(|c| (nid.as_str(), c))
                })
                .collect();

            if let Some((farthest, _)) = distances
                .iter()
                .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
            {
                let farthest = *farthest;
                if let Ok(k_paths) =
                    graph.k_shortest_paths(&source_node, farthest, params.num_alternatives)
                {
                    for (i, path) in k_paths[1..].iter().enumerate() {
                        let alt_name = format!("{}_alt{}", network_name, i + 2);
                        if let Ok(net) = self.build_network_from_path(
                            &graph,
                            path,
                            &demand_nodes,
                            &source_node,
                            &alt_name,
                            demand_per_node,
                            material,
                            hw_c,
                            source_head,
                            params.diameter_offset,
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

    // ── MST network builder ────────────────────────────────────────────────────

    /// Convert an MST into a pressurized PipeNetwork.
    ///
    /// Faithful port of Python `_build_network_from_mst()`.
    /// BFS orientation is SOURCE-OUTWARD (opposite of sewer's outlet-inward).
    #[allow(clippy::too_many_arguments)]
    fn build_network_from_mst(
        &self,
        graph: &TerrainGraph,
        mst_adj: &HashMap<String, Vec<(String, f64)>>,
        source_node: &str,
        demand_nodes: &[String],
        name: &str,
        demand_per_node: f64,
        material: PipeMaterial,
        hw_c: f64,
        source_head: f64,
        diameter_offset: i32,
        loop_density: f64,
    ) -> Result<PipeNetwork, String> {
        let demand_set: HashSet<&str> = demand_nodes.iter().map(|s| s.as_str()).collect();

        // Load MST adjacency into SolverGraph (undirected: both directions in mst_adj).
        let mst_sg = mst_adj_to_solver_graph(mst_adj);

        // BFS from source to build the oriented (source-outward) SolverGraph.
        // Python: directed.add_edge(current, neighbor)  ← source → outward
        let source_idx = mst_sg
            .node_index(source_node)
            .ok_or_else(|| format!("source node '{}' not in MST", source_node))?;

        let mut oriented = SolverGraph::new();
        let mut visited: HashSet<NodeIndex<u32>> = HashSet::new();
        let mut queue: VecDeque<NodeIndex<u32>> = VecDeque::new();

        visited.insert(source_idx);
        queue.push_back(source_idx);

        while let Some(cur) = queue.pop_front() {
            for (nb, w) in mst_sg.sorted_neighbors(cur) {
                if visited.insert(nb) {
                    // Edge goes FROM source outward: cur → nb
                    oriented.add_edge(mst_sg.node_id(cur), mst_sg.node_id(nb), w);
                    queue.push_back(nb);
                }
            }
        }

        // Ensure all MST nodes appear in oriented (source may be isolated)
        for idx in mst_sg.node_indices() {
            oriented.add_node(mst_sg.node_id(idx));
        }

        // Topological sort of oriented (source first, leaves last).
        let topo_order = oriented
            .sorted_kahn_topo_sort()
            .map_err(|e| e.to_string())?;

        // Create nodes indexed by topo position.
        let mut nodes: Vec<NetworkNode> = Vec::new();
        // node_map: &str → index into nodes vec. &str borrows from oriented (stable).
        let mut node_map: HashMap<&str, usize> = HashMap::new();

        for &idx in &topo_order {
            let nid = oriented.node_id(idx);
            let coord = graph
                .get_node_coord(nid)
                .ok_or_else(|| format!("node {} not found in graph", nid))?;
            let z = graph
                .get_node_elevation(nid)
                .ok_or_else(|| format!("elevation for {} not found", nid))?;

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

            let pos = nodes.len();
            node_map.insert(nid, pos);
            let mut node = NetworkNode::new(nid, coord.0, coord.1, z, ntype);
            node.rim_elevation = z;
            node.demand = demand;
            nodes.push(node);
        }

        // Accumulated demand per node (leaves to source via topo order reverse).
        // demand_at_node keyed by NodeIndex — no String clones.
        let mut demand_at_node: HashMap<NodeIndex<u32>, f64> = topo_order
            .iter()
            .map(|&idx| {
                let nid = oriented.node_id(idx);
                let base = if demand_set.contains(nid) {
                    demand_per_node
                } else {
                    0.0
                };
                (idx, base)
            })
            .collect();

        // Predecessors for reverse accumulation — NodeIndex only.
        let mut predecessors: HashMap<NodeIndex<u32>, Vec<NodeIndex<u32>>> = HashMap::new();
        for &parent in &topo_order {
            for (child, _) in oriented.sorted_neighbors(parent) {
                predecessors.entry(child).or_default().push(parent);
            }
        }

        // Accumulate: reversed(topo_order) = leaves first (for source-outward tree)
        for &idx in topo_order.iter().rev() {
            let d = demand_at_node.get(&idx).copied().unwrap_or(0.0);
            if let Some(preds) = predecessors.get(&idx) {
                for &pred in preds {
                    *demand_at_node.entry(pred).or_insert(0.0) += d;
                }
            }
        }

        // Sorted available diameters (to_vec — no clone call)
        let mut available: Vec<f64> = self.constraints.available_diameters.to_vec();
        available.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        // Create pipes with Hazen-Williams dimensioning (topo order: source first).
        let mut pipe_counter = 0usize;
        let mut pipes: Vec<NetworkPipe> = Vec::new();
        let mut running_total_length = 0.0_f64;

        for &u_idx in &topo_order {
            let u_id = oriented.node_id(u_idx);
            for (v_idx, _w) in oriented.sorted_neighbors(u_idx) {
                let v_id = oriented.node_id(v_idx);

                let start_pos = match node_map.get(u_id) {
                    Some(&i) => i,
                    None => continue,
                };
                let end_pos = match node_map.get(v_id) {
                    Some(&i) => i,
                    None => continue,
                };

                let length = {
                    let sn = &nodes[start_pos];
                    let en = &nodes[end_pos];
                    ((sn.x - en.x).powi(2) + (sn.y - en.y).powi(2)).sqrt()
                };

                if length < 0.1 {
                    continue;
                }

                // Flow through this pipe = accumulated demand downstream (at v)
                let flow = demand_at_node
                    .get(&v_idx)
                    .copied()
                    .unwrap_or(demand_per_node)
                    .max(demand_per_node);

                // Available head for this segment (dynamic: uses running_total_length)
                let available_head =
                    (source_head * (length / running_total_length.max(length))).max(1.0);

                let pressure_diameter =
                    required_diameter_pressure(flow, length, available_head, hw_c, &available);

                let diameter = self.select_pressure_diameter(
                    flow,
                    length,
                    available_head,
                    hw_c,
                    Some(pressure_diameter),
                );

                let diameter = if diameter_offset > 0 {
                    if let Some(idx) = available.iter().position(|&d| (d - diameter).abs() < 1e-12)
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

                let start_z = nodes[start_pos].z;
                let end_z = nodes[end_pos].z;
                let start_invert = start_z - self.constraints.min_cover;
                let end_invert = end_z - self.constraints.min_cover;

                pipe_counter += 1;
                running_total_length += length;

                let mut pipe_meta: HashMap<String, Value> = HashMap::new();
                pipe_meta.insert("flow_m3s".to_string(), Value::from(flow));

                pipes.push(NetworkPipe {
                    id: PipeId::new(format!("Pipe - ({})", pipe_counter)),
                    start_node_id: NodeId::new(u_id),
                    end_node_id: NodeId::new(v_id),
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
                    metadata: pipe_meta,
                });
            }
        }

        // Build oriented &str adjacency (source-outward) for loop connections.
        // Borrows from oriented — no String allocation needed.
        let mut directed_adj: HashMap<&str, Vec<&str>> = HashMap::new();
        for &u_idx in &topo_order {
            let u_id = oriented.node_id(u_idx);
            directed_adj.entry(u_id).or_default();
            for (v_idx, _) in oriented.sorted_neighbors(u_idx) {
                directed_adj
                    .entry(u_id)
                    .or_default()
                    .push(oriented.node_id(v_idx));
            }
        }

        let mut network = PipeNetwork::new(name, nodes, pipes);

        // Add cross-connections for loop redundancy
        self.add_loop_connections(
            graph,
            &mut network,
            &directed_adj,
            demand_per_node,
            material,
            hw_c,
            loop_density,
            &available,
        )?;

        // Check pressures at all nodes (BFS from source)
        self.check_pressures(&mut network, source_node, source_head, hw_c);

        Ok(network)
    }

    // ── Loop connections ───────────────────────────────────────────────────────

    /// Add short cross-connections to create loops for redundancy.
    ///
    /// Faithful port of Python `_add_loop_connections()`.
    #[allow(clippy::too_many_arguments)]
    fn add_loop_connections(
        &self,
        graph: &TerrainGraph,
        network: &mut PipeNetwork,
        directed: &HashMap<&str, Vec<&str>>,
        demand_per_node: f64,
        material: PipeMaterial,
        hw_c: f64,
        loop_density: f64,
        available: &[f64],
    ) -> Result<(), String> {
        // Build tree_edges (both directions) from directed (source-outward)
        let mut tree_edges: HashSet<(&str, &str)> = HashSet::new();
        for (&u, children) in directed {
            for &v in children {
                tree_edges.insert((u, v));
                tree_edges.insert((v, u));
            }
        }

        let tree_nodes: HashSet<&str> = directed.keys().copied().collect();

        let base_max = 3usize.max(tree_nodes.len() / 5);
        let max_loops = 1usize.max((base_max as f64 * loop_density) as usize);

        // Candidate edges: in full graph, between tree nodes, not in tree
        let all_edges = graph.all_edges();
        let mut candidates: Vec<(String, String, f64)> = all_edges
            .into_iter()
            .filter_map(|(u, v, w, _)| {
                if tree_nodes.contains(u.as_str())
                    && tree_nodes.contains(v.as_str())
                    && !tree_edges.contains(&(u.as_str(), v.as_str()))
                {
                    Some((u, v, w))
                } else {
                    None
                }
            })
            .collect();

        // Sort by weight (prefer short/cheap connections)
        candidates.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));

        // Collect all loop pipes first, then rebuild the network once.
        let mut added = 0usize;
        let mut pipe_counter = network.pipe_count();
        let mut new_pipes: Vec<NetworkPipe> = Vec::new();

        for (u, v, _) in candidates {
            if added >= max_loops {
                break;
            }

            // Extract (x, y, z) from nodes without cloning NetworkNode.
            let (start_x, start_y, start_z) = match network.get_node(&NodeId::new(u.as_str())) {
                Some(n) => (n.x, n.y, n.z),
                None => continue,
            };
            let (end_x, end_y, end_z) = match network.get_node(&NodeId::new(v.as_str())) {
                Some(n) => (n.x, n.y, n.z),
                None => continue,
            };

            let length = {
                let dx = start_x - end_x;
                let dy = start_y - end_y;
                (dx * dx + dy * dy).sqrt()
            };

            if length < 0.1 {
                continue;
            }

            pipe_counter += 1;

            let pressure_diameter =
                required_diameter_pressure(demand_per_node, length, 10.0, hw_c, available);

            let diameter = self.select_pressure_diameter(
                demand_per_node,
                length,
                10.0,
                hw_c,
                Some(pressure_diameter),
            );

            let start_invert = start_z - self.constraints.min_cover;
            let end_invert = end_z - self.constraints.min_cover;

            let mut pipe_meta: HashMap<String, Value> = HashMap::new();
            pipe_meta.insert("flow_m3s".to_string(), Value::from(demand_per_node));

            new_pipes.push(NetworkPipe {
                id: PipeId::new(format!("Pipe - ({})", pipe_counter)),
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
                design_flow: demand_per_node,
                waypoints: vec![],
                metadata: pipe_meta,
            });

            added += 1;
        }

        // Rebuild network once if any loop pipes were added.
        if !new_pipes.is_empty() {
            let mut all_pipes = std::mem::take(&mut network.pipes);
            all_pipes.extend(new_pipes);
            let all_nodes = std::mem::take(&mut network.nodes);
            let name = network.name.as_str();
            *network = PipeNetwork::new(name, all_nodes, all_pipes);
        }

        Ok(())
    }

    // ── Pressure check ─────────────────────────────────────────────────────────

    /// Calculate pressure at each node by BFS from source.
    ///
    /// Faithful port of Python `_check_pressures()`.
    /// Energy balance: P_node = P_upstream - hf - dz
    /// Stores pressure in node.metadata["pressure_mca"].
    fn check_pressures(
        &self,
        network: &mut PipeNetwork,
        source_node: &str,
        source_head: f64,
        hw_c: f64,
    ) {
        let source_id = NodeId::new(source_node);

        // Find source node index
        let src_idx = match network.nodes.iter().position(|n| n.id == source_id) {
            Some(i) => i,
            None => return,
        };

        // Set source pressure
        network.nodes[src_idx]
            .metadata
            .insert("pressure_mca".to_string(), Value::from(source_head));

        let mut pressures: HashMap<String, f64> = HashMap::new();
        pressures.insert(source_node.to_string(), source_head);

        // BFS from source through pipes (undirected adjacency)
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

            // Get current node's z
            let current_z = match network.nodes.iter().find(|n| n.id == current_id) {
                Some(n) => n.z,
                None => continue,
            };

            // Get adjacency for this node (list of (neighbor_id_str, pipe_index)).
            // We extract the String from NodeId to release the borrow on network.
            let adjacency: Vec<(String, usize)> = network
                .adjacency
                .get(&current_id)
                .map(|v| v.iter().map(|(nid, pi)| (nid.0.to_owned(), *pi)).collect())
                .unwrap_or_default();

            for (nb_str, pipe_idx) in adjacency {
                if visited.contains(&nb_str) {
                    continue;
                }

                let pipe = &network.pipes[pipe_idx];

                // Get flow from metadata — fallback uses neighbor.demand
                let neighbor_demand = network
                    .nodes
                    .iter()
                    .find(|n| n.id.0 == nb_str)
                    .map(|n| n.demand)
                    .unwrap_or(0.0);

                let flow = pipe
                    .metadata
                    .get("flow_m3s")
                    .and_then(|v| v.as_f64())
                    .unwrap_or_else(|| neighbor_demand.max(1.0) / 1000.0);

                let hf = head_loss(flow, pipe.diameter, pipe.length, hw_c);

                // Get neighbor z
                let neighbor_z = match network.nodes.iter().find(|n| n.id.0 == nb_str) {
                    Some(n) => n.z,
                    None => continue,
                };

                let dz = neighbor_z - current_z;
                let pressure = current_pressure - hf - dz;

                // Insert pressure and mark visited — use to_owned() (no extra allocation vs clone).
                pressures.insert(nb_str.to_owned(), pressure);

                // Store pressure in neighbor's metadata
                if let Some(n_idx) = network.nodes.iter().position(|n| n.id.0 == nb_str) {
                    network.nodes[n_idx]
                        .metadata
                        .insert("pressure_mca".to_string(), Value::from(pressure));
                }

                visited.insert(nb_str.to_owned());
                queue.push_back(nb_str);
            }
        }
    }

    // ── Path network builder ───────────────────────────────────────────────────

    /// Build a network along a single path (for alternative routes).
    ///
    /// Faithful port of Python `_build_network_from_path()`.
    #[allow(clippy::too_many_arguments)]
    fn build_network_from_path(
        &self,
        graph: &TerrainGraph,
        path: &[String],
        demand_nodes: &[String],
        source_node: &str,
        name: &str,
        demand_per_node: f64,
        material: PipeMaterial,
        hw_c: f64,
        source_head: f64,
        diameter_offset: i32,
    ) -> Result<PipeNetwork, String> {
        let demand_set: HashSet<&str> = demand_nodes.iter().map(|s| s.as_str()).collect();

        // Create nodes along path
        let mut nodes: Vec<NetworkNode> = Vec::new();
        let mut node_map: HashMap<&str, usize> = HashMap::new();

        for nid in path {
            let coord = graph
                .get_node_coord(nid)
                .ok_or_else(|| format!("node {} not found in graph", nid))?;
            let z = graph
                .get_node_elevation(nid)
                .ok_or_else(|| format!("elevation for {} not found", nid))?;

            let ntype = if nid == source_node {
                NodeType::Tank
            } else if demand_set.contains(nid.as_str()) {
                NodeType::Service
            } else {
                NodeType::Junction
            };

            let demand = if demand_set.contains(nid.as_str()) {
                demand_per_node * 1000.0
            } else {
                0.0
            };

            let idx = nodes.len();
            node_map.insert(nid.as_str(), idx);
            let mut node = NetworkNode::new(nid.as_str(), coord.0, coord.1, z, ntype);
            node.rim_elevation = z;
            node.demand = demand;
            nodes.push(node);
        }

        let mut available: Vec<f64> = self.constraints.available_diameters.to_vec();
        available.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        // Create pipes along path (source outward: path[i] → path[i+1])
        let mut pipes: Vec<NetworkPipe> = Vec::new();

        for window in path.windows(2) {
            let u = &window[0];
            let v = &window[1];

            let start_idx = match node_map.get(u.as_str()) {
                Some(&idx) => idx,
                None => continue,
            };
            let end_idx = match node_map.get(v.as_str()) {
                Some(&idx) => idx,
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

            let flow = demand_per_node.max(0.001);
            // Python: available_head = max(source_head * 0.3, 1.0)
            let available_head = (source_head * 0.3_f64).max(1.0);

            let pressure_diameter =
                required_diameter_pressure(flow, length, available_head, hw_c, &available);

            let diameter = self.select_pressure_diameter(
                flow,
                length,
                available_head,
                hw_c,
                Some(pressure_diameter),
            );

            let diameter = if diameter_offset > 0 {
                if let Some(idx) = available.iter().position(|&d| (d - diameter).abs() < 1e-12) {
                    let new_idx =
                        (idx as i32 + diameter_offset).min((available.len() - 1) as i32) as usize;
                    available[new_idx]
                } else {
                    diameter
                }
            } else {
                diameter
            };

            let pipe_idx = pipes.len();
            let start_z = nodes[start_idx].z;
            let end_z = nodes[end_idx].z;
            let start_invert = start_z - self.constraints.min_cover;
            let end_invert = end_z - self.constraints.min_cover;

            let mut pipe_meta: HashMap<String, Value> = HashMap::new();
            pipe_meta.insert("flow_m3s".to_string(), Value::from(flow));

            pipes.push(NetworkPipe {
                id: PipeId::new(format!("Pipe - ({})", pipe_idx + 1)),
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
                metadata: pipe_meta,
            });
        }

        let mut network = PipeNetwork::new(name, nodes, pipes);

        // Check pressures
        self.check_pressures(&mut network, source_node, source_head, hw_c);

        Ok(network)
    }

    // ── Diameter selection ─────────────────────────────────────────────────────

    /// Select a pressure pipe diameter that also respects velocity limits.
    ///
    /// Faithful port of Python `_select_pressure_diameter()`.
    fn select_pressure_diameter(
        &self,
        flow: f64,
        length: f64,
        available_head: f64,
        hw_c: f64,
        minimum: Option<f64>,
    ) -> f64 {
        let mut available: Vec<f64> = self
            .constraints
            .available_diameters
            .iter()
            .filter(|&&d| d >= self.constraints.min_diameter)
            .copied()
            .collect();
        available.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        // candidates: diameters at or above the minimum pressure diameter.
        let start = if let Some(min_d) = minimum {
            available.partition_point(|&d| d < min_d)
        } else {
            0
        };
        let candidates = if start < available.len() {
            &available[start..]
        } else {
            available.as_slice()
        };

        // First: find diameters where hf <= available_head AND velocity is in [min, max]
        let feasible = candidates.iter().find(|&&d| {
            let hf = head_loss(flow, d, length, hw_c);
            let v = velocity(flow, d);
            hf <= available_head
                && v >= self.constraints.min_velocity
                && v <= self.constraints.max_velocity
        });

        if let Some(&d) = feasible {
            return d;
        }

        // Fallback: find diameters that are pressure-safe (hf <= available_head)
        let search: &[f64] = {
            let safe_start = available
                .iter()
                .position(|&d| head_loss(flow, d, length, hw_c) <= available_head);
            if let Some(i) = safe_start {
                &available[i..]
            } else {
                available.as_slice()
            }
        };

        // Among search, pick the one closest to velocity limits
        search
            .iter()
            .min_by(|&&da, &&db| {
                let va = velocity(flow, da);
                let vb = velocity(flow, db);
                let min_v = self.constraints.min_velocity;
                let max_v = self.constraints.max_velocity;
                let score_a = (va - min_v).abs().min((va - max_v).abs());
                let score_b = (vb - min_v).abs().min((vb - max_v).abs());
                score_a
                    .partial_cmp(&score_b)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .copied()
            .unwrap_or_else(|| available.last().copied().unwrap_or(0.1))
    }
}

// ── Solver trait impl ─────────────────────────────────────────────────────────

impl Solver for WaterSupplySolver {
    fn solve(&mut self, params: &SolverParams) -> Result<Vec<Solution>, SolverError> {
        // Guard: no demand points → no network can be built.
        if self.demand_points.is_empty() {
            return Err(SolverError::InsufficientTerminals);
        }

        // Clone fields that need a borrow release before the &mut self call.
        let demand_points = self.demand_points.clone();
        let network_name = self.network_name.clone();
        let source = self.source;
        let demand_per_node = self.demand_per_node;
        let material = self.material; // PipeMaterial is Copy

        let result = self.solve_water_supply(
            &demand_points,
            source,
            params.source_head,
            &network_name,
            demand_per_node,
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
        let mut total_excavation = 0.0_f64;
        let mut violations = 0i64;

        // Get HW coefficient from first pipe material (all same material)
        let _hw_c = network
            .pipes
            .first()
            .map(|p| hw_coefficient(p.material.as_str()))
            .unwrap_or(150.0);

        for pipe in &network.pipes {
            let start_node = network.get_node(&pipe.start_node_id);
            let end_node = network.get_node(&pipe.end_node_id);

            if let (Some(sn), Some(en)) = (start_node, end_node) {
                // Average excavation depth
                let depth_start = self.terrain.elevation_at(sn.x, sn.y) - pipe.start_invert;
                let depth_end = self.terrain.elevation_at(en.x, en.y) - pipe.end_invert;
                let avg_depth = ((depth_start + depth_end) / 2.0).max(0.0);
                total_excavation += avg_depth * pipe.length;

                // Check velocity bounds
                let flow = pipe
                    .metadata
                    .get("flow_m3s")
                    .and_then(|v| v.as_f64())
                    .unwrap_or_else(|| sn.demand.max(0.001) / 1000.0);

                let v = velocity(flow, pipe.diameter);
                if v < self.constraints.min_velocity {
                    violations += 1;
                }
                if v > self.constraints.max_velocity {
                    violations += 1;
                }
            }
        }

        // Check pressure at all non-source nodes
        for node in &network.nodes {
            if matches!(node.node_type, NodeType::Tank | NodeType::Reservoir) {
                continue;
            }
            let pressure = node
                .metadata
                .get("pressure_mca")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            if pressure < self.constraints.min_pressure {
                violations += 1;
            }
            if pressure > self.constraints.max_pressure {
                violations += 1;
            }
        }

        // Cost formula: 1.0*L + 1.5*excav + 150.0*violations
        let cost = 1.0 * total_length + 1.5 * total_excavation + 150.0 * violations as f64;

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
                "avg_excavation": avg_excavation,
            }),
        })
    }
}

// ── Private helpers ───────────────────────────────────────────────────────────

/// Load an MST adjacency map (as returned by `TerrainGraph::minimum_spanning_tree`)
/// into a `SolverGraph`.
///
/// The MST adjacency is undirected (both A→B and B→A are present). Edges are
/// inserted sorted by `(from_id, to_id)` to ensure deterministic insertion order
/// independent of `HashMap` iteration order.
///
/// This is the single conversion point from the `hydro_terrain` String-based API
/// to the crate-private `SolverGraph` — per design §4 "MST handling" for PR-C.
pub(crate) fn mst_adj_to_solver_graph(
    mst_adj: &HashMap<String, Vec<(String, f64)>>,
) -> SolverGraph {
    let mut sg = SolverGraph::new();

    // Collect all edges, sort by (from, to) for deterministic insertion order.
    let mut edges: Vec<(&str, &str, f64)> = mst_adj
        .iter()
        .flat_map(|(from, neighbors)| {
            neighbors
                .iter()
                .map(move |(to, w)| (from.as_str(), to.as_str(), *w))
        })
        .collect();
    edges.sort_by(|a, b| a.0.cmp(b.0).then(a.1.cmp(b.1)));

    for (from, to, w) in edges {
        sg.add_edge(from, to, w);
    }

    sg
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::SolverGraph;
    use std::collections::HashMap;

    // ── mst_adj_to_solver_graph — BFS visit order snapshot ──────────────────
    //
    // Fixture: a 5-node MST-like adjacency (undirected via bidirectional edges)
    //   source -> a, b
    //   a -> c
    //   b -> d
    //
    // BFS from source (SOURCE-OUTWARD) yields visit order: [source, a, b, c, d]
    // (sorted_neighbors sorts lex: "a" < "b", so a before b; then "c" < "d")
    #[test]
    fn ws_mst_adj_to_solver_graph_bfs_visit_order_snapshot() {
        // Build an MST adjacency map (bidirectional, as returned by TerrainGraph::minimum_spanning_tree)
        let mut mst_adj: HashMap<String, Vec<(String, f64)>> = HashMap::new();
        mst_adj.insert(
            "source".to_string(),
            vec![("a".to_string(), 1.0), ("b".to_string(), 1.0)],
        );
        mst_adj.insert(
            "a".to_string(),
            vec![("source".to_string(), 1.0), ("c".to_string(), 1.0)],
        );
        mst_adj.insert(
            "b".to_string(),
            vec![("source".to_string(), 1.0), ("d".to_string(), 1.0)],
        );
        mst_adj.insert("c".to_string(), vec![("a".to_string(), 1.0)]);
        mst_adj.insert("d".to_string(), vec![("b".to_string(), 1.0)]);

        // mst_adj_to_solver_graph loads the MST adjacency into a SolverGraph.
        let sg: SolverGraph = mst_adj_to_solver_graph(&mst_adj);

        // BFS from source (SOURCE-OUTWARD) produces [source, a, b, c, d]
        let source_idx = sg.node_index("source").expect("source must be in sg");
        let visit_order = sg.bfs_oriented(source_idx);
        let ids: Vec<&str> = visit_order.iter().map(|&idx| sg.node_id(idx)).collect();

        assert_eq!(
            ids,
            vec!["source", "a", "b", "c", "d"],
            "BFS from source in MST SolverGraph must yield lex-sorted visit order, got {:?}",
            ids
        );
    }
}
