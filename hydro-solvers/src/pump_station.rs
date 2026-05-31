//! PumpStationSolver — pump station design given flow and head requirements.
//!
//! Faithful port of `hydro_engine/solvers/pump_station.py` (PumpStationSolver class).
//!
//! ## Algorithm
//! 1. Compute static lift = discharge_elevation − suction_elevation.
//! 2. Compute TDH = abs(static_lift) + hf_suction + hf_discharge + minor_losses
//!    (minor losses via K_SUCTION=4.5 and K_DISCHARGE=7.0).
//! 3. Select pump: efficiency via np.interp on (Q, eta) table, shaft power → motor size.
//! 4. Size wet well: retention volume clamped to ≥ 2 m³, cube-root width, depth ≥ 1.5 m.
//! 5. Build schematic 5-node / 4-pipe PipeNetwork layout.
//! 6. Evaluate: cost = 1200*kW*pumps + 800*well_vol + 150*L + 5000*violations.
//!    NOTE: SolutionScore.total_excavation is REPURPOSED to carry wet_well_volume_m3.

use std::collections::HashMap;

use hydro_hydraulics::hazen_williams::{head_loss, hw_coefficient, velocity};
use hydro_terrain::TerrainModel;
use hydro_types::{
    DesignConstraints, NetworkNode, NetworkPipe, NodeId, NodeType, PipeId, PipeMaterial,
    PipeNetwork, Solution, SolutionScore,
};
use serde_json::Value;

use crate::error::SolverError;
use crate::solver::{Solver, SolverParams};

// ── Constants ─────────────────────────────────────────────────────────────────

const GRAVITY: f64 = 9.81;
const WATER_DENSITY: f64 = 998.0;
/// Minor-loss K for suction side (entrance + bends + valve).
const K_SUCTION: f64 = 4.5;
/// Minor-loss K for discharge side (check valve + gate valve + bends + exit).
const K_DISCHARGE: f64 = 7.0;

/// Standard pipe diameters for pump stations (m). Mirrors Python PUMP_DIAMETERS.
const PUMP_DIAMETERS: [f64; 6] = [0.100, 0.150, 0.200, 0.250, 0.300, 0.400];

/// Standard motor sizes (kW). Mirrors Python standard_motors array exactly.
const STANDARD_MOTORS: [f64; 24] = [
    0.37, 0.55, 0.75, 1.1, 1.5, 2.2, 3.0, 4.0, 5.5, 7.5, 11.0, 15.0, 18.5, 22.0, 30.0, 37.0, 45.0,
    55.0, 75.0, 90.0, 110.0, 132.0, 160.0, 200.0,
];

/// Efficiency table: [(flow_m3s, eta), ...]. Used for np.interp (clamped linear interpolation).
const EFF_TABLE: [(f64, f64); 4] = [(0.010, 0.60), (0.050, 0.70), (0.200, 0.78), (1.000, 0.82)];

// ── PumpStationSolver ─────────────────────────────────────────────────────────

/// Solver for pump station design.
///
/// Faithful port of Python `PumpStationSolver`. No terrain graph is used; the
/// `terrain` field satisfies the `BaseSolver` contract but is never read.
pub struct PumpStationSolver {
    /// Unused by the algorithm — kept for structural consistency with other solvers.
    #[allow(dead_code)]
    pub terrain: Option<TerrainModel>,
    pub constraints: DesignConstraints,
}

impl PumpStationSolver {
    /// Create a new `PumpStationSolver`. `terrain` is accepted but never used.
    pub fn new(terrain: Option<TerrainModel>, constraints: DesignConstraints) -> Self {
        PumpStationSolver {
            terrain,
            constraints,
        }
    }

