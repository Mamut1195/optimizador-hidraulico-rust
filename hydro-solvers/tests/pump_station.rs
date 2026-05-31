//! T-5.4b — PumpStationSolver oracle-parity integration tests.
//!
//! Parity gate:
//!   - total_cost within 1e-4 of oracle (E2E solve)
//!   - pump_count EXACT
//!   - norm_violations EXACT
//!   - total_length within 1e-6
//!   - power_kw EXACT (motor rounding is deterministic)
//!   - efficiency within 1e-6
//!   - wet_well dimensions within 1e-6
//!   - total_excavation (= wet_well_volume) within 1e-6
//!   - alternatives: pairwise-distinct total_costs, sorted ascending

use approx::assert_abs_diff_eq;
use hydro_solvers::{PumpStationSolver, Solver, SolverParams};
use hydro_types::{
    DesignConstraints, NetworkNode, NetworkPipe, NodeId, NodeType, PipeId, PipeMaterial,
    PipeNetwork,
};
use oracle::solvers::{load_pump_station_alternatives, load_pump_station_golden, PumpGolden};
use serde_json::Value;
use std::collections::HashMap;

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Parse a node type string from the pump station oracle fixture.
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
        _ => panic!("Unknown node type in pump station fixture: {s}"),
    }
}

/// Reconstruct the oracle-built PipeNetwork from golden fixture data.
///
/// This is used for the evaluate-only test (T-5.4b.a).
fn reconstruct_pump_network(f: &PumpGolden) -> PipeNetwork {
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
                material: PipeMaterial::Steel,
                roughness: PipeMaterial::Steel.manning_n(),
                start_invert: p.start_invert,
                end_invert: p.end_invert,
                slope: p.slope,
                design_flow: p.design_flow,
                waypoints: vec![],
                metadata: HashMap::new(),
            }
        })
        .collect();

    let mut network = PipeNetwork::new("PumpStationTest", nodes, pipes);

    // Inject network metadata from fixture so evaluate() can read pump/wet_well info
    let meta = &f.network_metadata;
    let pump = &meta.pump;
    let ww = &meta.wet_well;
    network
        .metadata
        .insert("tdh".to_string(), Value::from(meta.tdh));
    network
        .metadata
        .insert("static_lift".to_string(), Value::from(meta.static_lift));
    network
        .metadata
        .insert("hw_c".to_string(), Value::from(meta.hw_c));
    network.metadata.insert(
        "pump".to_string(),
        serde_json::json!({
            "flow_m3s": pump.flow_m3s,
            "tdh_m": pump.tdh_m,
            "power_kw": pump.power_kw,
            "efficiency": pump.efficiency,
            "num_operating": pump.num_operating,
            "num_reserve": pump.num_reserve,
            "total_pumps": pump.total_pumps,
        }),
    );
    network.metadata.insert(
        "wet_well".to_string(),
        serde_json::json!({
            "volume_m3": ww.volume_m3,
            "width_m": ww.width_m,
            "length_m": ww.length_m,
            "depth_m": ww.depth_m,
            "retention_minutes": ww.retention_minutes,
            "num_pumps_installed": ww.num_pumps_installed,
        }),
    );

    network
}

// ── T-5.4b.a: fixture loads and has correct shape ─────────────────────────────

/// T-5.4b.a: Fixture loads without error and structural counts match.
#[test]
fn pump_station_golden_loads_and_has_correct_shape() {
    let f = load_pump_station_golden();
    assert_eq!(f.schema_version, 1);
    assert_eq!(f.node_count, 5, "pump station always has 5 nodes");
    assert_eq!(f.pipe_count, 4, "pump station always has 4 pipes");
    assert_eq!(f.nodes.len(), 5, "nodes array must have 5 entries");
    assert_eq!(f.pipes.len(), 4, "pipes array must have 4 entries");
    assert_eq!(
        f.evaluate_only.pump_count, f.network_metadata.pump.total_pumps,
        "pump_count must equal pump.total_pumps"
    );
    // total_excavation repurposed as wet_well_volume
    assert_abs_diff_eq!(
        f.evaluate_only.total_excavation,
        f.network_metadata.wet_well.volume_m3,
        epsilon = 1e-6
    );
}

