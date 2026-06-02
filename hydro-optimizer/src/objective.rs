//! 5-objective function for hydraulic network optimization (REQ-003).
//!
//! Faithful port of Python oracle `hydro_engine/optimization/objective.py`.
//!
//! # Objectives (all minimized)
//! 0. `cost_usd`           — pipe material + manholes + excavation
//! 1. `excavation_m3`      — avg cover depth × trench width × length
//! 2. `pumping_score`      — pump station CAPEX + lifecycle OPEX
//! 3. `interference_count` — crossings with existing networks violating separation
//! 4. `resilience_deficit` — 1 - Todini_Ir (pressurized) or 1 - Rc (gravity)
//!
//! # Design §12 determinism rule
//! No `HashMap`; sorted BTreeMap for diameter catalogue.

use std::collections::BTreeMap;

use hydro_types::enums::NodeType;
use hydro_types::network::PipeNetwork;
use hydro_types::response::Solution;

// ── Unit cost constants (oracle class-level defaults) ─────────────────────────

/// Pipe unit cost catalogue: diameter (m) → cost per metre (USD).
/// Oracle `ObjectiveFunction.PIPE_COST_PER_M` exactly.
fn pipe_cost_catalogue() -> &'static BTreeMap<u64, f64> {
    use std::sync::OnceLock;
    static CATALOGUE: OnceLock<BTreeMap<u64, f64>> = OnceLock::new();
    CATALOGUE.get_or_init(|| {
        // Keys are diameter × 1_000_000 as integer to allow BTreeMap ordering.
        // Exact oracle values from objective.py PIPE_COST_PER_M dict.
        let entries: &[(f64, f64)] = &[
            (0.05, 8.0),
            (0.075, 10.0),
            (0.1, 12.0),
            (0.15, 15.0),
            (0.2, 18.0),
            (0.25, 22.0),
            (0.3, 28.0),
            (0.38, 40.0),
            (0.45, 55.0),
            (0.5, 65.0),
            (0.6, 85.0),
            (0.61, 85.0),
            (0.76, 120.0),
            (0.91, 160.0),
            (1.07, 220.0),
            (1.22, 300.0),
        ];
        entries
            .iter()
            .map(|&(d, c)| ((d * 1_000_000.0).round() as u64, c))
            .collect()
    })
}

/// Look up the cost per metre for a diameter; finds nearest catalogue entry.
///
/// Mirrors `_pipe_unit_cost` in oracle `objective.py`.
fn pipe_unit_cost(diameter: f64) -> f64 {
    let cat = pipe_cost_catalogue();
    let key = (diameter * 1_000_000.0).round() as u64;
    if let Some(&cost) = cat.get(&key) {
        return cost;
    }
    // Nearest-diameter lookup (argmin of |d_catalogue - diameter|)
    cat.iter()
        .min_by(|(&k1, _), (&k2, _)| {
            let d1 = (k1 as i64 - key as i64).unsigned_abs();
            let d2 = (k2 as i64 - key as i64).unsigned_abs();
            d1.cmp(&d2)
        })
        .map(|(_, &c)| c)
        .unwrap_or(28.0) // fallback: 0.3m cost
}

const MANHOLE_COST: f64 = 2_500.0;
const EXCAVATION_COST_M3: f64 = 12.0;
const TRENCH_WIDTH: f64 = 0.8;
const PUMP_STATION_COST: f64 = 50_000.0;

/// Minimum horizontal separation between new pipe and existing network (m).
const MIN_HORIZONTAL_SEPARATION: f64 = 1.0;
/// Minimum vertical separation at a crossing (m).
const MIN_VERTICAL_SEPARATION: f64 = 0.3;
/// Todini minimum pressure head at demand nodes (mca, oracle H_MIN).
const H_MIN: f64 = 15.0;

// ── ExistingNetworkSpec ───────────────────────────────────────────────────────

