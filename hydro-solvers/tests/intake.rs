//! T-5.4c — IntakeSolver oracle-parity integration tests.
//!
//! Parity gate:
//!   - total_cost within 1e-4 of oracle (E2E solve)
//!   - norm_violations EXACT
//!   - pump_count EXACT (always 0)
//!   - total_length within 1e-6
//!   - total_excavation (= concrete_vol + screen_concrete) within 1e-6
//!   - screen dimensions within 1e-6
//!   - channel depth within 1e-4 (linspace rounding)
//!   - weir head/length within 1e-6
//!   - pipe diameter EXACT
//!   - alternatives: pairwise-distinct total_costs, sorted ascending

use approx::assert_abs_diff_eq;
use hydro_solvers::{IntakeSolver, Solver, SolverParams};
use hydro_types::{
    DesignConstraints, NetworkNode, NetworkPipe, NodeId, NodeType, PipeId, PipeMaterial,
    PipeNetwork,
};
use oracle::solvers::{
    load_intake_alternatives, load_intake_golden, IntakeGolden, IntakeNetworkMetadata,
};
use serde_json::Value;
use std::collections::HashMap;

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Parse node type string from oracle fixture.
fn parse_node_type(s: &str) -> NodeType {
    match s {
        "INLET" => NodeType::Inlet,
        "OUTLET" => NodeType::Outlet,
        "RESERVOIR" => NodeType::Reservoir,
        "PUMP" => NodeType::Pump,
        "JUNCTION" => NodeType::Junction,
        "TANK" => NodeType::Tank,
        "MANHOLE" => NodeType::Manhole,
        "SERVICE" => NodeType::Service,
        "VALVE" => NodeType::Valve,
        "HYDRANT" => NodeType::Hydrant,
        _ => panic!("Unknown node type in intake fixture: {s}"),
    }
}

/// Reconstruct the oracle-built PipeNetwork from golden fixture data.
///
/// Used for the evaluate-only test.
fn reconstruct_intake_network(f: &IntakeGolden) -> PipeNetwork {
    let nodes: Vec<NetworkNode> = f
        .nodes
        .iter()
        .map(|n| {
            let node_type = parse_node_type(&n.node_type);
            let mut node = NetworkNode::new(n.id.as_str(), n.x, n.y, n.z, node_type);
            node.rim_elevation = n.rim_elevation;
            node.sump_elevation = n.sump_elevation;
            node
        })
        .collect();

    let pipes: Vec<NetworkPipe> = f
        .pipes
        .iter()
        .map(|p| {
            let length = p.length;
            NetworkPipe {
                id: PipeId::new(p.id.as_str()),
                start_node_id: NodeId::new(p.start_node_id.as_str()),
                end_node_id: NodeId::new(p.end_node_id.as_str()),
                length,
                effective_length: length,
                diameter: p.diameter,
                material: PipeMaterial::Concrete,
                roughness: PipeMaterial::Concrete.manning_n(),
                start_invert: p.start_invert,
                end_invert: p.end_invert,
                slope: p.slope,
                design_flow: p.design_flow,
                waypoints: vec![],
                metadata: HashMap::new(),
            }
        })
        .collect();

    let mut network = PipeNetwork::new("IntakeTest", nodes, pipes);

    // Inject network metadata from fixture so evaluate() can read screen/channel/weir/pipe_transition
    inject_intake_metadata(&mut network, &f.network_metadata);

    network
}