    /// Design a pump station.
    ///
    /// Mirrors Python `PumpStationSolver.solve()`.
    #[allow(clippy::too_many_arguments)]
    pub fn solve_pump_station(
        &self,
        design_flow: f64,
        suction_elevation: f64,
        discharge_elevation: f64,
        suction_pipe_length: f64,
        discharge_pipe_length: f64,
        suction_diameter: f64,
        discharge_diameter: f64,
        material: PipeMaterial,
        num_alternatives: u32,
        num_pumps: Option<u32>,
        suction_d_idx: Option<i32>,
        discharge_d_idx: Option<i32>,
        wet_well_factor: f64,
        retention_minutes: f64,
    ) -> Result<Vec<Solution>, SolverError> {
        let hw_c = hw_coefficient(material.as_str());
        let static_lift = discharge_elevation - suction_elevation;

        // Apply genetic parameters for pipe diameters
        let eff_suction_d = if let Some(idx) = suction_d_idx {
            let i = (idx.max(0) as usize).min(PUMP_DIAMETERS.len() - 1);
            PUMP_DIAMETERS[i]
        } else {
            suction_diameter
        };
        let eff_discharge_d = if let Some(idx) = discharge_d_idx {
            let i = (idx.max(0) as usize).min(PUMP_DIAMETERS.len() - 1);
            PUMP_DIAMETERS[i]
        } else {
            discharge_diameter
        };

        // Build pump_counts: single design if num_pumps set, else range(1, num_alternatives+1)
        let pump_counts: Vec<u32> = if let Some(n) = num_pumps {
            vec![n]
        } else {
            (1..=(num_alternatives)).collect()
        };

        let mut solutions: Vec<Solution> = Vec::new();

        for (alt_idx_0based, &num_operating) in pump_counts.iter().enumerate() {
            let alt_idx = alt_idx_0based + 1; // 1-based like Python
            let flow_per_pump = design_flow / num_operating as f64;

            let tdh = self.calculate_tdh(
                static_lift,
                design_flow,
                eff_suction_d,
                suction_pipe_length,
                eff_discharge_d,
                discharge_pipe_length,
                hw_c,
            );

            let pump = self.select_pump(flow_per_pump, tdh, num_operating);
            let wet_well = self.design_wet_well(
                design_flow,
                pump.total_pumps,
                retention_minutes,
                wet_well_factor,
            );

            let network_name = format!("PumpStation_alt{alt_idx}");
            let mut network = self.build_layout(
                &pump,
                &wet_well,
                suction_elevation,
                discharge_elevation,
                eff_suction_d,
                eff_discharge_d,
                suction_pipe_length,
                discharge_pipe_length,
                material,
                design_flow,
                &network_name,
            );

            // Store metadata exactly as Python does (rounded)
            network
                .metadata
                .insert("tdh".to_string(), Value::from(round3(tdh)));
            network
                .metadata
                .insert("static_lift".to_string(), Value::from(round3(static_lift)));
            network
                .metadata
                .insert("hw_c".to_string(), Value::from(hw_c));
            network.metadata.insert(
                "pump".to_string(),
                serde_json::json!({
                    "flow_m3s": round4(pump.flow),
                    "tdh_m": round3(pump.tdh),
                    "power_kw": round2(pump.power_kw),
                    "efficiency": pump.efficiency,
                    "num_operating": pump.num_operating,
                    "num_reserve": pump.num_reserve,
                    "total_pumps": pump.total_pumps,
                }),
            );
            network.metadata.insert(
                "wet_well".to_string(),
                serde_json::json!({
                    "volume_m3": wet_well.volume_m3,
                    "width_m": wet_well.width_m,
                    "length_m": wet_well.length_m,
                    "depth_m": wet_well.depth_m,
                    "retention_minutes": wet_well.retention_minutes,
                    "num_pumps_installed": wet_well.num_pumps_installed,
                }),
            );

            let score = self.evaluate_pump_station(&network)?;
            solutions.push(Solution {
                rank: alt_idx as i64,
                network,
                score,
                metadata: Default::default(),
            });
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
        }

        Ok(solutions)
    }