/// Input descriptor for an existing underground network used in interference
/// checking. Mirrors the `existing_networks` list passed to the Python oracle
/// `ObjectiveFunction.__init__`.
#[derive(Debug, Clone)]
pub struct ExistingNetworkSpec {
    /// Polyline vertices as (x, y) pairs (≥2 points required).
    pub coords: Vec<(f64, f64)>,
    /// Average burial depth of the existing network (m). Default 1.5 m.
    pub depth: Option<f64>,
}

// ── ObjectiveFunction ─────────────────────────────────────────────────────────

/// 5-objective evaluation: `(cost, excavation, pumping, interference, resilience)`.
///
/// Pure, deterministic given the same `Solution` input.
/// Mirrors `ObjectiveFunction` in Python oracle `objective.py`.
pub struct ObjectiveFunction {
    /// Pre-parsed existing network polylines with their burial depths.
    existing: Vec<(Vec<(f64, f64)>, f64)>,
}

impl ObjectiveFunction {
    /// Construct with optional existing networks for interference checking.
    ///
    /// - `existing_networks`: list of polyline specs (coords + optional depth).
    ///   Networks with fewer than 2 coordinate points are silently ignored.
    /// - `_terrain`: reserved for future terrain-elevation lookup (not used in
    ///   the current oracle version — elevations are embedded in node.z).
    pub fn new(existing_networks: Vec<ExistingNetworkSpec>, _terrain: Option<()>) -> Self {
        let existing = existing_networks
            .into_iter()
            .filter(|n| n.coords.len() >= 2)
            .map(|n| (n.coords, n.depth.unwrap_or(1.5)))
            .collect();
        ObjectiveFunction { existing }
    }

    /// Evaluate the solution and return the 5-objective array.
    ///
    /// Returns `[cost_usd, excavation_m3, pumping_score, interference_count,
    /// resilience_deficit]`.
    pub fn score(&self, solution: &Solution) -> [f64; 5] {
        let excavation = self.calc_excavation(solution);
        let cost = self.calc_cost(solution, excavation);
        let pumping = self.calc_pumping(solution);
        let interference = self.calc_interference(solution);
        let resilience = self.calc_resilience(solution);
        [cost, excavation, pumping, interference, resilience]
    }

    // ── Objective 0: Construction cost ───────────────────────────────────────

    fn calc_cost(&self, solution: &Solution, excavation_m3: f64) -> f64 {
        let network = &solution.network;
        let mut pipe_cost = 0.0_f64;
        for pipe in &network.pipes {
            // Use effective_length (polyline length) for routed pipes.
            pipe_cost += pipe_unit_cost(pipe.diameter) * pipe.effective_length;
        }
        let manhole_cost = network.node_count() as f64 * MANHOLE_COST;
        let excavation_cost = excavation_m3 * EXCAVATION_COST_M3;
        pipe_cost + manhole_cost + excavation_cost
    }

    // ── Objective 1: Excavation volume ───────────────────────────────────────

    fn calc_excavation(&self, solution: &Solution) -> f64 {
        let network = &solution.network;
        let mut total = 0.0_f64;
        for pipe in &network.pipes {
            let start_node = network.get_node(&pipe.start_node_id);
            let end_node = network.get_node(&pipe.end_node_id);
            if let (Some(sn), Some(en)) = (start_node, end_node) {
                let start_cover = sn.z - pipe.start_invert;
                let end_cover = en.z - pipe.end_invert;
                let avg_depth = ((start_cover + end_cover) / 2.0).max(0.0);
                total += avg_depth * pipe.effective_length * TRENCH_WIDTH;
            }
        }
        total
    }

    // ── Objective 2: Pumping requirement ─────────────────────────────────────