/// Inject intake network metadata from fixture into a PipeNetwork.
fn inject_intake_metadata(network: &mut PipeNetwork, meta: &IntakeNetworkMetadata) {
    network.metadata.insert(
        "design_flow_m3s".to_string(),
        Value::from(meta.design_flow_m3s),
    );
    network.metadata.insert(
        "source_type".to_string(),
        Value::from(meta.source_type.as_str()),
    );

    let sc = &meta.screen;
    network.metadata.insert(
        "screen".to_string(),
        serde_json::json!({
            "net_area": sc.net_area,
            "gross_area": sc.gross_area,
            "clear_ratio": sc.clear_ratio,
            "gross_width": sc.gross_width,
            "net_width": sc.net_width,
            "height": sc.height,
            "bar_spacing_m": sc.bar_spacing_m,
            "bar_thickness_m": sc.bar_thickness_m,
            "velocity_through_screen": sc.velocity_through_screen,
            "head_loss_m": sc.head_loss_m,
        }),
    );

    let ch = &meta.channel;
    network.metadata.insert(
        "channel".to_string(),
        serde_json::json!({
            "width_m": ch.width_m,
            "depth_m": ch.depth_m,
            "area_m2": ch.area_m2,
            "wetted_perimeter_m": ch.wetted_perimeter_m,
            "hydraulic_radius_m": ch.hydraulic_radius_m,
            "velocity_m_s": ch.velocity_m_s,
            "flow_m3_s": ch.flow_m3_s,
            "flow_lps": ch.flow_lps,
            "froude_number": ch.froude_number,
            "regime": ch.regime.as_str(),
            "length_m": ch.length_m,
            "slope": ch.slope,
        }),
    );

    let w = &meta.weir;
    if w.weir_type == "v_notch" {
        network.metadata.insert(
            "weir".to_string(),
            serde_json::json!({
                "weir_type": "v_notch",
                "angle_degrees": w.angle_degrees,
                "head": w.head,
                "crest_length": w.crest_length,
                "flow_check_m3s": w.flow_check_m3s,
                "coefficient": w.coefficient,
            }),
        );
    } else {
        network.metadata.insert(
            "weir".to_string(),
            serde_json::json!({
                "weir_type": "rectangular",
                "crest_length": w.crest_length,
                "head": w.head,
                "flow_check_m3s": w.flow_check_m3s,
                "coefficient": w.coefficient,
            }),
        );
    }

    let pt = &meta.pipe_transition;
    network.metadata.insert(
        "pipe_transition".to_string(),
        serde_json::json!({
            "diameter": pt.diameter,
            "diameter_mm": pt.diameter_mm,
            "velocity_m_s": pt.velocity_m_s,
            "material": pt.material.as_str(),
            "hw_c": pt.hw_c,
            "transition_type": pt.transition_type.as_str(),
            "contraction_angle_deg": pt.contraction_angle_deg,
        }),
    );
}

// ── T-5.4c.a: fixture loads and has correct shape ─────────────────────────────

/// T-5.4c.a: Fixture loads without error and structural counts match.
#[test]
fn intake_golden_loads_and_has_correct_shape() {
    let f = load_intake_golden();
    assert_eq!(f.schema_version, 1);
    assert_eq!(f.node_count, 6, "intake always has 6 nodes");
    assert_eq!(f.pipe_count, 5, "intake always has 5 pipes/segments");
    assert_eq!(f.nodes.len(), 6, "nodes array must have 6 entries");
    assert_eq!(f.pipes.len(), 5, "pipes array must have 5 entries");
    assert_eq!(f.evaluate_only.pump_count, 0, "intake has pump_count == 0");
    // total_excavation repurposed as concrete volume
    assert_abs_diff_eq!(
        f.evaluate_only.total_excavation,
        f.evaluate_only.details.concrete_volume_m3 + f.evaluate_only.details.screen_concrete_m3,
        epsilon = 1.0 // rounded to 2dp so may differ slightly from raw
    );
}

// ── T-5.4c.b: evaluate() on oracle-reconstructed network ─────────────────────

/// T-5.4c.b: evaluate() on oracle-reconstructed golden network returns exact oracle score.
/// Formula: 600*(concrete+screen_concrete) + 120*pipe_L + 3000*violations.
/// total_excavation = concrete_vol + screen_concrete (repurposed).
#[test]
fn intake_evaluate_formula_exact() {
    let f = load_intake_golden();
    let constraints = DesignConstraints::default();
    let solver = IntakeSolver::new(
        None,
        constraints,
        "river".to_string(),
        f.solver_params.source_elevation,
        f.solver_params.pipe_elevation,
        f.solver_params.channel_slope,
        PipeMaterial::Concrete,
        f.solver_params.channel_width_factor,
        f.solver_params.channel_slope_factor,
        f.solver_params.weir_type,
        f.solver_params.screen_velocity_factor,
        0.15,
        0.025,
        0.010,
        0.015,
    );
    let network = reconstruct_intake_network(&f);

    let score = solver
        .evaluate(&network)
        .expect("evaluate must not error on valid oracle intake network");

    let oracle = &f.evaluate_only;

    // Cost parity
    assert_abs_diff_eq!(score.total_cost, oracle.total_cost, epsilon = 1e-4);
    // total_length parity
    assert_abs_diff_eq!(score.total_length, oracle.total_length, epsilon = 1e-6);
    // total_excavation (repurposed as concrete vol)
    assert_abs_diff_eq!(
        score.total_excavation,
        oracle.total_excavation,
        epsilon = 1e-9
    );
    // violations EXACT
    assert_eq!(
        score.norm_violations, oracle.norm_violations,
        "norm_violations must match oracle exactly"
    );
    // pump_count EXACT (always 0)
    assert_eq!(score.pump_count, oracle.pump_count, "pump_count must be 0");

    // Details
    let det = &oracle.details;
    let got_concrete = score
        .details
        .get("concrete_volume_m3")
        .and_then(|v| v.as_f64())
        .unwrap_or(-1.0);
    let got_screen = score
        .details
        .get("screen_concrete_m3")
        .and_then(|v| v.as_f64())
        .unwrap_or(-1.0);
    let got_pipe_cost = score
        .details
        .get("pipe_cost")
        .and_then(|v| v.as_f64())
        .unwrap_or(-1.0);
    let got_concrete_cost = score
        .details
        .get("concrete_cost")
        .and_then(|v| v.as_f64())
        .unwrap_or(-1.0);
    let got_ch_vel = score
        .details
        .get("channel_velocity")
        .and_then(|v| v.as_f64())
        .unwrap_or(-1.0);

    assert_abs_diff_eq!(got_concrete, det.concrete_volume_m3, epsilon = 1e-6);
    assert_abs_diff_eq!(got_screen, det.screen_concrete_m3, epsilon = 1e-6);
    assert_abs_diff_eq!(got_pipe_cost, det.pipe_cost, epsilon = 1e-6);
    assert_abs_diff_eq!(got_concrete_cost, det.concrete_cost, epsilon = 1e-6);
    assert_abs_diff_eq!(got_ch_vel, det.channel_velocity, epsilon = 1e-9);

    println!(
        "Intake evaluate: cost={:.6}, oracle={:.6}, concrete_vol={:.2}, violations={}",
        score.total_cost, oracle.total_cost, got_concrete, score.norm_violations
    );
}

