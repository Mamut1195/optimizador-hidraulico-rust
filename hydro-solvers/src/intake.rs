//! IntakeSolver — water intake structure design (captacion).
//!
//! Faithful port of `hydro_engine/solvers/intake.py` (IntakeSolver class).
//!
//! ## Algorithm
//! 1. Design bar screen: net_area = Q/v, Kirschmer HL.
//! 2. Design settling channel: TWO-PASS depth search over 200 linspace depths,
//!    then call rectangular_channel_flow_full() for authoritative rounded result.
//! 3. Design weir: rectangular Q=C*L*H^1.5 or v-notch Q=1.38*tan(45)*H^2.5.
//! 4. Design pipe transition: HW required_diameter_pressure + velocity upgrade.
//! 5. Build schematic 6-node / 5-pipe PipeNetwork layout.
//! 6. Evaluate: cost = 600*(concrete_vol+screen_concrete) + 120*pipe_L + 3000*violations.
//!    NOTE: SolutionScore.total_excavation is REPURPOSED to carry
//!    concrete_volume + screen_concrete (same convention as wu2 well_volume).

use std::collections::HashMap;

use hydro_hydraulics::{
    hazen_williams::{required_diameter_pressure, velocity},
    hw_coefficient, rectangular_channel_flow_full,
};
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
/// Francis formula coefficient for rectangular weir (m^0.5/s).
const C_RECT_WEIR: f64 = 1.84;
/// V-notch coefficient for 90-degree notch (m^0.5/s).
const C_VNOTCH_90: f64 = 1.38;
/// Kirschmer beta coefficient for rectangular bars.
const KIRSCHMER_BETA: f64 = 2.42;
/// Bar screen incline angle (60 degrees) in radians.
const ALPHA_RAD: f64 = std::f64::consts::PI / 3.0; // 60 degrees
/// Number of linspace steps for channel depth search.
const DEPTH_LINSPACE_N: usize = 200;
/// Min depth candidate (m).
const DEPTH_MIN: f64 = 0.05;
/// Max depth candidate (m).
const DEPTH_MAX: f64 = 3.0;

// ── IntakeSolver ──────────────────────────────────────────────────────────────

/// Solver for water intake (captacion) structure design.
///
/// Faithful port of Python `IntakeSolver`. No terrain graph is used; the
/// `terrain` field satisfies the `BaseSolver` contract but is never read.
pub struct IntakeSolver {
    /// Unused by the algorithm — kept for structural consistency with other solvers.
    #[allow(dead_code)]
    pub terrain: Option<TerrainModel>,
    pub constraints: DesignConstraints,
}

impl IntakeSolver {
    /// Create a new `IntakeSolver`. `terrain` is accepted but never used.
    pub fn new(terrain: Option<TerrainModel>, constraints: DesignConstraints) -> Self {
        IntakeSolver {
            terrain,
            constraints,
        }
    }