// ── T-5.4b.b: evaluate() on oracle-reconstructed network ─────────────────────

/// T-5.4b.b: evaluate() on oracle-reconstructed golden network returns exact oracle score.
/// Formula: 1200*kW*pumps + 800*well_vol + 150*L + 5000*violations.
/// total_excavation = well_volume (repurposed).
#[test]
fn pump_station_evaluate_formula_exact() {
    let f = load_pump_station_golden();
    let constraints = DesignConstraints::default();
    let solver = PumpStationSolver::new(None, constraints);
    let network = reconstruct_pump_network(&f);

    let score = solver
        .evaluate(&network)
        .expect("evaluate must not error on valid oracle pump station network");

    let oracle = &f.evaluate_only;

    // Cost parity
    assert_abs_diff_eq!(score.total_cost, oracle.total_cost, epsilon = 1e-4);
    // total_length parity
    assert_abs_diff_eq!(score.total_length, oracle.total_length, epsilon = 1e-6);
    // total_excavation = well_volume (repurposed)
    assert_abs_diff_eq!(
        score.total_excavation,
        oracle.total_excavation,
        epsilon = 1e-6
    );
    // violations EXACT
    assert_eq!(
        score.norm_violations, oracle.norm_violations,
        "norm_violations must match oracle exactly"
    );
    // pump_count EXACT
    assert_eq!(
        score.pump_count, oracle.pump_count,
        "pump_count must match oracle exactly"
    );

    // Details
    let details = &oracle.details;
    let got_power = score
        .details
        .get("power_kw_per_pump")
        .and_then(|v| v.as_f64())
        .unwrap_or(-1.0);
    let got_total_pumps = score
        .details
        .get("total_pumps")
        .and_then(|v| v.as_i64())
        .unwrap_or(-1);
    let got_well_vol = score
        .details
        .get("wet_well_volume_m3")
        .and_then(|v| v.as_f64())
        .unwrap_or(-1.0);

    assert_abs_diff_eq!(got_power, details.power_kw_per_pump, epsilon = 1e-6);
    assert_eq!(
        got_total_pumps, details.total_pumps as i64,
        "total_pumps detail must match"
    );
    assert_abs_diff_eq!(got_well_vol, details.wet_well_volume_m3, epsilon = 1e-6);

    println!(
        "Pump evaluate: cost={:.6}, oracle={:.6}, power_kw={:.2}, violations={}",
        score.total_cost, oracle.total_cost, got_power, score.norm_violations
    );
}

// ── T-5.4b.c: cost formula constants ─────────────────────────────────────────

/// T-5.4b.c: Fixture cost formula coefficients are correct and self-consistent.
#[test]
fn pump_station_cost_formula_constants() {
    let f = load_pump_station_golden();
    let cf = &f.cost_formula;

    assert_abs_diff_eq!(cf.w_equipment, 1200.0, epsilon = 1e-12);
    assert_abs_diff_eq!(cf.w_civil, 800.0, epsilon = 1e-12);
    assert_abs_diff_eq!(cf.w_pipe, 150.0, epsilon = 1e-12);
    assert_abs_diff_eq!(cf.w_violations, 5000.0, epsilon = 1e-12);

    // Cross-check: reconstruct cost from rounded detail components.
    // pipe_cost is rounded to 0dp in the fixture (9556.0 vs actual 9556.5), so
    // the cross-check can differ by up to 1.0 from the actual cost.
    let eo = &f.evaluate_only;
    let expected = eo.details.equipment_cost
        + eo.details.civil_cost
        + eo.details.pipe_cost
        + eo.norm_violations as f64 * cf.w_violations;
    assert!(
        (expected - cf.expected_cost).abs() < 1.0,
        "cost formula cross-check: computed={expected:.4} oracle={:.4} |diff|={:.4}",
        cf.expected_cost,
        (expected - cf.expected_cost).abs()
    );
}