// ── T-5.4c.c: cost formula constants ─────────────────────────────────────────

/// T-5.4c.c: Fixture cost formula coefficients are correct.
#[test]
fn intake_cost_formula_constants() {
    let f = load_intake_golden();
    let cf = &f.cost_formula;

    assert_abs_diff_eq!(cf.w_concrete, 600.0, epsilon = 1e-12);
    assert_abs_diff_eq!(cf.w_pipe, 120.0, epsilon = 1e-12);
    assert_abs_diff_eq!(cf.w_violations, 3000.0, epsilon = 1e-12);
}

// ── T-5.4c.d: E2E solve parity (golden) ──────────────────────────────────────

/// T-5.4c.d: solve_intake() on golden params produces oracle-matching score.
///
/// Parity assertions:
/// - screen dimensions within 1e-6
/// - channel depth within 1e-4 (linspace rounding)
/// - weir head/length within 1e-6
/// - pipe diameter EXACT
/// - total_cost within 1e-4
/// - violations EXACT
/// - pump_count EXACT (0)
#[test]
fn intake_solve_end_to_end_parity() {
    let f = load_intake_golden();
    let constraints = DesignConstraints::default();
    let solver = IntakeSolver::new(
        None,
        constraints,
        "river".to_string(),
        f.solver_params.source_elevation,
        f.solver_params.pipe_elevation,
        f.solver_params.channel_slope,
        PipeMaterial::Concrete,
        f.solver_params.channel_width_factor,
        f.solver_params.channel_slope_factor,
        f.solver_params.weir_type,
        f.solver_params.screen_velocity_factor,
        0.15,
        0.025,
        0.010,
        0.015,
    );

    let solutions = solver
        .solve_intake(
            f.solver_params.design_flow,
            "river",
            f.solver_params.source_elevation,
            f.solver_params.pipe_elevation,
            f.solver_params.channel_slope,
            PipeMaterial::Concrete,
            f.solver_params.num_alternatives,
            f.solver_params.channel_width_factor,
            f.solver_params.channel_slope_factor,
            f.solver_params.weir_type,
            f.solver_params.screen_velocity_factor,
            0.15,  // screen_velocity default
            0.025, // bar_spacing default
            0.010, // bar_thickness default
            0.015, // channel_roughness default
        )
        .expect("solve_intake must succeed on fixture params");

    assert!(
        !solutions.is_empty(),
        "solve_intake must return at least one solution"
    );
    let sol = &solutions[0];
    let oracle = &f.evaluate_only;
    let meta = &f.network_metadata;

    // Screen parity
    let screen_meta = sol
        .network
        .metadata
        .get("screen")
        .and_then(|v| v.as_object())
        .expect("screen metadata must be present");
    let got_net_width = screen_meta
        .get("net_width")
        .and_then(|v| v.as_f64())
        .expect("net_width");
    let got_height = screen_meta
        .get("height")
        .and_then(|v| v.as_f64())
        .expect("height");
    let got_gross_width = screen_meta
        .get("gross_width")
        .and_then(|v| v.as_f64())
        .expect("gross_width");
    assert_abs_diff_eq!(got_net_width, meta.screen.net_width, epsilon = 1e-6);
    assert_abs_diff_eq!(got_height, meta.screen.height, epsilon = 1e-6);
    assert_abs_diff_eq!(got_gross_width, meta.screen.gross_width, epsilon = 1e-6);

    // Channel depth parity (1e-4 due to linspace rounding)
    let channel_meta = sol
        .network
        .metadata
        .get("channel")
        .and_then(|v| v.as_object())
        .expect("channel metadata must be present");
    let got_depth = channel_meta
        .get("depth_m")
        .and_then(|v| v.as_f64())
        .expect("depth_m");
    let got_width = channel_meta
        .get("width_m")
        .and_then(|v| v.as_f64())
        .expect("width_m");
    let got_ch_vel = channel_meta
        .get("velocity_m_s")
        .and_then(|v| v.as_f64())
        .expect("velocity_m_s");
    assert_abs_diff_eq!(got_depth, meta.channel.depth_m, epsilon = 1e-4);
    assert_abs_diff_eq!(got_width, meta.channel.width_m, epsilon = 1e-9);
    assert_abs_diff_eq!(got_ch_vel, meta.channel.velocity_m_s, epsilon = 1e-9);

    // Weir parity
    let weir_meta = sol
        .network
        .metadata
        .get("weir")
        .and_then(|v| v.as_object())
        .expect("weir metadata must be present");
    let got_head = weir_meta
        .get("head")
        .and_then(|v| v.as_f64())
        .expect("head");
    let got_crest_len = weir_meta
        .get("crest_length")
        .and_then(|v| v.as_f64())
        .expect("crest_length");
    assert_abs_diff_eq!(got_head, meta.weir.head, epsilon = 1e-6);
    assert_abs_diff_eq!(got_crest_len, meta.weir.crest_length, epsilon = 1e-6);

    // Pipe diameter EXACT
    let pt_meta = sol
        .network
        .metadata
        .get("pipe_transition")
        .and_then(|v| v.as_object())
        .expect("pipe_transition metadata must be present");
    let got_diam = pt_meta
        .get("diameter")
        .and_then(|v| v.as_f64())
        .expect("diameter");
    assert!(
        (got_diam - meta.pipe_transition.diameter).abs() < 1e-9,
        "pipe diameter must match oracle exactly: got={got_diam:.9} expected={:.9}",
        meta.pipe_transition.diameter
    );

    // total_cost within 1e-4
    let abs_diff = (sol.score.total_cost - oracle.total_cost).abs();
    println!(
        "Intake E2E parity: Rust_cost={:.6}, oracle_cost={:.6}, |diff|={:.2e}",
        sol.score.total_cost, oracle.total_cost, abs_diff
    );
    assert!(
        abs_diff < 1e-4,
        "total_cost parity FAILED: Rust={:.6}, oracle={:.6}, |diff|={:.2e} >= 1e-4",
        sol.score.total_cost,
        oracle.total_cost,
        abs_diff
    );

    // violations EXACT
    assert_eq!(
        sol.score.norm_violations, oracle.norm_violations,
        "norm_violations mismatch: Rust={} oracle={}",
        sol.score.norm_violations, oracle.norm_violations
    );

    // pump_count EXACT (always 0)
    assert_eq!(
        sol.score.pump_count, oracle.pump_count,
        "pump_count must be 0"
    );

    // total_length within 1e-6
    assert_abs_diff_eq!(sol.score.total_length, oracle.total_length, epsilon = 1e-6);

    // total_excavation (repurposed) within 1e-6
    assert_abs_diff_eq!(
        sol.score.total_excavation,
        oracle.total_excavation,
        epsilon = 1e-6
    );

    // Rank 1 for single-alternative
    assert_eq!(sol.rank, 1, "single-alternative must have rank 1");
}