    /// Design an intake structure.
    ///
    /// Mirrors Python `IntakeSolver.solve()`.
    #[allow(clippy::too_many_arguments)]
    pub fn solve_intake(
        &self,
        design_flow: f64,
        source_type: &str,
        source_elevation: f64,
        pipe_elevation: f64,
        channel_slope: f64,
        material: PipeMaterial,
        num_alternatives: u32,
        channel_width_factor: f64,
        channel_slope_factor: f64,
        weir_type: i32,
        screen_velocity_factor: f64,
        screen_velocity: f64,
        bar_spacing: f64,
        bar_thickness: f64,
        channel_roughness: f64,
    ) -> Result<Vec<Solution>, SolverError> {
        let effective_screen_velocity = screen_velocity * screen_velocity_factor;

        let weir_type_str = if weir_type >= 1 {
            "v_notch"
        } else {
            "rectangular"
        };

        let eff_channel_slope = channel_slope * channel_slope_factor;

        // Build width multipliers exactly as Python:
        // num_alternatives == 1 → [channel_width_factor]
        // else → np.linspace(channel_width_factor, channel_width_factor + 0.5, num_alternatives)
        let width_multipliers: Vec<f64> = if num_alternatives == 1 {
            vec![channel_width_factor]
        } else {
            let n = num_alternatives as usize;
            let start = channel_width_factor;
            let end = channel_width_factor + 0.5;
            (0..n)
                .map(|i| start + (end - start) * (i as f64) / ((n - 1) as f64))
                .collect()
        };

        let mut solutions: Vec<Solution> = Vec::new();

        for (alt_idx_0based, &w_mult) in width_multipliers.iter().enumerate() {
            let alt_idx = alt_idx_0based + 1;

            let screen = self.design_screen(
                design_flow,
                effective_screen_velocity,
                bar_spacing,
                bar_thickness,
            );

            let base_width = screen.net_width.max(0.3);
            let ch_width = base_width * w_mult;

            let channel =
                self.design_channel(design_flow, ch_width, eff_channel_slope, channel_roughness);

            let weir = self.design_weir(design_flow, weir_type_str);

            let hw_c = hw_coefficient(material.as_str());
            let pipe_trans = self.design_pipe_transition(design_flow, material, hw_c);

            let intake = IntakeDesign {
                screen_width: screen.gross_width,
                screen_height: screen.height,
                screen_area: screen.net_area,
                channel_width: channel.width_m,
                channel_depth: channel.depth_m,
                channel_length: channel.length_m,
                channel_slope: eff_channel_slope,
                weir_length: weir.crest_length,
                weir_head: weir.head,
                pipe_diameter: pipe_trans.diameter,
                pipe_invert: pipe_elevation,
            };

            let network_name = format!("Intake_alt{alt_idx}");
            let mut network = self.build_layout(
                &intake,
                source_elevation,
                pipe_elevation,
                material,
                design_flow,
                &network_name,
            );

            // Store metadata exactly as Python does
            network
                .metadata
                .insert("design_flow_m3s".to_string(), Value::from(design_flow));
            network
                .metadata
                .insert("source_type".to_string(), Value::from(source_type));
            network.metadata.insert(
                "screen".to_string(),
                serde_json::json!({
                    "net_area": screen.net_area,
                    "gross_area": screen.gross_area,
                    "clear_ratio": screen.clear_ratio,
                    "gross_width": screen.gross_width,
                    "net_width": screen.net_width,
                    "height": screen.height,
                    "bar_spacing_m": screen.bar_spacing_m,
                    "bar_thickness_m": screen.bar_thickness_m,
                    "velocity_through_screen": screen.velocity_through_screen,
                    "head_loss_m": screen.head_loss_m,
                }),
            );
            network.metadata.insert(
                "channel".to_string(),
                serde_json::json!({
                    "width_m": channel.width_m,
                    "depth_m": channel.depth_m,
                    "area_m2": channel.area_m2,
                    "wetted_perimeter_m": channel.wetted_perimeter_m,
                    "hydraulic_radius_m": channel.hydraulic_radius_m,
                    "velocity_m_s": channel.velocity_m_s,
                    "flow_m3_s": channel.flow_m3_s,
                    "flow_lps": channel.flow_lps,
                    "froude_number": channel.froude_number,
                    "regime": channel.regime,
                    "length_m": channel.length_m,
                    "slope": channel.slope,
                }),
            );
            network.metadata.insert(
                "weir".to_string(),
                match weir_type_str {
                    "v_notch" => serde_json::json!({
                        "weir_type": "v_notch",
                        "angle_degrees": 90.0_f64,
                        "head": weir.head,
                        "crest_length": weir.crest_length,
                        "flow_check_m3s": weir.flow_check_m3s,
                        "coefficient": C_VNOTCH_90,
                    }),
                    _ => serde_json::json!({
                        "weir_type": "rectangular",
                        "crest_length": weir.crest_length,
                        "head": weir.head,
                        "flow_check_m3s": weir.flow_check_m3s,
                        "coefficient": C_RECT_WEIR,
                    }),
                },
            );
            network.metadata.insert(
                "pipe_transition".to_string(),
                serde_json::json!({
                    "diameter": pipe_trans.diameter,
                    "diameter_mm": pipe_trans.diameter * 1000.0,
                    "velocity_m_s": pipe_trans.velocity_m_s,
                    "material": pipe_trans.material_name,
                    "hw_c": pipe_trans.hw_c,
                    "transition_type": "gradual_contraction",
                    "contraction_angle_deg": 15.0_f64,
                }),
            );

            let score = self.evaluate_intake(&network)?;
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

    /// Evaluate an intake structure network.
    ///
    /// Faithful port of Python `evaluate()`.
    /// Cost = 600*(concrete_vol+screen_concrete) + 120*pipe_L + 3000*violations.
    /// NOTE: total_excavation is REPURPOSED to carry concrete_vol + screen_concrete.
    pub fn evaluate_intake(&self, network: &PipeNetwork) -> Result<SolutionScore, SolverError> {
        let meta = &network.metadata;
        let channel = meta
            .get("channel")
            .and_then(|v| v.as_object())
            .cloned()
            .unwrap_or_default();
        let screen = meta
            .get("screen")
            .and_then(|v| v.as_object())
            .cloned()
            .unwrap_or_default();
        let weir = meta
            .get("weir")
            .and_then(|v| v.as_object())
            .cloned()
            .unwrap_or_default();
        let pipe_trans = meta
            .get("pipe_transition")
            .and_then(|v| v.as_object())
            .cloned()
            .unwrap_or_default();

        let mut violations = 0i64;

        // Channel velocity check (reads rounded velocity_m_s from metadata)
        let ch_velocity = channel
            .get("velocity_m_s")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        if ch_velocity < 0.3 {
            violations += 1;
        }
        if ch_velocity > 2.0 {
            violations += 1;
        }

        // Screen velocity check
        let v_screen = screen
            .get("velocity_through_screen")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        if v_screen > 0.30 {
            violations += 1;
        }

        // Weir head check
        let weir_head = weir.get("head").and_then(|v| v.as_f64()).unwrap_or(0.0);
        if weir_head > 0.5 {
            violations += 1;
        }

        // Pipe velocity check
        let v_pipe = pipe_trans
            .get("velocity_m_s")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        if v_pipe < 0.6 {
            violations += 1;
        }
        if v_pipe > 5.0 {
            violations += 1;
        }

        // Concrete volume
        let ch_width = channel
            .get("width_m")
            .and_then(|v| v.as_f64())
            .unwrap_or(1.0);
        let ch_depth = channel
            .get("depth_m")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.5);
        let ch_length = channel
            .get("length_m")
            .and_then(|v| v.as_f64())
            .unwrap_or(3.0);
        let wall_thickness = 0.20_f64; // HARDCODED — never from constraints

        // Floor + two side walls + two end walls
        let concrete_volume = ch_width * ch_length * wall_thickness
            + 2.0 * ch_length * ch_depth * wall_thickness
            + 2.0 * ch_width * ch_depth * wall_thickness;

        // Screen structure concrete box
        let scr_w = screen
            .get("gross_width")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.5);
        let scr_h = screen.get("height").and_then(|v| v.as_f64()).unwrap_or(0.5);
        let screen_concrete = 4.0 * scr_w * scr_h * wall_thickness;

        let pipe_length = network.total_length;
        let pipe_cost = pipe_length * 120.0;
        let concrete_cost = (concrete_volume + screen_concrete) * 600.0;
        let violation_penalty = violations as f64 * 3000.0;
        let total_cost = concrete_cost + pipe_cost + violation_penalty;

        Ok(SolutionScore {
            total_cost,
            total_length: pipe_length,
            total_excavation: concrete_volume + screen_concrete, // REPURPOSED
            norm_violations: violations,
            pump_count: 0,
            details: serde_json::json!({
                "concrete_volume_m3": round2(concrete_volume),
                "screen_concrete_m3": round2(screen_concrete),
                "pipe_cost": round0(pipe_cost),
                "concrete_cost": round0(concrete_cost),
                "channel_velocity": ch_velocity,
            }),
        })
    }

