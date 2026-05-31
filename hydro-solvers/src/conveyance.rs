//! ConveyanceSolver — single pipeline from source to destination (linea de conduccion).
//!
//! Faithful port of `hydro_engine/solvers/conveyance.py` (ConveyanceSolver class).
//!
//! ## Algorithm
//! 1. Build terrain graph from topography.
//! 2. Find k-shortest paths from source to destination (ALL paths, starting from index 0).
//! 3. For each path, select a SINGLE diameter for the whole pipeline using Hazen-Williams.
//! 4. Calculate pressure profile inline (running_pressure -= hf + dz).
//! 5. Find high/low interior points → place air valves / blowoff valves.
//! 6. Score: cost = 1.5*L + 2.0*excav + 200*violations + 500*structure_count.
//!
//! ## Key differences from WaterSupplySolver
//! - k_paths[0] IS used (not skipped).
//! - Single uniform diameter per path (not per-segment).
//! - Pressure propagated inline in _build_pipeline (not via BFS afterwards).
//! - evaluate() reads flow key `"design_flow"` (not `"flow_m3s"`).
//! - VALVE nodes count as structure_count in evaluate().
//! - Source node_type = TANK, destination = RESERVOIR, interior = JUNCTION.
//! - pump_count is always 0.

use std::collections::HashMap;

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

// ── ConveyanceSolver ──────────────────────────────────────────────────────────

/// Solver for single conveyance pipelines (linea de conduccion).
///
/// Faithful port of Python `ConveyanceSolver`. Finds the optimal single pipeline
/// route from a source (tank/spring) to a destination (tank/distribution point),
/// dimensions the pipeline using Hazen-Williams, and places air/blowoff valves
/// at critical elevation points.
pub struct ConveyanceSolver {
    pub terrain: TerrainModel,
    pub constraints: DesignConstraints,
}

impl ConveyanceSolver {
    /// Create a new ConveyanceSolver.
    pub fn new(terrain: TerrainModel, constraints: DesignConstraints) -> Self {
        ConveyanceSolver {
            terrain,
            constraints,
        }
    }