// ── T-5.4c.e: network layout invariants ──────────────────────────────────────

/// T-5.4c.e: Intake network has exactly 6 nodes, 5 pipes, correct node types.
/// Layout: Source(RESERVOIR) → Screen(INLET) → ChannelIn(JUNCTION) →
///         ChannelOut(JUNCTION) → Weir(JUNCTION) → PipeStart(OUTLET).
#[test]
fn intake_network_layout_invariants() {
    let f = load_intake_golden();
    let constraints = DesignConstraints::default();
    let solver = IntakeSolver::new(
        None,
        constraints,
        "river".to_string(),
        f.solver_params.source_elevation,
        f.solver_params.pipe_elevation,
        f.solver_params.channel_slope,
        PipeMaterial::Concrete,
        f.solver_params.channel_width_factor,
        f.solver_params.channel_slope_factor,
        f.solver_params.weir_type,
        f.solver_params.screen_velocity_factor,
        0.15,
        0.025,
        0.010,
        0.015,
    );

    let solutions = solver
        .solve_intake(
            f.solver_params.design_flow,
            "river",
            f.solver_params.source_elevation,
            f.solver_params.pipe_elevation,
            f.solver_params.channel_slope,
            PipeMaterial::Concrete,
            f.solver_params.num_alternatives,
            f.solver_params.channel_width_factor,
            f.solver_params.channel_slope_factor,
            f.solver_params.weir_type,
            f.solver_params.screen_velocity_factor,
            0.15,
            0.025,
            0.010,
            0.015,
        )
        .expect("solve_intake must succeed");

    let network = &solutions[0].network;

    assert_eq!(network.node_count(), 6, "must have exactly 6 nodes");
    assert_eq!(network.pipe_count(), 5, "must have exactly 5 pipe segments");

    // Check node IDs and types
    let expected_node_types = [
        ("Source", NodeType::Reservoir),
        ("Screen", NodeType::Inlet),
        ("ChannelIn", NodeType::Junction),
        ("ChannelOut", NodeType::Junction),
        ("Weir", NodeType::Junction),
        ("PipeStart", NodeType::Outlet),
    ];

    for (expected_id, expected_type) in &expected_node_types {
        let node = network
            .nodes
            .iter()
            .find(|n| n.id.0 == *expected_id)
            .unwrap_or_else(|| panic!("node '{expected_id}' must be present"));
        assert_eq!(
            node.node_type, *expected_type,
            "node '{expected_id}' must be type {:?}",
            expected_type
        );
    }

    // Check pipe segment IDs
    let expected_pipe_ids = [
        "Seg-Screen",
        "Seg-ChIn",
        "Seg-Channel",
        "Seg-Weir",
        "Seg-Pipe",
    ];
    for pid in &expected_pipe_ids {
        assert!(
            network.pipes.iter().any(|p| p.id.0 == *pid),
            "pipe segment '{pid}' must be present"
        );
    }

    // All pipes have positive length
    for pipe in &network.pipes {
        assert!(
            pipe.length > 0.0,
            "pipe {} must have positive length",
            pipe.id
        );
    }

    // Pipe inverts = node.z - 0.2 (FIXED 0.2m cover)
    for pipe in &network.pipes {
        let sn = network
            .nodes
            .iter()
            .find(|n| n.id == pipe.start_node_id)
            .expect("start node");
        let en = network
            .nodes
            .iter()
            .find(|n| n.id == pipe.end_node_id)
            .expect("end node");
        assert_abs_diff_eq!(pipe.start_invert, sn.z - 0.2, epsilon = 1e-9);
        assert_abs_diff_eq!(pipe.end_invert, en.z - 0.2, epsilon = 1e-9);
    }
}