    // ── Private helpers ───────────────────────────────────────────────────────

    /// Design the bar screen (rejilla).
    ///
    /// Mirrors Python `_design_screen()` including all rounding.
    fn design_screen(
        &self,
        flow: f64,
        velocity_through: f64,
        bar_spacing: f64,
        bar_thickness: f64,
    ) -> ScreenResult {
        let net_area = flow / velocity_through;
        let clear_ratio = bar_spacing / (bar_spacing + bar_thickness);
        let gross_area = net_area / clear_ratio;

        // Aspect ratio: width = 2 * height → gross_area = height * (2*height) = 2*height^2
        let height = (gross_area / 2.0_f64).sqrt().max(0.3);
        let width = gross_area / height;

        // Kirschmer HL: beta=2.42, alpha=60deg
        let hl_screen = KIRSCHMER_BETA
            * (bar_thickness / bar_spacing).powf(4.0 / 3.0)
            * (velocity_through * velocity_through / (2.0 * GRAVITY))
            * ALPHA_RAD.sin();

        ScreenResult {
            net_area: round4(net_area),
            gross_area: round4(gross_area),
            clear_ratio: round3(clear_ratio),
            gross_width: round3(width),
            net_width: round3(width * clear_ratio),
            height: round3(height),
            bar_spacing_m: bar_spacing,
            bar_thickness_m: bar_thickness,
            velocity_through_screen: round3(velocity_through),
            head_loss_m: round4(hl_screen),
        }
    }