    /// Solve the conveyance pipeline with explicit source/destination parameters.
    ///
    /// Faithful port of Python `solve()`. Enumerates k-shortest paths, builds a
    /// single pipeline per path, places special structures (air/blowoff valves),
    /// evaluates each, and returns solutions sorted by total_cost ascending.
    #[allow(clippy::too_many_arguments)]
    pub fn solve_conveyance(
        &mut self,
        source: (f64, f64),
        destination: (f64, f64),
        design_flow: f64,
        source_head: f64,
        network_name: &str,
        material: PipeMaterial,
        params: &SolverParams,
    ) -> Result<Vec<Solution>, SolverError> {
        let cost_fn = CostFunction::new(
            CostWeights {
                w_length: 1.5,
                w_excavation: 2.0,
                w_slope_penalty: 1.0,
                w_pump_penalty: 30.0,
                w_crossing: 0.0,
            },
            self.constraints.min_slope,
            self.constraints.max_slope,
            self.constraints.min_cover,
        );

        let mut graph = TerrainGraph::new(cost_fn);
        graph.from_grid(params.grid_resolution, &self.terrain);

        // Perturb edge weights BEFORE k-paths if route_variant > 0
        if params.route_variant > 0 {
            graph.perturb_weights(params.route_variant);
        }

        let source_node = graph
            .nearest_node(source.0, source.1)
            .map_err(|_| SolverError::NoOutlet)?;
        let dest_node = graph
            .nearest_node(destination.0, destination.1)
            .map_err(|_| SolverError::NoOutlet)?;

        if source_node == dest_node {
            return Err(SolverError::BuildFailed(
                "Source and destination map to the same graph node".to_string(),
            ));
        }

        // Compute k-shortest paths
        let k_paths =
            match graph.k_shortest_paths(&source_node, &dest_node, params.num_alternatives) {
                Ok(paths) if !paths.is_empty() => paths,
                Ok(_) | Err(_) => {
                    // Fallback to single shortest path
                    let path = graph
                        .shortest_path(&source_node, &dest_node)
                        .map_err(|e| SolverError::BuildFailed(e.to_string()))?;
                    vec![path]
                }
            };

        let hw_c = hw_coefficient(material.as_str());
        let mut solutions: Vec<Solution> = Vec::new();
        let mut alternative_errors: Vec<String> = Vec::new();

        // NOTE: Python iterates enumerate(k_paths, start=1) — all paths including [0].
        for (i, path) in k_paths.iter().enumerate() {
            let alt_name = if i == 0 {
                network_name.to_string()
            } else {
                format!("{}_alt{}", network_name, i + 1)
            };

            match self.build_pipeline(
                &graph,
                path,
                &source_node,
                &dest_node,
                &alt_name,
                design_flow,
                material,
                hw_c,
                source_head,
                params.cover_factor,
            ) {
                Ok(mut network) => {
                    // Find high/low points and place special structures
                    let (high_points, low_points) = self.find_high_low_points(path, &graph);
                    self.place_special_structures(
                        &mut network,
                        &high_points,
                        &low_points,
                        params.valve_spacing,
                    );

                    let score = self
                        .evaluate(&network)
                        .map_err(|e| SolverError::BuildFailed(e.to_string()))?;

                    solutions.push(Solution {
                        rank: (i + 1) as i64,
                        network,
                        score,
                        metadata: SolutionMetadata::default(),
                    });
                }
                Err(e) => {
                    alternative_errors.push(format!("Conveyance path {} failed: {}", i + 1, e));
                }
            }
        }

        if solutions.is_empty() {
            return Err(SolverError::BuildFailed(format!(
                "Failed to build any conveyance solution: {}",
                alternative_errors.join("; ")
            )));
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

    // ── Pipeline builder ───────────────────────────────────────────────────────

    /// Build a single pipeline along a path.
    ///
    /// Faithful port of Python `_build_pipeline()`:
    /// - Creates nodes with TANK (source) / RESERVOIR (dest) / JUNCTION (interior).
    /// - Selects ONE diameter for the entire pipeline (using total path length).
    /// - Upgrades diameter if velocity exceeds max_velocity.
    /// - Propagates pressure inline: running_pressure -= hf + dz.
    /// - Skips segments shorter than 0.1m.
    /// - Pipe IDs: `"Pipe - ({i+1})"` (1-indexed loop counter over path).
    #[allow(clippy::too_many_arguments)]
    fn build_pipeline(
        &self,
        graph: &TerrainGraph,
        path: &[String],
        source_node: &str,
        dest_node: &str,
        name: &str,
        design_flow: f64,
        material: PipeMaterial,
        hw_c: f64,
        source_head: f64,
        cover_factor: f64,
    ) -> Result<PipeNetwork, SolverError> {
        // Create nodes along path
        let mut nodes: Vec<NetworkNode> = Vec::new();
        let mut node_map: HashMap<String, usize> = HashMap::new();

        for nid in path {
            let coord = graph
                .get_node_coord(nid)
                .ok_or_else(|| SolverError::BuildFailed(format!("node {} not found", nid)))?;
            let z = graph.get_node_elevation(nid).ok_or_else(|| {
                SolverError::BuildFailed(format!("elevation for {} not found", nid))
            })?;

            let ntype = if nid == source_node {
                NodeType::Tank
            } else if nid == dest_node {
                NodeType::Reservoir
            } else {
                NodeType::Junction
            };

            let idx = nodes.len();
            node_map.insert(nid.clone(), idx);
            let mut node = NetworkNode::new(nid.as_str(), coord.0, coord.1, z, ntype);
            node.rim_elevation = z;
            nodes.push(node);
        }

        // Calculate TOTAL path length for single-diameter selection
        let mut total_path_length = 0.0_f64;
        for i in 0..path.len().saturating_sub(1) {
            let (x1, y1) = graph.get_node_coord(&path[i]).unwrap_or((0.0, 0.0));
            let (x2, y2) = graph.get_node_coord(&path[i + 1]).unwrap_or((0.0, 0.0));
            total_path_length += ((x1 - x2).powi(2) + (y1 - y2).powi(2)).sqrt();
        }

        // Select a single diameter for the entire pipeline
        let mut available: Vec<f64> = self.constraints.available_diameters.clone();
        available.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        let diameter = required_diameter_pressure(
            design_flow,
            total_path_length,
            source_head,
            hw_c,
            &available,
        );

        // Upgrade if velocity exceeds max_velocity
        let v_flow = velocity(design_flow, diameter);
        let diameter = if v_flow > self.constraints.max_velocity {
            available
                .iter()
                .filter(|&&d| d > diameter)
                .find(|&&d| velocity(design_flow, d) <= self.constraints.max_velocity)
                .copied()
                .unwrap_or(diameter)
        } else {
            diameter
        };

        // Create pipes and propagate pressure inline
        let mut running_pressure = source_head;

        // Set source node pressure
        if let Some(&src_idx) = node_map.get(source_node) {
            nodes[src_idx]
                .metadata
                .insert("pressure_mca".to_string(), Value::from(source_head));
        }

        let mut pipes: Vec<NetworkPipe> = Vec::new();
        let cover = self.constraints.min_cover * cover_factor;

        for i in 0..path.len().saturating_sub(1) {
            let u = &path[i];
            let v = &path[i + 1];

            let start_idx = match node_map.get(u) {
                Some(&idx) => idx,
                None => continue,
            };
            let end_idx = match node_map.get(v) {
                Some(&idx) => idx,
                None => continue,
            };

            let length = {
                let sn = &nodes[start_idx];
                let en = &nodes[end_idx];
                ((sn.x - en.x).powi(2) + (sn.y - en.y).powi(2)).sqrt()
            };

            // Python: if length < 0.1: continue
            if length < 0.1 {
                continue;
            }

            let start_z = nodes[start_idx].z;
            let end_z = nodes[end_idx].z;

            // Inverts: z - min_cover * cover_factor
            let start_invert = start_z - cover;
            let end_invert = end_z - cover;

            let mut pipe_meta: HashMap<String, Value> = HashMap::new();
            pipe_meta.insert("design_flow".to_string(), Value::from(design_flow));

            pipes.push(NetworkPipe {
                id: PipeId::new(format!("Pipe - ({})", i + 1)),
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
                design_flow,
                waypoints: vec![],
                metadata: pipe_meta,
            });

            // Inline pressure propagation: P_end = P_start - hf - dz
            // Python: hf = float(head_loss(design_flow, diameter, length, hw_c))
            //         dz = end_node.z - start_node.z
            //         running_pressure = running_pressure - hf - dz
            let hf = head_loss(design_flow, diameter, length, hw_c);
            let dz = end_z - start_z;
            running_pressure -= hf + dz;

            // Store pressure at end node (NOT clamped — may go negative)
            nodes[end_idx]
                .metadata
                .insert("pressure_mca".to_string(), Value::from(running_pressure));
        }

        Ok(PipeNetwork::new(name, nodes, pipes))
    }

    // ── High/low point detection ───────────────────────────────────────────────

    /// Find indices of local high and low points along the path.
    ///
    /// Faithful port of Python `_find_high_low_points()`:
    /// - High point: z[i] > z[i-1] AND z[i] > z[i+1]
    /// - Low point:  z[i] < z[i-1] AND z[i] < z[i+1]
    /// - Only interior nodes (indices 1..len-1) are checked.
    fn find_high_low_points(
        &self,
        path: &[String],
        graph: &TerrainGraph,
    ) -> (Vec<usize>, Vec<usize>) {
        if path.len() < 3 {
            return (Vec::new(), Vec::new());
        }

        let elevations: Vec<f64> = path
            .iter()
            .map(|nid| graph.get_node_elevation(nid).unwrap_or(0.0))
            .collect();

        let mut high_points: Vec<usize> = Vec::new();
        let mut low_points: Vec<usize> = Vec::new();

        for i in 1..elevations.len().saturating_sub(1) {
            let z_prev = elevations[i - 1];
            let z_curr = elevations[i];
            let z_next = elevations[i + 1];

            if z_curr > z_prev && z_curr > z_next {
                high_points.push(i);
            } else if z_curr < z_prev && z_curr < z_next {
                low_points.push(i);
            }
        }

        (high_points, low_points)
    }

    // ── Special structure placement ────────────────────────────────────────────

    /// Place air valves at high points and blowoff valves at low points.
    ///
    /// Faithful port of Python `_place_special_structures()`:
    /// - Iterates high_points first, then low_points.
    /// - Skips nodes outside [0, len(nodes)) range.
    /// - Only places valve if _far_enough: cumulative length from pipes[:idx] differs
    ///   from ALL previously placed positions by at least valve_spacing.
    /// - Cumulative length: sum(pipe.length for pipe in network.pipes[:idx]).
    ///   This is the Python oracle's exact behavior (RISK-7: pipe index alignment).
    fn place_special_structures(
        &self,
        network: &mut PipeNetwork,
        high_points: &[usize],
        low_points: &[usize],
        valve_spacing: f64,
    ) {
        let mut placed_positions: Vec<f64> = Vec::new();

        let n_nodes = network.nodes.len();
        let n_pipes = network.pipes.len();

        // Closure: _far_enough(idx) = True if no placed position is within valve_spacing
        // Python: cum = sum(p.length for p in network.pipes[:idx])
        let compute_cum = |idx: usize| -> f64 {
            network.pipes[..idx.min(n_pipes)]
                .iter()
                .map(|p| p.length)
                .sum::<f64>()
        };

        let far_enough = |idx: usize, placed: &[f64]| -> bool {
            if placed.is_empty() {
                return true;
            }
            let cum = compute_cum(idx);
            placed.iter().all(|&pos| (cum - pos).abs() >= valve_spacing)
        };

        // High points → air valves
        for &idx in high_points {
            if idx < n_nodes && far_enough(idx, &placed_positions) {
                let cum = compute_cum(idx);
                network.nodes[idx].node_type = NodeType::Valve;
                network.nodes[idx]
                    .metadata
                    .insert("valve_type".to_string(), Value::from("air_valve"));
                network.nodes[idx].metadata.insert(
                    "description".to_string(),
                    Value::from("Valvula de admision y expulsion de aire"),
                );
                placed_positions.push(cum);
            }
        }

        // Low points → blowoff valves
        for &idx in low_points {
            if idx < n_nodes && far_enough(idx, &placed_positions) {
                let cum = compute_cum(idx);
                network.nodes[idx].node_type = NodeType::Valve;
                network.nodes[idx]
                    .metadata
                    .insert("valve_type".to_string(), Value::from("blowoff_valve"));
                network.nodes[idx].metadata.insert(
                    "description".to_string(),
                    Value::from("Valvula de desague (purga)"),
                );
                placed_positions.push(cum);
            }
        }
    }
}

// ── Solver trait impl ─────────────────────────────────────────────────────────

impl Solver for ConveyanceSolver {
    fn solve(&mut self, _params: &SolverParams) -> Result<Vec<Solution>, SolverError> {
        Err(SolverError::BuildFailed(
            "Use ConveyanceSolver::solve_conveyance() with explicit source and destination"
                .to_string(),
        ))
    }

    fn evaluate(&self, network: &PipeNetwork) -> Result<SolutionScore, SolverError> {
        let total_length = network.total_length;
        let mut total_excavation = 0.0_f64;
        let mut violations: i64 = 0;
        let mut structure_count: i64 = 0;

        for pipe in &network.pipes {
            let start_node = network.get_node(&pipe.start_node_id);
            let end_node = network.get_node(&pipe.end_node_id);

            if let (Some(sn), Some(en)) = (start_node, end_node) {
                // Average excavation depth
                // Python: avg_depth = ((terrain.elev_at(sx,sy) - start_invert) + (terrain.elev_at(ex,ey) - end_invert)) / 2.0
                // terrain.elevation_at(x,y) == node.z for on-grid nodes
                let depth_start = self.terrain.elevation_at(sn.x, sn.y) - pipe.start_invert;
                let depth_end = self.terrain.elevation_at(en.x, en.y) - pipe.end_invert;
                let avg_depth = ((depth_start + depth_end) / 2.0).max(0.0);
                total_excavation += avg_depth * pipe.length;

                // Velocity violations
                // Python: flow = pipe.metadata.get("design_flow", 0.010)
                let flow = pipe
                    .metadata
                    .get("design_flow")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.010);

                let v = velocity(flow, pipe.diameter);
                if v < self.constraints.min_velocity {
                    violations += 1;
                }
                if v > self.constraints.max_velocity {
                    violations += 1;
                }
            }
        }

        // Pressure violations and structure count (from node metadata)
        for node in &network.nodes {
            let pressure = node
                .metadata
                .get("pressure_mca")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);

            // Skip pressure check for TANK and RESERVOIR nodes
            match node.node_type {
                NodeType::Tank | NodeType::Reservoir => {}
                _ => {
                    if pressure < self.constraints.min_pressure {
                        violations += 1;
                    }
                    if pressure > self.constraints.max_pressure {
                        violations += 1;
                    }
                }
            }

            // Count VALVE nodes as structures
            if node.node_type == NodeType::Valve {
                structure_count += 1;
            }
        }

        // Cost formula: 1.5*L + 2.0*excav + 200*violations + 500*structure_count
        let cost = 1.5 * total_length
            + 2.0 * total_excavation
            + 200.0 * violations as f64
            + 500.0 * structure_count as f64;

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
                "structure_count": structure_count,
            }),
        })
    }
}