// ── T-5.4c.f: two-pass depth search + velocity rounding ──────────────────────

/// T-5.4c.f: Verify the two-pass depth search and velocity rounding.
///
/// The channel velocity_m_s in metadata MUST be the ROUNDED (3dp) value
/// from rectangular_channel_flow_full, NOT the raw float from pass 1.
/// evaluate() reads the rounded value for violation checks.
#[test]
fn intake_channel_velocity_is_rounded_in_metadata() {
    let constraints = DesignConstraints::default();
    let solver = IntakeSolver::new(
        None,
        constraints,
        "river".to_string(),
        120.0,
        118.0,
        0.005,
        PipeMaterial::Concrete,
        1.0,
        1.0,
        0,
        1.0,
        0.15,
        0.025,
        0.010,
        0.015,
    );

    let solutions = solver
        .solve_intake(
            0.050,
            "river",
            120.0,
            118.0,
            0.005,
            PipeMaterial::Concrete,
            1,
            1.0,
            1.0,
            0, // rectangular weir
            1.0,
            0.15,
            0.025,
            0.010,
            0.015,
        )
        .expect("solve must succeed");

    let channel_meta = solutions[0]
        .network
        .metadata
        .get("channel")
        .and_then(|v| v.as_object())
        .expect("channel metadata");

    let vel = channel_meta
        .get("velocity_m_s")
        .and_then(|v| v.as_f64())
        .expect("velocity_m_s");

    // Must be rounded to exactly 3 decimal places (= oracle value 0.832)
    let rounded = (vel * 1000.0).round() / 1000.0;
    assert!(
        (vel - rounded).abs() < 1e-12,
        "velocity_m_s must be exactly 3dp rounded: got {vel}"
    );

    // Oracle value is 0.832
    assert_abs_diff_eq!(vel, 0.832, epsilon = 1e-9);

    println!("Channel velocity (3dp rounded): got={vel:.9}, expected=0.832");
}