    /// Design the settling channel using Manning for rectangular section.
    ///
    /// TWO-PASS depth search (mirrors Python `_design_channel()`):
    ///   Pass 1: vectorized scan over 200 linspace depths → find first depth
    ///           where capacity >= flow.
    ///   Pass 2: call `rectangular_channel_flow_full(width, depth, slope, roughness)`
    ///           for authoritative rounded result.
    ///
    /// TRAP: velocity_m_s in the returned dict is ROUNDED to 3dp by
    /// rectangular_channel_flow_full. evaluate() reads that rounded value.
    fn design_channel(&self, flow: f64, width: f64, slope: f64, roughness: f64) -> ChannelResult {
        // Pass 1: vectorized scan — mirrors np.linspace(0.05, 3.0, 200)
        let mut chosen_depth = DEPTH_MAX;
        for i in 0..DEPTH_LINSPACE_N {
            let d =
                DEPTH_MIN + (DEPTH_MAX - DEPTH_MIN) * (i as f64) / ((DEPTH_LINSPACE_N - 1) as f64);
            let area = width * d;
            let perim = width + 2.0 * d;
            let r = if perim > 0.0 { area / perim } else { 0.0 };
            let v = (1.0 / roughness) * r.powf(2.0 / 3.0) * slope.abs().sqrt();
            let capacity = v * area;
            if capacity >= flow {
                chosen_depth = d;
                break;
            }
        }

        // Pass 2: authoritative result with rounded fields
        let result = rectangular_channel_flow_full(width, chosen_depth, slope, roughness);

        // Length rule: L = max(25 * width, 3.0)
        let length = (25.0 * width).max(3.0);

        ChannelResult {
            width_m: width,
            depth_m: chosen_depth,
            area_m2: result.area_m2,
            wetted_perimeter_m: result.wetted_perimeter_m,
            hydraulic_radius_m: result.hydraulic_radius_m,
            velocity_m_s: result.velocity_m_s, // already rounded to 3dp
            flow_m3_s: result.flow_m3_s,
            flow_lps: result.flow_lps,
            froude_number: result.froude_number,
            regime: result.regime,
            length_m: round2(length),
            slope,
        }
    }

    /// Design the inlet weir.
    ///
    /// Mirrors Python `_design_weir()` for both rectangular and v_notch.
    fn design_weir(&self, flow: f64, weir_type: &str) -> WeirResult {
        if weir_type == "v_notch" {
            // Q = 1.38 * tan(45°) * H^(5/2)
            // tan(45°) = 1.0
            let tan_half = 1.0_f64; // tan(45°)
            let head_raw = (flow / (C_VNOTCH_90 * tan_half)).powf(2.0 / 5.0);
            let head = head_raw.max(0.02);
            let q_check = C_VNOTCH_90 * tan_half * head.powf(5.0 / 2.0);
            WeirResult {
                crest_length: 0.0, // not applicable for v_notch
                head: round4(head),
                flow_check_m3s: round6(q_check),
            }
        } else {
            // Rectangular: Q = C * L * H^(3/2)
            // Start with L = max(0.5, 3 * sqrt(Q))
            let crest_length_init = (3.0 * flow.sqrt()).max(0.5);
            let head_raw = (flow / (C_RECT_WEIR * crest_length_init)).powf(2.0 / 3.0);
            let head_init = head_raw.max(0.02);

            let (crest_length, head) = if head_init > 0.40 {
                let cl = flow / (C_RECT_WEIR * 0.40_f64.powf(1.5));
                (cl, 0.40_f64)
            } else {
                (crest_length_init, head_init)
            };

            let q_check = C_RECT_WEIR * crest_length * head.powf(3.0 / 2.0);
            WeirResult {
                crest_length: round3(crest_length),
                head: round4(head),
                flow_check_m3s: round6(q_check),
            }
        }
    }