    /// Evaluate a pump station network.
    ///
    /// Faithful port of Python `evaluate()`.
    /// Cost = 1200*kW*pumps + 800*well_vol + 150*L + 5000*violations.
    /// NOTE: total_excavation is REPURPOSED to carry well_volume_m3.
    pub fn evaluate_pump_station(
        &self,
        network: &PipeNetwork,
    ) -> Result<SolutionScore, SolverError> {
        let meta = &network.metadata;
        let pump_info = meta
            .get("pump")
            .and_then(|v| v.as_object())
            .cloned()
            .unwrap_or_default();
        let wet_well = meta
            .get("wet_well")
            .and_then(|v| v.as_object())
            .cloned()
            .unwrap_or_default();

        let power_kw = pump_info
            .get("power_kw")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        let total_pumps = pump_info
            .get("total_pumps")
            .and_then(|v| v.as_i64())
            .unwrap_or(1);
        let efficiency = pump_info
            .get("efficiency")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.75);
        let num_operating = pump_info
            .get("num_operating")
            .and_then(|v| v.as_i64())
            .unwrap_or(1);
        let flow_m3s = pump_info
            .get("flow_m3s")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.01);
        let well_volume = wet_well
            .get("volume_m3")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);

        let mut violations = 0i64;

        // Velocity check on all pipes using TOTAL flow (flow_m3s * num_operating).
        // Bounds are HARDCODED 0.3 / 3.0 — NOT constraints.min/max_velocity.
        // TRAP: flow = max(flow_m3s, 0.001) * num_operating (from Python oracle line 243)
        let total_flow = flow_m3s.max(0.001) * num_operating as f64;
        for pipe in &network.pipes {
            let v = velocity(total_flow, pipe.diameter);
            if v < 0.3 {
                violations += 1;
            }
            if v > 3.0 {
                violations += 1;
            }
        }

        // Efficiency check
        if efficiency < 0.60 {
            violations += 1;
        }

        // Cost model
        let equipment_cost = power_kw * total_pumps as f64 * 1200.0;
        let civil_cost = well_volume * 800.0;
        let pipe_cost = network.total_length * 150.0;
        let violation_penalty = violations as f64 * 5000.0;
        let total_cost = equipment_cost + civil_cost + pipe_cost + violation_penalty;

        Ok(SolutionScore {
            total_cost,
            total_length: network.total_length,
            total_excavation: well_volume, // REPURPOSED as wet_well_volume
            norm_violations: violations,
            pump_count: total_pumps,
            details: serde_json::json!({
                "equipment_cost": round0(equipment_cost),
                "civil_cost": round0(civil_cost),
                "pipe_cost": round0(pipe_cost),
                "power_kw_per_pump": round2(power_kw),
                "total_pumps": total_pumps,
                "wet_well_volume_m3": round2(well_volume),
            }),
        })
    }

    // ── Private helpers ───────────────────────────────────────────────────────

    /// Compute Total Dynamic Head.
    ///
    /// TDH = abs(static_lift) + hf_suction + hf_discharge + minor_suction + minor_discharge
    /// TRAP: abs(static_lift) — even if discharge < suction, lift is taken as positive.
    /// TRAP: hf uses TOTAL design_flow (not per-pump flow).
    #[allow(clippy::too_many_arguments)]
    fn calculate_tdh(
        &self,
        static_lift: f64,
        flow: f64,
        suction_d: f64,
        suction_l: f64,
        discharge_d: f64,
        discharge_l: f64,
        c: f64,
    ) -> f64 {
        let hf_suction = head_loss(flow, suction_d, suction_l, c);
        let hf_discharge = head_loss(flow, discharge_d, discharge_l, c);

        let v_suction = velocity(flow, suction_d);
        let v_discharge = velocity(flow, discharge_d);

        let minor_suction = K_SUCTION * v_suction * v_suction / (2.0 * GRAVITY);
        let minor_discharge = K_DISCHARGE * v_discharge * v_discharge / (2.0 * GRAVITY);

        static_lift.abs() + hf_suction + hf_discharge + minor_suction + minor_discharge
    }

    /// Select pump for given operating point.
    ///
    /// Mirrors Python `_select_pump()`:
    /// - Efficiency via `np.interp` (clamped linear interpolation on EFF_TABLE).
    /// - Shaft power = (Q * H * rho * g) / (eta * 1000).
    /// - Motor size: first standard motor >= power_kw; 200 kW if none found.
    /// - N+1 rule: num_reserve = 1 always.
    fn select_pump(&self, flow: f64, tdh: f64, num_operating: u32) -> PumpSelection {
        let eta = np_interp(flow, &EFF_TABLE);
        let power_kw = (flow * tdh * WATER_DENSITY * GRAVITY) / (eta * 1000.0);

        // Motor rounding: first standard motor >= power_kw (>= not >)
        let motor_kw = STANDARD_MOTORS
            .iter()
            .find(|&&m| m >= power_kw)
            .copied()
            .unwrap_or(200.0);

        PumpSelection {
            flow,
            tdh,
            power_kw: motor_kw,
            efficiency: eta,
            num_operating,
            num_reserve: 1,
            total_pumps: num_operating + 1,
        }
    }

    /// Size the wet well (carcamo de bombeo).
    ///
    /// Mirrors Python `_design_wet_well()`:
    /// - volume = flow * retention_minutes * 60 * volume_factor, clamp ≥ 2.0
    /// - width = cbrt(volume / 3.0), clamp ≥ 1.0
    /// - length = 2.0 * width
    /// - depth = volume / (width * length), clamp ≥ 1.5
    /// - actual_volume = width * length * depth (may differ from volume after depth clamp)
    /// - All values rounded in the returned struct.
    fn design_wet_well(
        &self,
        flow: f64,
        num_pumps: u32,
        retention_minutes: f64,
        volume_factor: f64,
    ) -> WetWell {
        let volume = (flow * retention_minutes * 60.0 * volume_factor).max(2.0);
        let width = (volume / 3.0_f64).cbrt().max(1.0);
        let length = 2.0 * width;
        let depth = (volume / (width * length)).max(1.5);
        let actual_volume = width * length * depth;

        let retention_actual = actual_volume / flow.max(1e-6) / 60.0;

        WetWell {
            volume_m3: round2(actual_volume),
            width_m: round2(width),
            length_m: round2(length),
            depth_m: round2(depth),
            retention_minutes: round1(retention_actual),
            num_pumps_installed: num_pumps,
        }
    }

    /// Build schematic 5-node / 4-pipe PipeNetwork.
    ///
    /// Mirrors Python `_build_layout()` exactly including:
    /// - Node positions computed from wet_well.length_m and pipe params.
    /// - start_invert = node.z - 0.5 (FIXED 0.5m cover, not min_cover).
    /// - Pipe (3): PumpManifold→DischargeManifold with length = suction_l (NOT discharge_l).
    ///   This matches the Python oracle exactly (appears swapped but is correct).
    /// - max(length, 0.1) guard on all pipe lengths.
    #[allow(clippy::too_many_arguments)]
    fn build_layout(
        &self,
        pump: &PumpSelection,
        wet_well: &WetWell,
        suction_elev: f64,
        discharge_elev: f64,
        suction_d: f64,
        discharge_d: f64,
        suction_l: f64,
        discharge_l: f64,
        material: PipeMaterial,
        design_flow: f64,
        name: &str,
    ) -> PipeNetwork {
        let well_l = wet_well.length_m;

        // Node specs: (id, x, y, z, node_type)
        let nodes_spec: &[(&str, f64, f64, f64, NodeType)] = &[
            ("Inlet", 0.0, 0.0, suction_elev, NodeType::Inlet),
            (
                "WetWell",
                well_l / 2.0,
                0.0,
                suction_elev,
                NodeType::Reservoir,
            ),
            (
                "PumpManifold",
                well_l + 2.0,
                0.0,
                suction_elev,
                NodeType::Pump,
            ),
            (
                "DischargeManifold",
                well_l + 2.0 + suction_l,
                0.0,
                (suction_elev + discharge_elev) / 2.0,
                NodeType::Junction,
            ),
            (
                "Outlet",
                well_l + 2.0 + suction_l + discharge_l,
                0.0,
                discharge_elev,
                NodeType::Outlet,
            ),
        ];

        // Build nodes with sump_elevation = z - 1.5 (NetworkNode default)
        let nodes: Vec<NetworkNode> = nodes_spec
            .iter()
            .map(|&(nid, x, y, z, ntype)| {
                let mut node = NetworkNode::new(nid, x, y, z, ntype);
                node.rim_elevation = z;
                // sump_elevation = z - 1.5 (matches Python NetworkNode default)
                node.sump_elevation = z - 1.5;
                node
            })
            .collect();

        // Build node lookup by id
        let node_map: HashMap<&str, usize> = nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (n.id.0.as_str(), i))
            .collect();

        // Pipe specs: (id, start_node_id, end_node_id, length, diameter)
        // TRAP: Pipe (3) length = suction_l (not discharge_l) — matches Python oracle exactly.
        let pipe_specs: &[(&str, &str, &str, f64, f64)] = &[
            ("Pipe - (1)", "Inlet", "WetWell", well_l / 2.0, suction_d),
            ("Pipe - (2)", "WetWell", "PumpManifold", 2.0, suction_d),
            (
                "Pipe - (3)",
                "PumpManifold",
                "DischargeManifold",
                suction_l,
                discharge_d,
            ),
            (
                "Pipe - (4)",
                "DischargeManifold",
                "Outlet",
                discharge_l,
                discharge_d,
            ),
        ];

        let pipes: Vec<NetworkPipe> = pipe_specs
            .iter()
            .filter_map(|&(pid, sn, en, length, diam)| {
                let si = *node_map.get(sn)?;
                let ei = *node_map.get(en)?;
                let s = &nodes[si];
                let e = &nodes[ei];
                let eff_length = length.max(0.1);
                let start_invert = s.z - 0.5;
                let end_invert = e.z - 0.5;
                let slope = if eff_length > 0.0 {
                    (start_invert - end_invert) / eff_length
                } else {
                    0.0
                };
                Some(NetworkPipe {
                    id: PipeId::new(pid),
                    start_node_id: NodeId::new(sn),
                    end_node_id: NodeId::new(en),
                    length: eff_length,
                    effective_length: eff_length,
                    diameter: diam,
                    material,
                    roughness: material.manning_n(),
                    start_invert,
                    end_invert,
                    slope,
                    design_flow,
                    waypoints: vec![],
                    metadata: HashMap::new(),
                })
            })
            .collect();

        // Store pump metadata in nodes (not needed for evaluate, but kept for completeness)
        // The pump field is written into network.metadata by the caller.
        let _ = pump; // pump metadata is set by caller after this function returns

        PipeNetwork::new(name, nodes, pipes)
    }
}