// ── T-5.4c.g: weir formulas (rectangular + v_notch) ──────────────────────────

/// T-5.4c.g: Rectangular weir formula: Q = 1.84 * L * H^1.5.
/// Verifies crest_length and head match oracle for fixture 1.
#[test]
fn intake_rectangular_weir_formula() {
    let f = load_intake_golden();
    let constraints = DesignConstraints::default();
    let solver = IntakeSolver::new(
        None,
        constraints,
        "river".to_string(),
        f.solver_params.source_elevation,
        f.solver_params.pipe_elevation,
        f.solver_params.channel_slope,
        PipeMaterial::Concrete,
        f.solver_params.channel_width_factor,
        f.solver_params.channel_slope_factor,
        f.solver_params.weir_type,
        f.solver_params.screen_velocity_factor,
        0.15,
        0.025,
        0.010,
        0.015,
    );

    let solutions = solver
        .solve_intake(
            f.solver_params.design_flow,
            "river",
            f.solver_params.source_elevation,
            f.solver_params.pipe_elevation,
            f.solver_params.channel_slope,
            PipeMaterial::Concrete,
            f.solver_params.num_alternatives,
            f.solver_params.channel_width_factor,
            f.solver_params.channel_slope_factor,
            0, // rectangular weir
            f.solver_params.screen_velocity_factor,
            0.15,
            0.025,
            0.010,
            0.015,
        )
        .expect("solve_intake must succeed");

    let weir_meta = solutions[0]
        .network
        .metadata
        .get("weir")
        .and_then(|v| v.as_object())
        .expect("weir metadata");

    let weir_type = weir_meta
        .get("weir_type")
        .and_then(|v| v.as_str())
        .expect("weir_type");
    assert_eq!(weir_type, "rectangular", "weir_type must be rectangular");

    let crest_len = weir_meta
        .get("crest_length")
        .and_then(|v| v.as_f64())
        .expect("crest_length");
    let head = weir_meta
        .get("head")
        .and_then(|v| v.as_f64())
        .expect("head");

    assert_abs_diff_eq!(
        crest_len,
        f.network_metadata.weir.crest_length,
        epsilon = 1e-6
    );
    assert_abs_diff_eq!(head, f.network_metadata.weir.head, epsilon = 1e-6);

    println!(
        "Rectangular weir: crest_length={:.6}, head={:.6}",
        crest_len, head
    );
}

/// T-5.4c.h: V-notch weir formula: Q = 1.38 * tan(45°) * H^2.5.
/// Verifies head matches oracle, crest_length=0.0, angle_degrees=90.0.
#[test]
fn intake_v_notch_weir_formula() {
    let f = load_intake_alternatives();
    let constraints = DesignConstraints::default();
    let solver = IntakeSolver::new(
        None,
        constraints,
        "river".to_string(),
        f.solver_params.source_elevation,
        f.solver_params.pipe_elevation,
        f.solver_params.channel_slope,
        PipeMaterial::Concrete,
        f.solver_params.channel_width_factor,
        f.solver_params.channel_slope_factor,
        f.solver_params.weir_type,
        f.solver_params.screen_velocity_factor,
        0.15,
        0.025,
        0.010,
        0.015,
    );

    let solutions = solver
        .solve_intake(
            f.solver_params.design_flow,
            "river",
            f.solver_params.source_elevation,
            f.solver_params.pipe_elevation,
            f.solver_params.channel_slope,
            PipeMaterial::Concrete,
            f.solver_params.num_alternatives,
            f.solver_params.channel_width_factor,
            f.solver_params.channel_slope_factor,
            1, // v_notch weir
            f.solver_params.screen_velocity_factor,
            0.15,
            0.025,
            0.010,
            0.015,
        )
        .expect("solve_intake alternatives must succeed");

    // All alternatives share the same weir (weir doesn't depend on width_mult)
    let weir_meta = solutions[0]
        .network
        .metadata
        .get("weir")
        .and_then(|v| v.as_object())
        .expect("weir metadata");

    let weir_type = weir_meta
        .get("weir_type")
        .and_then(|v| v.as_str())
        .expect("weir_type");
    assert_eq!(weir_type, "v_notch", "weir_type must be v_notch");

    let crest_length = weir_meta
        .get("crest_length")
        .and_then(|v| v.as_f64())
        .expect("crest_length");
    let angle = weir_meta
        .get("angle_degrees")
        .and_then(|v| v.as_f64())
        .expect("angle_degrees");
    let head = weir_meta
        .get("head")
        .and_then(|v| v.as_f64())
        .expect("head");

    assert!(
        crest_length.abs() < 1e-12,
        "v_notch crest_length must be 0.0, got {crest_length}"
    );
    assert!(
        (angle - 90.0).abs() < 1e-12,
        "v_notch angle must be 90.0, got {angle}"
    );

    // Oracle head from fixture 2, alt 1
    let oracle_head = f.solutions[0].network_metadata.weir.head;
    assert_abs_diff_eq!(head, oracle_head, epsilon = 1e-6);

    println!(
        "V-notch weir: head={:.6}, crest_length={crest_length}, angle={angle}",
        head
    );
}