    fn calc_pumping(&self, solution: &Solution) -> f64 {
        let network = &solution.network;
        let pump_count = solution.score.pump_count;

        // Identify pipes with "bypassed_by_pump" metadata or adverse slope.
        // The oracle computes lifecycle OPEX using shaft_power_kw + energy model.
        // For this port, we implement the CAPEX term (pump_count × PUMP_STATION_COST)
        // plus a simplified OPEX using pump_count (oracle approach for pipes
        // without explicit flow/lift metadata). See oracle `_calc_pumping` for
        // the full lifecycle model used when per-pipe flow/lift is available.
        let mut extra_pumps: i64 = 0;
        // Track explicit PUMP node id strings to avoid double-counting
        let explicit_pump_node_ids: std::collections::BTreeSet<&str> = network
            .nodes
            .iter()
            .filter(|n| n.node_type == NodeType::Pump)
            .map(|n| n.id.as_str())
            .collect();

        let mut total_lifecycle_opex = 0.0_f64;
        for pipe in &network.pipes {
            // Check for explicitly pumped pipes (bypassed_by_pump metadata)
            if pipe
                .metadata
                .get("bypassed_by_pump")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
            {
                let lift = pipe
                    .metadata
                    .get("pump_lift")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                let flow = if pipe.design_flow > 0.0 {
                    pipe.design_flow
                } else {
                    pipe.metadata
                        .get("design_flow")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0)
                };
                total_lifecycle_opex += lifecycle_opex_usd(flow, lift);
                continue;
            }
            // Adverse slope (end_invert > start_invert) — implicit pump needed
            if pipe.end_invert > pipe.start_invert && pipe.length > 0.0 {
                let lift = pipe.end_invert - pipe.start_invert;
                let nominal_flow = if pipe.design_flow > 0.0 {
                    pipe.design_flow
                } else {
                    // Fallback: π × r² at 1 m/s velocity (conservative worst-case)
                    std::f64::consts::PI * (pipe.diameter / 2.0).powi(2)
                };
                total_lifecycle_opex += lifecycle_opex_usd(nominal_flow, lift);
                // Count a new implicit pump only if downstream node is not already explicit
                if !explicit_pump_node_ids.contains(pipe.end_node_id.as_str()) {
                    extra_pumps += 1;
                }
            }
        }

        let total_pump_count = pump_count + extra_pumps;
        (total_pump_count as f64) * PUMP_STATION_COST + total_lifecycle_opex
    }

    // ── Objective 3: Interference with existing networks ─────────────────────

    fn calc_interference(&self, solution: &Solution) -> f64 {
        if self.existing.is_empty() {
            return 0.0;
        }
        let network = &solution.network;
        let mut violations = 0_u64;

        for pipe in &network.pipes {
            let start_node = network.get_node(&pipe.start_node_id);
            let end_node = network.get_node(&pipe.end_node_id);
            let (sn, en) = match (start_node, end_node) {
                (Some(s), Some(e)) => (s, e),
                _ => continue,
            };

            // Build polyline for this pipe: start → waypoints → end
            let mut pipe_coords: Vec<(f64, f64)> = Vec::with_capacity(2 + pipe.waypoints.len());
            pipe_coords.push((sn.x, sn.y));
            for wp in &pipe.waypoints {
                pipe_coords.push((wp[0], wp[1]));
            }
            pipe_coords.push((en.x, en.y));

            let start_cover = sn.z - pipe.start_invert;
            let end_cover = en.z - pipe.end_invert;
            let pipe_avg_depth = (start_cover + end_cover) / 2.0;

            for (existing_coords, existing_depth) in &self.existing {
                if polylines_intersect(&pipe_coords, existing_coords) {
                    // Lines cross — check vertical separation
                    let vertical_sep = (pipe_avg_depth - existing_depth).abs();
                    if vertical_sep < MIN_VERTICAL_SEPARATION {
                        violations += 1;
                    }
                } else {
                    // No crossing — check minimum horizontal separation
                    let dist = polyline_min_distance(&pipe_coords, existing_coords);
                    if dist < MIN_HORIZONTAL_SEPARATION {
                        violations += 1;
                    }
                }
            }
        }

        violations as f64
    }

    // ── Objective 4: Resilience deficit ──────────────────────────────────────

    fn calc_resilience(&self, solution: &Solution) -> f64 {
        let network = &solution.network;
        let has_pressure = network
            .nodes
            .iter()
            .filter(|n| !matches!(n.node_type, NodeType::Tank | NodeType::Reservoir))
            .any(|n| n.metadata.contains_key("pressure_mca"));

        if has_pressure {
            1.0 - todini_index(network)
        } else {
            1.0 - connectivity_redundancy(network)
        }
    }
}