// ── T-5.4b.d: E2E solve parity (golden) ──────────────────────────────────────

/// T-5.4b.d: solve_pump_station() on golden params produces oracle-matching score.
///
/// Parity assertions (from spec):
/// - tdh within 1e-6
/// - power_kw EXACT (motor rounding is deterministic)
/// - efficiency within 1e-6
/// - wet_well dimensions within 1e-6
/// - total_cost within 1e-4
/// - pump_count EXACT
/// - violations EXACT
#[test]
fn pump_station_solve_end_to_end_parity() {
    let f = load_pump_station_golden();
    let constraints = DesignConstraints::default();
    let solver = PumpStationSolver::new(None, constraints);

    let solutions = solver
        .solve_pump_station(
            f.solver_params.design_flow,
            f.solver_params.suction_elevation,
            f.solver_params.discharge_elevation,
            f.solver_params.suction_pipe_length,
            f.solver_params.discharge_pipe_length,
            f.solver_params.suction_diameter,
            f.solver_params.discharge_diameter,
            PipeMaterial::Steel,
            f.solver_params.num_alternatives,
            None,
            None,
            None,
            1.0, // wet_well_factor
            5.0, // retention_minutes
        )
        .expect("solve_pump_station must succeed on fixture params");

    assert!(
        !solutions.is_empty(),
        "solve_pump_station must return at least one solution"
    );
    let sol = &solutions[0];
    let oracle = &f.evaluate_only;
    let meta = &f.network_metadata;

    // TDH parity
    let got_tdh = sol
        .network
        .metadata
        .get("tdh")
        .and_then(|v| v.as_f64())
        .expect("tdh must be in network metadata");
    assert_abs_diff_eq!(got_tdh, meta.tdh, epsilon = 1e-6);

    // power_kw EXACT (motor table rounding is deterministic)
    let pump_info = sol
        .network
        .metadata
        .get("pump")
        .and_then(|v| v.as_object())
        .expect("pump metadata must be present");
    let got_power = pump_info
        .get("power_kw")
        .and_then(|v| v.as_f64())
        .expect("power_kw must be in pump metadata");
    assert_abs_diff_eq!(got_power, meta.pump.power_kw, epsilon = 1e-9);

    // efficiency within 1e-6
    let got_eta = pump_info
        .get("efficiency")
        .and_then(|v| v.as_f64())
        .expect("efficiency must be in pump metadata");
    assert_abs_diff_eq!(got_eta, meta.pump.efficiency, epsilon = 1e-6);

    // num_operating EXACT
    let got_num_op = pump_info
        .get("num_operating")
        .and_then(|v| v.as_i64())
        .expect("num_operating must be in pump metadata");
    assert_eq!(
        got_num_op, meta.pump.num_operating,
        "num_operating mismatch"
    );

    // total_pumps EXACT
    let got_total_pumps = pump_info
        .get("total_pumps")
        .and_then(|v| v.as_i64())
        .expect("total_pumps must be in pump metadata");
    assert_eq!(
        got_total_pumps, meta.pump.total_pumps,
        "total_pumps mismatch"
    );

    // Wet well dimensions within 1e-6
    let ww_info = sol
        .network
        .metadata
        .get("wet_well")
        .and_then(|v| v.as_object())
        .expect("wet_well metadata must be present");
    let got_vol = ww_info
        .get("volume_m3")
        .and_then(|v| v.as_f64())
        .expect("volume_m3");
    let got_width = ww_info
        .get("width_m")
        .and_then(|v| v.as_f64())
        .expect("width_m");
    let got_length = ww_info
        .get("length_m")
        .and_then(|v| v.as_f64())
        .expect("length_m");
    let got_depth = ww_info
        .get("depth_m")
        .and_then(|v| v.as_f64())
        .expect("depth_m");

    let oracle_ww = &meta.wet_well;
    assert_abs_diff_eq!(got_vol, oracle_ww.volume_m3, epsilon = 1e-6);
    assert_abs_diff_eq!(got_width, oracle_ww.width_m, epsilon = 1e-6);
    assert_abs_diff_eq!(got_length, oracle_ww.length_m, epsilon = 1e-6);
    assert_abs_diff_eq!(got_depth, oracle_ww.depth_m, epsilon = 1e-6);

    // total_cost within 1e-4
    let abs_diff = (sol.score.total_cost - oracle.total_cost).abs();
    println!(
        "Pump E2E parity: Rust_cost={:.6}, oracle_cost={:.6}, |diff|={:.2e}",
        sol.score.total_cost, oracle.total_cost, abs_diff
    );
    assert!(
        abs_diff < 1e-4,
        "total_cost parity FAILED: Rust={:.6}, oracle={:.6}, |diff|={:.2e} >= 1e-4",
        sol.score.total_cost,
        oracle.total_cost,
        abs_diff
    );

    // pump_count EXACT
    assert_eq!(
        sol.score.pump_count, oracle.pump_count,
        "pump_count mismatch: Rust={} oracle={}",
        sol.score.pump_count, oracle.pump_count
    );

    // violations EXACT
    assert_eq!(
        sol.score.norm_violations, oracle.norm_violations,
        "norm_violations mismatch: Rust={} oracle={}",
        sol.score.norm_violations, oracle.norm_violations
    );

    // total_length within 1e-6
    assert_abs_diff_eq!(sol.score.total_length, oracle.total_length, epsilon = 1e-6);

    // total_excavation = well_volume (repurposed)
    assert_abs_diff_eq!(
        sol.score.total_excavation,
        oracle.total_excavation,
        epsilon = 1e-6
    );

    // Rank 1 for single-alternative
    assert_eq!(sol.rank, 1, "single-alternative must have rank 1");
}