// ── T-5.4c.i: alternatives pairwise-distinct costs ───────────────────────────

/// T-5.4c.i: solve_intake() with num_alternatives=3 returns pairwise-strictly
/// distinct total_costs, sorted ascending. Validates per-alt cost vs oracle.
#[test]
fn intake_alternatives_pairwise_distinct() {
    let f = load_intake_alternatives();
    let constraints = DesignConstraints::default();
    let solver = IntakeSolver::new(
        None,
        constraints,
        "river".to_string(),
        f.solver_params.source_elevation,
        f.solver_params.pipe_elevation,
        f.solver_params.channel_slope,
        PipeMaterial::Concrete,
        f.solver_params.channel_width_factor,
        f.solver_params.channel_slope_factor,
        f.solver_params.weir_type,
        f.solver_params.screen_velocity_factor,
        0.15,
        0.025,
        0.010,
        0.015,
    );

    let solutions = solver
        .solve_intake(
            f.solver_params.design_flow,
            "river",
            f.solver_params.source_elevation,
            f.solver_params.pipe_elevation,
            f.solver_params.channel_slope,
            PipeMaterial::Concrete,
            f.solver_params.num_alternatives,
            f.solver_params.channel_width_factor,
            f.solver_params.channel_slope_factor,
            f.solver_params.weir_type,
            f.solver_params.screen_velocity_factor,
            0.15,
            0.025,
            0.010,
            0.015,
        )
        .expect("solve_intake alternatives must succeed");

    assert_eq!(
        solutions.len(),
        f.num_solutions_returned,
        "solution count {} does not match oracle {}",
        solutions.len(),
        f.num_solutions_returned
    );

    let costs: Vec<f64> = solutions.iter().map(|s| s.score.total_cost).collect();

    // Per-solution cost matches oracle within 1e-4
    for (i, (sol, oracle_sol)) in solutions.iter().zip(f.solutions.iter()).enumerate() {
        let abs_diff = (sol.score.total_cost - oracle_sol.total_cost).abs();
        println!(
            "  Alt {}: Rust={:.6} oracle={:.6} |diff|={:.2e}",
            i + 1,
            sol.score.total_cost,
            oracle_sol.total_cost,
            abs_diff
        );
        assert!(
            abs_diff < 1e-4,
            "alternatives[{}] cost parity FAILED: Rust={:.6} oracle={:.6} |diff|={:.2e}",
            i,
            sol.score.total_cost,
            oracle_sol.total_cost,
            abs_diff
        );
    }

    // Strictly ascending costs (gap >= 1e-3)
    for i in 0..costs.len().saturating_sub(1) {
        assert!(
            costs[i + 1] - costs[i] >= 1e-3,
            "alternatives must be strictly ascending: costs[{}]={:.6} >= costs[{}]={:.6}",
            i,
            costs[i],
            i + 1,
            costs[i + 1]
        );
    }

    // Pairwise-distinct costs
    for i in 0..costs.len() {
        for j in (i + 1)..costs.len() {
            assert!(
                (costs[i] - costs[j]).abs() >= 1e-3,
                "alternatives costs[{}]={:.6} and costs[{}]={:.6} must be pairwise distinct",
                i,
                costs[i],
                j,
                costs[j]
            );
        }
    }

    // Rank numbers correct (1-based, sorted by cost ascending)
    for (i, sol) in solutions.iter().enumerate() {
        assert_eq!(
            sol.rank,
            (i + 1) as i64,
            "solution rank must be {} got {}",
            i + 1,
            sol.rank
        );
    }

    println!(
        "Intake alternatives: {} solutions, costs={:?}",
        solutions.len(),
        costs
    );
}