// ── Todini resilience index ───────────────────────────────────────────────────

/// Todini (2000) resilience index for pressurized networks.
///
/// Faithful port of oracle `_todini_index`.
/// Uses piezometric heads to prevent Ir > 1 on downhill networks.
pub(crate) fn todini_index(network: &PipeNetwork) -> f64 {
    let demand_nodes: Vec<_> = network
        .nodes
        .iter()
        .filter(|n| n.demand > 0.0 && !matches!(n.node_type, NodeType::Tank | NodeType::Reservoir))
        .collect();

    if demand_nodes.is_empty() {
        return 0.0;
    }

    // numerator = sum_i( (demand_i/1000) * max(pressure_mca_i - H_MIN, 0) )
    let numerator: f64 = demand_nodes
        .iter()
        .map(|n| {
            let pressure = n
                .metadata
                .get("pressure_mca")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            (n.demand / 1000.0) * (pressure - H_MIN).max(0.0)
        })
        .sum();

    let source_nodes: Vec<_> = network
        .nodes
        .iter()
        .filter(|n| matches!(n.node_type, NodeType::Tank | NodeType::Reservoir))
        .collect();

    if source_nodes.is_empty() {
        return 0.0;
    }

    let total_demand_m3s: f64 = demand_nodes.iter().map(|n| n.demand / 1000.0).sum();
    let num_sources = source_nodes.len() as f64;

    // mean_source_head = mean( z_k + pressure_mca_k ) for tank/reservoir nodes
    let mean_source_head: f64 = source_nodes
        .iter()
        .map(|n| {
            let p = n
                .metadata
                .get("pressure_mca")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            n.z + p
        })
        .sum::<f64>()
        / num_sources;

    let input_power = mean_source_head * total_demand_m3s;

    // min_demand_head = sum_i( (demand_i/1000) * (z_i + H_MIN) )
    let min_demand_head: f64 = demand_nodes
        .iter()
        .map(|n| (n.demand / 1000.0) * (n.z + H_MIN))
        .sum();

    let denominator = input_power - min_demand_head;
    if denominator <= 0.0 {
        return 0.0;
    }

    (numerator / denominator).clamp(0.0, 1.0)
}

// ── Connectivity redundancy ───────────────────────────────────────────────────

/// Connectivity redundancy Rc = (E - N + 1) / (2N - 5) for gravity networks.
///
/// Faithful port of oracle `_connectivity_redundancy`.
pub(crate) fn connectivity_redundancy(network: &PipeNetwork) -> f64 {
    let e = network.pipe_count() as f64;
    let n = network.node_count() as f64;
    if n < 3.0 {
        return 0.0;
    }
    let denom = 2.0 * n - 5.0;
    if denom <= 0.0 {
        return 0.0;
    }
    ((e - n + 1.0) / denom).clamp(0.0, 1.0)
}

// ── Lifecycle OPEX helper ─────────────────────────────────────────────────────

/// Simplified lifecycle OPEX using the oracle default parameters.
///
/// Uses the oracle's formula: P_kw = ρgQH/η, η=0.7 per REQ-003.
/// annual = P_kw × hours_per_year × tariff, total = annual × lifetime.
///
/// Oracle defaults: hours_per_year=4000, tariff=0.10 USD/kWh, lifetime=20 yr.
fn lifecycle_opex_usd(flow_m3s: f64, lift_m: f64) -> f64 {
    if flow_m3s <= 0.0 || lift_m <= 0.0 {
        return 0.0;
    }
    const RHO: f64 = 1000.0; // kg/m³
    const G: f64 = 9.81; // m/s²
                         // Pump efficiency η = 0.7 per REQ-003 (Python oracle hardcodes this constant; not a placeholder).
    const ETA: f64 = 0.7;
    const HOURS_PER_YEAR: f64 = 4000.0;
    const TARIFF: f64 = 0.10;
    const LIFETIME: f64 = 20.0;

    let power_kw = RHO * G * flow_m3s * lift_m / ETA / 1000.0;
    let annual = power_kw * HOURS_PER_YEAR * TARIFF;
    annual * LIFETIME
}