    /// Design the transition from channel to conveyance pipe.
    ///
    /// Mirrors Python `_design_pipe_transition()`.
    fn design_pipe_transition(
        &self,
        flow: f64,
        material: PipeMaterial,
        hw_c: f64,
    ) -> PipeTransResult {
        let mut diameter = required_diameter_pressure(
            flow,
            100.0, // length
            10.0,  // available_head
            hw_c,
            &self.constraints.available_diameters,
        );

        let mut v = velocity(flow, diameter);

        // Upgrade diameter if velocity too high
        if v > self.constraints.max_velocity {
            let mut sorted_diameters = self.constraints.available_diameters.clone();
            sorted_diameters.sort_by(|a, b| a.partial_cmp(b).unwrap());
            for &d in &sorted_diameters {
                if d > diameter && velocity(flow, d) <= self.constraints.max_velocity {
                    diameter = d;
                    v = velocity(flow, diameter);
                    break;
                }
            }
        }

        // Get the material name as stored in Python (Spanish names from PipeMaterial.value)
        let material_name = match material {
            PipeMaterial::Pvc => "PVC",
            PipeMaterial::Hdpe => "PEAD",
            PipeMaterial::Steel => "Acero",
            PipeMaterial::CastIron => "Fierro fundido",
            PipeMaterial::Concrete => "Concreto",
        };

        PipeTransResult {
            diameter,
            velocity_m_s: round3(v),
            material_name,
            hw_c,
        }
    }