// ── Solver trait impl ─────────────────────────────────────────────────────────

impl Solver for PumpStationSolver {
    fn solve(&mut self, _params: &SolverParams) -> Result<Vec<Solution>, SolverError> {
        Err(SolverError::BuildFailed(
            "Use PumpStationSolver::solve_pump_station() with explicit flow/elevation params"
                .to_string(),
        ))
    }

    fn evaluate(&self, network: &PipeNetwork) -> Result<SolutionScore, SolverError> {
        self.evaluate_pump_station(network)
    }
}

// ── Internal types ────────────────────────────────────────────────────────────

/// Pump sizing result (mirrors Python `PumpSelection` dataclass).
struct PumpSelection {
    flow: f64,
    tdh: f64,
    power_kw: f64,
    efficiency: f64,
    num_operating: u32,
    num_reserve: u32,
    total_pumps: u32,
}

/// Wet well sizing result.
struct WetWell {
    volume_m3: f64,
    width_m: f64,
    length_m: f64,
    depth_m: f64,
    retention_minutes: f64,
    num_pumps_installed: u32,
}

// ── Private helpers ───────────────────────────────────────────────────────────

/// Linear interpolation mirroring Python `np.interp(x, xp, fp)` with clamping.
///
/// - If x < xp[0] → fp[0] (clamp left)
/// - If x > xp[-1] → fp[-1] (clamp right)
/// - Otherwise: linear interpolation between the two surrounding points.
fn np_interp(x: f64, table: &[(f64, f64)]) -> f64 {
    if table.is_empty() {
        return 0.0;
    }
    if x <= table[0].0 {
        return table[0].1;
    }
    if x >= table[table.len() - 1].0 {
        return table[table.len() - 1].1;
    }
    // Find the segment [xp[i], xp[i+1]] that contains x
    for i in 0..table.len() - 1 {
        let (x0, y0) = table[i];
        let (x1, y1) = table[i + 1];
        if x <= x1 {
            // Linear interpolation
            let t = (x - x0) / (x1 - x0);
            return y0 + t * (y1 - y0);
        }
    }
    table[table.len() - 1].1
}

/// Round to 0 decimal places (mirrors Python `round(x, 0)`).
#[inline]
fn round0(x: f64) -> f64 {
    x.round()
}

/// Round to 1 decimal place (mirrors Python `round(x, 1)`).
#[inline]
fn round1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

/// Round to 2 decimal places (mirrors Python `round(x, 2)`).
#[inline]
fn round2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

/// Round to 3 decimal places (mirrors Python `round(x, 3)`).
#[inline]
fn round3(x: f64) -> f64 {
    (x * 1000.0).round() / 1000.0
}

/// Round to 4 decimal places (mirrors Python `round(x, 4)`).
#[inline]
fn round4(x: f64) -> f64 {
    (x * 10000.0).round() / 10000.0
}