// ── 2D geometry helpers (deterministic, no HashMap) ──────────────────────────

type Pt = (f64, f64);
type Seg = (Pt, Pt);

/// Check whether two polylines (sequences of 2D line segments) intersect.
///
/// Replaces Shapely `LineString.intersects`. Uses segment-segment intersection
/// tests with the standard cross-product formulation.
fn polylines_intersect(a: &[Pt], b: &[Pt]) -> bool {
    for i in 0..a.len().saturating_sub(1) {
        for j in 0..b.len().saturating_sub(1) {
            if seg_intersect((a[i], a[i + 1]), (b[j], b[j + 1])) {
                return true;
            }
        }
    }
    false
}

/// Test whether segment AB intersects segment CD (proper crossing or T-junction).
fn seg_intersect(ab: Seg, cd: Seg) -> bool {
    let ((ax, ay), (bx, by)) = ab;
    let ((cx, cy), (dx, dy)) = cd;
    let d1x = bx - ax;
    let d1y = by - ay;
    let d2x = dx - cx;
    let d2y = dy - cy;
    let cross = d1x * d2y - d1y * d2x;

    if cross.abs() < 1e-12 {
        return false; // parallel or collinear — treated as non-intersecting
    }

    let t = ((cx - ax) * d2y - (cy - ay) * d2x) / cross;
    let u = ((cx - ax) * d1y - (cy - ay) * d1x) / cross;
    (0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u)
}

/// Minimum Euclidean distance between two 2D polylines (minimum over all segment pairs).
fn polyline_min_distance(a: &[Pt], b: &[Pt]) -> f64 {
    let mut min_dist = f64::INFINITY;
    for i in 0..a.len().saturating_sub(1) {
        for j in 0..b.len().saturating_sub(1) {
            let d = seg_to_seg_distance((a[i], a[i + 1]), (b[j], b[j + 1]));
            if d < min_dist {
                min_dist = d;
            }
        }
    }
    min_dist
}

/// Minimum distance between point P and segment AB.
fn pt_to_seg_distance(p: Pt, seg: Seg) -> f64 {
    let (px, py) = p;
    let ((ax, ay), (bx, by)) = seg;
    let dx = bx - ax;
    let dy = by - ay;
    let len2 = dx * dx + dy * dy;
    if len2 < 1e-14 {
        // Degenerate segment (a == b)
        return ((px - ax).powi(2) + (py - ay).powi(2)).sqrt();
    }
    let t = ((px - ax) * dx + (py - ay) * dy) / len2;
    let t = t.clamp(0.0, 1.0);
    let closest_x = ax + t * dx;
    let closest_y = ay + t * dy;
    ((px - closest_x).powi(2) + (py - closest_y).powi(2)).sqrt()
}