// ── T-5.4c.j: Solver trait default method returns error ──────────────────────

/// T-5.4c.j: IntakeSolver::solve() (Solver trait) now delegates to solve_intake.
/// The old stub that returned Err has been replaced by a real delegation wrapper.
/// With the wired fields and valid golden-fixture params, Solver::solve must return Ok.
#[test]
fn intake_solver_trait_default_errors() {
    let constraints = DesignConstraints::default();
    let mut solver = IntakeSolver::new(
        None,
        constraints,
        "river".to_string(),
        120.0,
        118.0,
        0.005,
        PipeMaterial::Concrete,
        1.0,
        1.0,
        0,
        1.0,
        0.15,
        0.025,
        0.010,
        0.015,
    );
    let params = SolverParams {
        design_flow: 0.05,
        num_alternatives: 1,
        ..SolverParams::default()
    };
    let result = solver.solve(&params);
    // Delegation is now live: valid params produce a real intake design.
    assert!(
        result.is_ok(),
        "Solver::solve() must return Ok after wiring — got: {:?}",
        result.err()
    );
    assert!(
        !result.unwrap().is_empty(),
        "Solver::solve() must return at least one solution (REQ-007)"
    );
}

// ── T-6.5.F: intake_solve_via_trait_roundtrip ─────────────────────────────────

/// T-6.5.F (RED → GREEN via PR-F): `Solver::solve` delegates to `solve_intake`
/// and returns a feasible solution for the golden fixture.
///
/// Scenario 1 — happy path: construct with all 13 fixture fields, call
/// `Solver::solve`, assert `Ok(solutions)` with `len >= 1` and `total_cost > 0.0`.
///
/// Scenario 2 — invalid source_type: construct with `source_type = "unknown_type"`,
/// call `Solver::solve`, assert `Err(_)` — NOT `Ok(vec![])`.
#[test]
fn intake_solve_via_trait_roundtrip() {
    let f = load_intake_golden();

    // ── Scenario 1: happy path via trait ─────────────────────────────────────
    {
        let constraints = DesignConstraints::default();
        let mut solver = IntakeSolver::new(
            None,
            constraints,
            "river".to_string(),                    // source_type
            f.solver_params.source_elevation,       // source_elevation
            f.solver_params.pipe_elevation,         // pipe_elevation
            f.solver_params.channel_slope,          // channel_slope
            PipeMaterial::Concrete,                 // material
            f.solver_params.channel_width_factor,   // channel_width_factor
            f.solver_params.channel_slope_factor,   // channel_slope_factor
            f.solver_params.weir_type,              // weir_type
            f.solver_params.screen_velocity_factor, // screen_velocity_factor
            0.15,                                   // screen_velocity default
            0.025,                                  // bar_spacing default
            0.010,                                  // bar_thickness default
            0.015,                                  // channel_roughness default
        );

        let params = SolverParams {
            design_flow: f.solver_params.design_flow,
            num_alternatives: f.solver_params.num_alternatives,
            ..SolverParams::default()
        };

        let result = Solver::solve(&mut solver, &params);
        assert!(
            result.is_ok(),
            "Solver::solve (happy path) must return Ok — got {:?}",
            result
        );
        let solutions = result.unwrap();
        assert!(
            !solutions.is_empty(),
            "Solver::solve must return at least one solution, got 0"
        );
        assert!(
            solutions[0].score.total_cost > 0.0,
            "solutions[0].score.total_cost must be > 0.0, got {}",
            solutions[0].score.total_cost
        );
    }

    // ── Scenario 2: invalid source_type must yield Err ────────────────────────
    {
        let constraints = DesignConstraints::default();
        let mut solver = IntakeSolver::new(
            None,
            constraints,
            "unknown_type".to_string(), // invalid source_type
            f.solver_params.source_elevation,
            f.solver_params.pipe_elevation,
            f.solver_params.channel_slope,
            PipeMaterial::Concrete,
            f.solver_params.channel_width_factor,
            f.solver_params.channel_slope_factor,
            f.solver_params.weir_type,
            f.solver_params.screen_velocity_factor,
            0.15,
            0.025,
            0.010,
            0.015,
        );

        let params = SolverParams {
            design_flow: f.solver_params.design_flow,
            num_alternatives: f.solver_params.num_alternatives,
            ..SolverParams::default()
        };

        let result = Solver::solve(&mut solver, &params);
        assert!(
            result.is_err(),
            "Solver::solve with unknown_type must return Err, got Ok"
        );
    }
}