    /// Generate a schematic PipeNetwork for the intake.
    ///
    /// Mirrors Python `_build_layout()` with 6 nodes, 5 pipe/segment connections.
    /// start_invert = node.z - 0.2 (FIXED 0.2m cover, not min_cover).
    #[allow(clippy::too_many_arguments)]
    fn build_layout(
        &self,
        design: &IntakeDesign,
        source_elevation: f64,
        pipe_elevation: f64,
        material: PipeMaterial,
        design_flow: f64,
        name: &str,
    ) -> PipeNetwork {
        // Elevations
        let screen_elev = source_elevation - design.screen_height * 0.5;
        let ch_in_elev = screen_elev - 0.1;
        let ch_out_elev = ch_in_elev - design.channel_slope * design.channel_length;
        let weir_elev = ch_out_elev;
        let pipe_start_elev = pipe_elevation;

        // X positions via cumsum([0.0, 1.0, channel_length, 0.5, 2.0])
        // Python: x_positions = np.cumsum([0.0, 1.0, channel_length, 0.5, 2.0])
        // So x_positions = [0.0, 1.0, 1.0+channel_length, 1.5+channel_length, 3.5+channel_length]
        let x0 = 0.0_f64;
        let x1 = 1.0_f64;
        let x2 = x1 + design.channel_length;
        let x3 = x2 + 0.5;
        let x4 = x3 + 2.0;

        // ChannelIn.x = x2 * 0.3 + x1 * 0.7
        let x_channel_in = x2 * 0.3 + x1 * 0.7;

        // Node specs: (id, x, z, node_type)
        let nodes_spec: &[(&str, f64, f64, NodeType)] = &[
            ("Source", x0, source_elevation, NodeType::Reservoir),
            ("Screen", x1, screen_elev, NodeType::Inlet),
            ("ChannelIn", x_channel_in, ch_in_elev, NodeType::Junction),
            ("ChannelOut", x2, ch_out_elev, NodeType::Junction),
            ("Weir", x3, weir_elev, NodeType::Junction),
            ("PipeStart", x4, pipe_start_elev, NodeType::Outlet),
        ];

        let nodes: Vec<NetworkNode> = nodes_spec
            .iter()
            .map(|&(nid, x, z, ntype)| {
                let mut node = NetworkNode::new(nid, x, 0.0, z, ntype);
                node.rim_elevation = z + 0.3;
                // sump_elevation = z - 1.5 (NetworkNode default)
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

        // Pipe specs: (id, start_node_id, end_node_id, dimension)
        // For non-Seg-Pipe: diam = max(dim, 0.2)
        // For Seg-Pipe: diam = pipe_diameter
        let pipe_specs: &[(&str, &str, &str, f64, bool)] = &[
            ("Seg-Screen", "Source", "Screen", design.screen_width, false),
            (
                "Seg-ChIn",
                "Screen",
                "ChannelIn",
                design.channel_width,
                false,
            ),
            (
                "Seg-Channel",
                "ChannelIn",
                "ChannelOut",
                design.channel_width,
                false,
            ),
            ("Seg-Weir", "ChannelOut", "Weir", design.weir_length, false),
            ("Seg-Pipe", "Weir", "PipeStart", design.pipe_diameter, true),
        ];

        let pipes: Vec<NetworkPipe> = pipe_specs
            .iter()
            .filter_map(|&(pid, sn_id, en_id, dim, is_pipe)| {
                let si = *node_map.get(sn_id)?;
                let ei = *node_map.get(en_id)?;
                let sn = &nodes[si];
                let en = &nodes[ei];

                // distance_to: Euclidean distance in XY (y=0 for both)
                let dist = ((sn.x - en.x).powi(2) + (sn.y - en.y).powi(2)).sqrt();
                let eff_length = dist.max(0.1);

                let diam = if is_pipe { dim } else { dim.max(0.2) };
                let start_invert = sn.z - 0.2;
                let end_invert = en.z - 0.2;
                let slope = if eff_length > 0.0 {
                    (start_invert - end_invert) / eff_length
                } else {
                    0.0
                };

                Some(NetworkPipe {
                    id: PipeId::new(pid),
                    start_node_id: NodeId::new(sn_id),
                    end_node_id: NodeId::new(en_id),
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

        PipeNetwork::new(name, nodes, pipes)
    }
}

// ── Solver trait impl ─────────────────────────────────────────────────────────

impl Solver for IntakeSolver {
    fn solve(&mut self, _params: &SolverParams) -> Result<Vec<Solution>, SolverError> {
        Err(SolverError::BuildFailed(
            "Use IntakeSolver::solve_intake() with explicit flow/elevation params".to_string(),
        ))
    }

    fn evaluate(&self, network: &PipeNetwork) -> Result<SolutionScore, SolverError> {
        self.evaluate_intake(network)
    }
}

// ── Internal types ────────────────────────────────────────────────────────────

/// Screen sizing result (mirrors Python dict from `_design_screen()`).
struct ScreenResult {
    net_area: f64,
    gross_area: f64,
    clear_ratio: f64,
    gross_width: f64,
    net_width: f64,
    height: f64,
    bar_spacing_m: f64,
    bar_thickness_m: f64,
    velocity_through_screen: f64,
    head_loss_m: f64,
}

/// Channel sizing result (mirrors Python dict from `_design_channel()`).
struct ChannelResult {
    width_m: f64,
    depth_m: f64,
    area_m2: f64,
    wetted_perimeter_m: f64,
    hydraulic_radius_m: f64,
    velocity_m_s: f64,
    flow_m3_s: f64,
    flow_lps: f64,
    froude_number: f64,
    regime: &'static str,
    length_m: f64,
    slope: f64,
}

/// Weir design result (mirrors Python dict from `_design_weir()`).
struct WeirResult {
    crest_length: f64,
    head: f64,
    flow_check_m3s: f64,
}

/// Pipe transition result (mirrors Python dict from `_design_pipe_transition()`).
struct PipeTransResult {
    diameter: f64,
    velocity_m_s: f64,
    material_name: &'static str,
    hw_c: f64,
}

/// Intake design parameters (mirrors Python `IntakeDesign` dataclass).
#[allow(dead_code)]
struct IntakeDesign {
    screen_width: f64,
    screen_height: f64,
    screen_area: f64,
    channel_width: f64,
    channel_depth: f64,
    channel_length: f64,
    channel_slope: f64,
    weir_length: f64,
    weir_head: f64,
    pipe_diameter: f64,
    pipe_invert: f64,
}

// ── Private helpers ───────────────────────────────────────────────────────────

/// Round to 0 decimal places (mirrors Python `round(x, 0)`).
#[inline]
fn round0(x: f64) -> f64 {
    x.round()
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

/// Round to 6 decimal places (mirrors Python `round(x, 6)`).
#[inline]
fn round6(x: f64) -> f64 {
    (x * 1_000_000.0).round() / 1_000_000.0
}