/// Minimum distance between segment AB and segment CD.
///
/// If segments intersect, distance is 0. Otherwise returns min of
/// the four point-to-segment distances (endpoints).
fn seg_to_seg_distance(ab: Seg, cd: Seg) -> f64 {
    if seg_intersect(ab, cd) {
        return 0.0;
    }
    let (a, b) = ab;
    let (c, d) = cd;
    let d1 = pt_to_seg_distance(a, cd);
    let d2 = pt_to_seg_distance(b, cd);
    let d3 = pt_to_seg_distance(c, ab);
    let d4 = pt_to_seg_distance(d, ab);
    d1.min(d2).min(d3).min(d4)
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests (REQ-003)
// Previously in tests/pr8b_objective.rs. Moved inline (REQ-014).
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use hydro_types::enums::{NodeType, PipeMaterial};
    use hydro_types::network::{NetworkNode, NetworkPipe, NodeId, PipeId, PipeNetwork};
    use hydro_types::response::{Solution, SolutionMetadata, SolutionScore};

    fn make_solution(
        nodes: Vec<NetworkNode>,
        pipes: Vec<NetworkPipe>,
        score: SolutionScore,
    ) -> Solution {
        Solution {
            rank: 1,
            network: PipeNetwork::new("test", nodes, pipes),
            score,
            metadata: SolutionMetadata::default(),
        }
    }

    fn make_node(
        id: &str,
        x: f64,
        y: f64,
        z: f64,
        node_type: NodeType,
        demand: f64,
    ) -> NetworkNode {
        let mut n = NetworkNode::new(id, x, y, z, node_type);
        n.demand = demand;
        n
    }

    fn make_node_with_pressure(
        id: &str,
        x: f64,
        y: f64,
        z: f64,
        node_type: NodeType,
        demand: f64,
        pressure_mca: f64,
    ) -> NetworkNode {
        let mut n = make_node(id, x, y, z, node_type, demand);
        n.metadata
            .insert("pressure_mca".to_owned(), serde_json::json!(pressure_mca));
        n
    }

    fn make_pipe(
        id: &str,
        start: &str,
        end: &str,
        length: f64,
        diameter: f64,
        si: f64,
        ei: f64,
    ) -> NetworkPipe {
        NetworkPipe {
            id: PipeId::new(id),
            start_node_id: NodeId::new(start),
            end_node_id: NodeId::new(end),
            length,
            effective_length: length,
            diameter,
            material: PipeMaterial::Pvc,
            roughness: 0.009,
            start_invert: si,
            end_invert: ei,
            slope: (si - ei) / length,
            design_flow: 0.01,
            waypoints: vec![],
            metadata: Default::default(),
        }
    }

    #[test]
    fn test_pumping_cost_zero_pumps() {
        let nodes = vec![
            make_node("n1", 0.0, 0.0, 20.0, NodeType::Reservoir, 0.0),
            make_node("n2", 100.0, 0.0, 15.0, NodeType::Junction, 0.5),
        ];
        let pipes = vec![make_pipe("p1", "n1", "n2", 100.0, 0.3, 18.0, 14.0)];
        let solution = make_solution(
            nodes,
            pipes,
            SolutionScore {
                pump_count: 0,
                ..Default::default()
            },
        );
        let of = ObjectiveFunction::new(vec![], None);
        let obj = of.score(&solution);
        assert_eq!(obj[2], 0.0, "zero pumps → pumping cost must be 0.0");
    }

    #[test]
    fn test_pumping_cost_with_pump_stations() {
        let nodes = vec![
            make_node("n1", 0.0, 0.0, 10.0, NodeType::Reservoir, 0.0),
            make_node("n2", 100.0, 0.0, 10.0, NodeType::Pump, 0.0),
            make_node("n3", 200.0, 0.0, 10.0, NodeType::Junction, 0.5),
        ];
        let pipes = vec![
            make_pipe("p1", "n1", "n2", 100.0, 0.3, 9.0, 8.5),
            make_pipe("p2", "n2", "n3", 100.0, 0.3, 8.5, 8.0),
        ];
        let solution = make_solution(
            nodes,
            pipes,
            SolutionScore {
                pump_count: 2,
                ..Default::default()
            },
        );
        let of = ObjectiveFunction::new(vec![], None);
        let obj = of.score(&solution);
        assert!(
            obj[2] >= 100_000.0,
            "2 pump stations → cost ≥ 100_000, got {}",
            obj[2]
        );
    }

    #[test]
    fn test_excavation_formula() {
        let nodes = vec![
            make_node("n1", 0.0, 0.0, 10.0, NodeType::Junction, 0.0),
            make_node("n2", 100.0, 0.0, 9.0, NodeType::Junction, 0.0),
        ];
        let pipes = vec![make_pipe("p1", "n1", "n2", 100.0, 0.3, 8.5, 7.5)];
        let solution = make_solution(nodes, pipes, SolutionScore::default());
        let of = ObjectiveFunction::new(vec![], None);
        let obj = of.score(&solution);
        let expected = 1.5_f64 * 0.8 * 100.0;
        assert!(
            (obj[1] - expected).abs() < 1e-6,
            "expected {expected}, got {}",
            obj[1]
        );
    }

    #[test]
    fn test_cost_formula() {
        let nodes = vec![
            make_node("n1", 0.0, 0.0, 10.0, NodeType::Junction, 0.0),
            make_node("n2", 100.0, 0.0, 9.0, NodeType::Junction, 0.0),
        ];
        let pipes = vec![make_pipe("p1", "n1", "n2", 100.0, 0.3, 8.5, 7.5)];
        let solution = make_solution(nodes, pipes, SolutionScore::default());
        let of = ObjectiveFunction::new(vec![], None);
        let obj = of.score(&solution);
        let expected_cost = 2_800.0 + 5_000.0 + 1_440.0;
        assert!(
            (obj[0] - expected_cost).abs() < 1.0,
            "expected {expected_cost}, got {}",
            obj[0]
        );
    }

    #[test]
    fn test_interference_count_zero_existing_networks() {
        let nodes = vec![
            make_node("n1", 0.0, 0.0, 10.0, NodeType::Junction, 0.0),
            make_node("n2", 100.0, 0.0, 9.0, NodeType::Junction, 0.0),
        ];
        let pipes = vec![make_pipe("p1", "n1", "n2", 100.0, 0.3, 8.5, 7.5)];
        let solution = make_solution(nodes, pipes, SolutionScore::default());
        let of = ObjectiveFunction::new(vec![], None);
        let obj = of.score(&solution);
        assert_eq!(
            obj[3], 0.0,
            "no existing networks → interference_count must be 0"
        );
    }

    #[test]
    fn test_interference_count_matches() {
        let nodes = vec![
            make_node("n1", 0.0, 0.0, 10.0, NodeType::Junction, 0.0),
            make_node("n2", 100.0, 0.0, 10.0, NodeType::Junction, 0.0),
        ];
        let pipes = vec![make_pipe("p1", "n1", "n2", 100.0, 0.3, 9.0, 9.0)];
        let existing = vec![ExistingNetworkSpec {
            coords: vec![(50.0, -10.0), (50.0, 10.0)],
            depth: Some(1.1),
        }];
        let solution = make_solution(nodes, pipes, SolutionScore::default());
        let of = ObjectiveFunction::new(existing, None);
        let obj = of.score(&solution);
        assert_eq!(
            obj[3] as i64, 1,
            "one crossing with sep 0.1m < 0.3m → 1, got {}",
            obj[3]
        );
    }

    #[test]
    fn test_todini_ir_deficit_zero_for_adequate_pressure() {
        let nodes = vec![
            make_node_with_pressure("src", 0.0, 0.0, 50.0, NodeType::Reservoir, 0.0, 0.0),
            make_node_with_pressure("n1", 100.0, 0.0, 10.0, NodeType::Junction, 1.0, 30.0),
        ];
        let pipes = vec![make_pipe("p1", "src", "n1", 100.0, 0.3, 49.0, 9.5)];
        let solution = make_solution(nodes, pipes, SolutionScore::default());
        let of = ObjectiveFunction::new(vec![], None);
        let obj = of.score(&solution);
        assert!(
            obj[4] < 0.5,
            "adequate pressure → deficit < 0.5, got {}",
            obj[4]
        );
    }

    #[test]
    fn test_todini_ir_deficit_positive_for_insufficient_head() {
        let nodes = vec![
            make_node_with_pressure("src", 0.0, 0.0, 50.0, NodeType::Reservoir, 0.0, 0.0),
            make_node_with_pressure("n1", 100.0, 0.0, 10.0, NodeType::Junction, 1.0, 5.0),
        ];
        let pipes = vec![make_pipe("p1", "src", "n1", 100.0, 0.3, 49.0, 9.5)];
        let solution = make_solution(nodes, pipes, SolutionScore::default());
        let of = ObjectiveFunction::new(vec![], None);
        let obj = of.score(&solution);
        assert!(
            obj[4] > 0.0,
            "pressure 5mca < H_MIN → deficit > 0, got {}",
            obj[4]
        );
    }

    #[test]
    fn test_gravity_resilience_uses_connectivity() {
        let nodes = vec![
            make_node("n1", 0.0, 0.0, 20.0, NodeType::Reservoir, 0.0),
            make_node("n2", 50.0, 0.0, 18.0, NodeType::Junction, 0.0),
            make_node("n3", 100.0, 0.0, 16.0, NodeType::Junction, 0.0),
            make_node("n4", 150.0, 0.0, 14.0, NodeType::Outlet, 0.0),
        ];
        let pipes = vec![
            make_pipe("p1", "n1", "n2", 50.0, 0.3, 19.0, 17.0),
            make_pipe("p2", "n2", "n3", 50.0, 0.3, 17.0, 15.0),
            make_pipe("p3", "n3", "n4", 50.0, 0.3, 15.0, 13.0),
        ];
        let solution = make_solution(nodes, pipes, SolutionScore::default());
        let of = ObjectiveFunction::new(vec![], None);
        let obj = of.score(&solution);
        assert!(
            (obj[4] - 1.0).abs() < 1e-9,
            "linear chain N=4, E=3 → Rc=0 → deficit=1.0, got {}",
            obj[4]
        );
    }

    #[test]
    fn test_all_five_objectives_are_nonnegative() {
        let nodes = vec![
            make_node("n1", 0.0, 0.0, 20.0, NodeType::Reservoir, 0.0),
            make_node("n2", 100.0, 0.0, 15.0, NodeType::Junction, 0.5),
        ];
        let pipes = vec![make_pipe("p1", "n1", "n2", 100.0, 0.3, 18.0, 14.0)];
        let solution = make_solution(nodes, pipes, SolutionScore::default());
        let of = ObjectiveFunction::new(vec![], None);
        let obj = of.score(&solution);
        for (i, v) in obj.iter().enumerate() {
            assert!(*v >= 0.0, "objectives[{i}] = {v} is negative");
        }
    }

    #[test]
    fn test_objective_function_is_deterministic() {
        let nodes = vec![
            make_node("n1", 0.0, 0.0, 20.0, NodeType::Reservoir, 0.0),
            make_node("n2", 100.0, 0.0, 15.0, NodeType::Junction, 0.5),
        ];
        let pipes = vec![make_pipe("p1", "n1", "n2", 100.0, 0.3, 18.0, 14.0)];
        let solution = make_solution(nodes, pipes, SolutionScore::default());
        let of = ObjectiveFunction::new(vec![], None);
        let a = of.score(&solution);
        let b = of.score(&solution);
        for i in 0..5 {
            assert_eq!(a[i], b[i]);
        }
    }

    #[test]
    fn test_todini_ir_formula_matches_oracle() {
        let nodes = vec![
            make_node_with_pressure("src", 0.0, 0.0, 100.0, NodeType::Reservoir, 0.0, 0.0),
            make_node_with_pressure("n1", 100.0, 0.0, 0.0, NodeType::Junction, 2.0, 40.0),
        ];
        let pipes = vec![make_pipe("p1", "src", "n1", 100.0, 0.3, 99.0, -0.5)];
        let solution = make_solution(nodes, pipes, SolutionScore::default());
        let of = ObjectiveFunction::new(vec![], None);
        let obj = of.score(&solution);
        let ir_expected = 0.05_f64 / 0.17_f64;
        let deficit_expected = 1.0 - ir_expected.clamp(0.0, 1.0);
        assert!(
            (obj[4] - deficit_expected).abs() < 1e-6,
            "expected {deficit_expected:.6}, got {:.6}",
            obj[4]
        );
    }
}