// ── T-5.4b.e: network layout invariants ──────────────────────────────────────

/// T-5.4b.e: Pump station network has exactly 5 nodes, 4 pipes, correct node types.
/// Node order: Inlet(INLET) → WetWell(RESERVOIR) → PumpManifold(PUMP) →
///             DischargeManifold(JUNCTION) → Outlet(OUTLET).
#[test]
fn pump_station_network_layout_invariants() {
    let f = load_pump_station_golden();
    let constraints = DesignConstraints::default();
    let solver = PumpStationSolver::new(None, constraints);

    let solutions = solver
        .solve_pump_station(
            f.solver_params.design_flow,
            f.solver_params.suction_elevation,
            f.solver_params.discharge_elevation,
            f.solver_params.suction_pipe_length,
            f.solver_params.discharge_pipe_length,
            f.solver_params.suction_diameter,
            f.solver_params.discharge_diameter,
            PipeMaterial::Steel,
            f.solver_params.num_alternatives,
            None,
            None,
            None,
            1.0,
            5.0,
        )
        .expect("solve_pump_station must succeed");

    let network = &solutions[0].network;

    assert_eq!(network.node_count(), 5, "must have exactly 5 nodes");
    assert_eq!(network.pipe_count(), 4, "must have exactly 4 pipes");

    // Check node IDs and types
    let expected_node_types = [
        ("Inlet", NodeType::Inlet),
        ("WetWell", NodeType::Reservoir),
        ("PumpManifold", NodeType::Pump),
        ("DischargeManifold", NodeType::Junction),
        ("Outlet", NodeType::Outlet),
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

    // Check pipe IDs
    let expected_pipe_ids = ["Pipe - (1)", "Pipe - (2)", "Pipe - (3)", "Pipe - (4)"];
    for pid in &expected_pipe_ids {
        assert!(
            network.pipes.iter().any(|p| p.id.0 == *pid),
            "pipe '{pid}' must be present"
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

    // Pipe inverts = z - 0.5 for both start and end nodes
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
        assert_abs_diff_eq!(pipe.start_invert, sn.z - 0.5, epsilon = 1e-9);
        assert_abs_diff_eq!(pipe.end_invert, en.z - 0.5, epsilon = 1e-9);
    }
}

// ── T-5.4b.f: alternatives pairwise-distinct costs ───────────────────────────

/// T-5.4b.f: solve_pump_station() with num_alternatives=3 returns pairwise-strictly
/// distinct total_costs, sorted ascending. Validates per-alt cost vs oracle.
#[test]
fn pump_station_alternatives_pairwise_distinct() {
    let f = load_pump_station_alternatives();
    let constraints = DesignConstraints::default();
    let solver = PumpStationSolver::new(None, constraints);

    let solutions = solver
        .solve_pump_station(
            f.solver_params.design_flow,
            f.solver_params.suction_elevation,
            f.solver_params.discharge_elevation,
            f.solver_params.suction_pipe_length,
            f.solver_params.discharge_pipe_length,
            f.solver_params.suction_diameter,
            f.solver_params.discharge_diameter,
            PipeMaterial::Steel,
            f.solver_params.num_alternatives,
            None,
            None,
            None,
            1.0,
            5.0,
        )
        .expect("solve_pump_station alternatives must succeed");

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

    // Strictly ascending costs (gap >= 1e-3 between consecutive solutions)
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
        "Pump alternatives: {} solutions, costs={:?}",
        solutions.len(),
        costs
    );
}

// ── T-5.4b.g: TDH abs(static_lift) trap ──────────────────────────────────────

/// T-5.4b.g: TDH uses abs(static_lift) — even negative static lift gives positive TDH.
/// This verifies the TRAP from the spec: negative suction-to-discharge static head
/// does NOT reduce TDH below friction + minor losses.
#[test]
fn pump_station_tdh_abs_static_lift_trap() {
    let constraints = DesignConstraints::default();
    let solver = PumpStationSolver::new(None, constraints);

    // Discharge BELOW suction → static_lift = 95.0 - 100.0 = -5.0 (negative)
    // abs(-5.0) = 5.0 should be used, not -5.0
    let solutions_negative_lift = solver
        .solve_pump_station(
            0.050,
            100.0, // suction higher than discharge
            95.0,  // discharge lower
            10.0,
            50.0,
            0.3,
            0.25,
            PipeMaterial::Steel,
            1,
            None,
            None,
            None,
            1.0,
            5.0,
        )
        .expect("solve with negative static lift must succeed");

    let solutions_positive_lift = solver
        .solve_pump_station(
            0.050,
            100.0,
            105.0, // discharge above suction, same abs(static_lift)=5.0
            10.0,
            50.0,
            0.3,
            0.25,
            PipeMaterial::Steel,
            1,
            None,
            None,
            None,
            1.0,
            5.0,
        )
        .expect("solve with positive static lift must succeed");

    let tdh_neg = solutions_negative_lift[0]
        .network
        .metadata
        .get("tdh")
        .and_then(|v| v.as_f64())
        .expect("tdh must be in metadata");
    let tdh_pos = solutions_positive_lift[0]
        .network
        .metadata
        .get("tdh")
        .and_then(|v| v.as_f64())
        .expect("tdh must be in metadata");

    // TDH must be same for +5 and -5 static lift (abs is applied)
    assert!(
        (tdh_neg - tdh_pos).abs() < 1e-6,
        "TDH must be equal for abs(static_lift)=5.0 regardless of sign: tdh_neg={tdh_neg:.9}, tdh_pos={tdh_pos:.9}"
    );
    assert!(
        tdh_neg > 0.0,
        "TDH must be positive even with negative static lift (abs applied)"
    );
    println!(
        "TDH abs trap: tdh_neg={:.6}, tdh_pos={:.6}, equal={}",
        tdh_neg,
        tdh_pos,
        (tdh_neg - tdh_pos).abs() < 1e-6
    );
}

// ── T-5.4b.h: Solver trait default method returns error ──────────────────────

/// T-5.4b.h: PumpStationSolver::solve() (Solver trait) returns an error.
/// Use solve_pump_station() instead.
#[test]
fn pump_station_solver_trait_default_errors() {
    let constraints = DesignConstraints::default();
    let mut solver = PumpStationSolver::new(None, constraints);
    let params = SolverParams::default();
    let result = solver.solve(&params);
    assert!(
        result.is_err(),
        "Solver::solve() must return Err — use solve_pump_station()"
    );
}
