//! T-5.3b — ConveyanceSolver oracle-parity integration tests.
//!
//! Three fixture families:
//!   1. Primary pipeline  (structure_count=0, route_variant=0, num_alternatives=1)
//!   2. Valve-placement   (structure_count=2, air_valve + blowoff_valve)
//!   3. Alternatives      (num_alternatives=2, 2 pairwise-distinct costs via Yen)
//!
//! Parity gates (matching conveyance.py oracle):
//!   - total_cost within 1e-4 (E2E solve)
//!   - evaluate() total_cost within 1e-6 (reconstruct + evaluate)
//!   - per-pipe diameter EXACT (same f64 value)
//!   - per-pipe start/end inverts within 1e-9
//!   - per-node pressure_mca within 1e-6
//!   - violations + structure_count INTEGER EXACT
//!   - node_count / pipe_count INTEGER EXACT

use approx::assert_abs_diff_eq;
use hydro_solvers::{ConveyanceSolver, Solver, SolverParams};
use hydro_terrain::TerrainModel;
use hydro_types::{
    DesignConstraints, NetworkNode, NetworkPipe, NodeId, NodeType, PipeId, PipeMaterial,
    PipeNetwork,
};
use oracle::solvers::{
    load_conveyance_alternatives, load_conveyance_golden, load_conveyance_valves, ConvGolden,
};
use std::collections::HashMap;

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Reconstruct terrain from fixture point list.
fn make_conv_terrain(points: &[[f64; 3]], grid_res: f64) -> TerrainModel {
    let mut terrain =
        TerrainModel::from_xyz_list(points).expect("terrain from conveyance fixture points");
    terrain
        .build_grid(grid_res)
        .expect("build conveyance fixture grid");
    terrain
}

/// Reconstruct oracle-built PipeNetwork from fixture node/pipe data.
fn reconstruct_conv_network(f: &ConvGolden) -> PipeNetwork {
    let nodes: Vec<NetworkNode> = f
        .nodes
        .iter()
        .map(|n| {
            let node_type = parse_node_type(&n.node_type);
            let mut node = NetworkNode::new(n.id.as_str(), n.x, n.y, n.z, node_type);
            node.rim_elevation = n.rim_elevation;
            node.sump_elevation = n.sump_elevation;
            // Restore pressure_mca in metadata
            if let Some(p) = n.pressure_mca {
                node.metadata
                    .insert("pressure_mca".to_string(), serde_json::Value::from(p));
            }
            node
        })
        .collect();

    let pipes: Vec<NetworkPipe> = f
        .pipes
        .iter()
        .map(|p| {
            let length = p.length;
            let mut meta = HashMap::new();
            // ConveyanceSolver uses "design_flow" as the metadata key (not "flow_m3s")
            meta.insert(
                "design_flow".to_string(),
                serde_json::Value::from(p.design_flow),
            );
            NetworkPipe {
                id: PipeId::new(p.id.as_str()),
                start_node_id: NodeId::new(p.start_node_id.as_str()),
                end_node_id: NodeId::new(p.end_node_id.as_str()),
                length,
                effective_length: length,
                diameter: p.diameter,
                material: PipeMaterial::Pvc,
                roughness: 0.009,
                start_invert: p.start_invert,
                end_invert: p.end_invert,
                slope: p.slope,
                design_flow: p.design_flow,
                waypoints: vec![],
                metadata: meta,
            }
        })
        .collect();

    PipeNetwork::new("ConveyanceReconstructed", nodes, pipes)
}

fn parse_node_type(s: &str) -> NodeType {
    match s {
        "MANHOLE" => NodeType::Manhole,
        "JUNCTION" => NodeType::Junction,
        "OUTLET" => NodeType::Outlet,
        "INLET" => NodeType::Inlet,
        "PUMP" => NodeType::Pump,
        "VALVE" => NodeType::Valve,
        "HYDRANT" => NodeType::Hydrant,
        "TANK" => NodeType::Tank,
        "RESERVOIR" => NodeType::Reservoir,
        "SERVICE" => NodeType::Service,
        _ => panic!("Unknown node type in conveyance fixture: {}", s),
    }
}

// ── T-5.3b.a: evaluate formula exact (primary fixture) ────────────────────────

/// T-5.3b.a: evaluate() on the oracle-reconstructed primary network returns the
/// exact oracle score. Formula: 1.5*L + 2.0*excav + 200*violations + 500*structures.
#[test]
fn conv_evaluate_formula_exact() {
    let f = load_conveyance_golden();
    let terrain = make_conv_terrain(&f.terrain.points, f.terrain.grid_res);
    let constraints = DesignConstraints::default();
    let solver = ConveyanceSolver::new(terrain, constraints);
    let network = reconstruct_conv_network(&f);

    let score = solver
        .evaluate(&network)
        .expect("evaluate must not error on valid oracle conveyance network");

    let oracle = &f.evaluate_only;

    assert_abs_diff_eq!(score.total_cost, oracle.total_cost, epsilon = 1e-6);
    assert_abs_diff_eq!(score.total_length, oracle.total_length, epsilon = 1e-6);
    assert_abs_diff_eq!(
        score.total_excavation,
        oracle.total_excavation,
        epsilon = 1e-6
    );
    assert_eq!(
        score.norm_violations, oracle.norm_violations,
        "norm_violations must match oracle exactly"
    );
    assert_eq!(score.pump_count, 0, "conv pump_count must always be 0");
}

// ── T-5.3b.b: cost formula constants ─────────────────────────────────────────

/// T-5.3b.b: Cost formula uses w_length=1.5, w_excavation=2.0, w_violations=200.0,
/// w_structures=500.0. Verifies the fixture cost_formula weights match the implementation.
#[test]
fn conv_cost_formula_constants() {
    let f = load_conveyance_golden();
    assert_abs_diff_eq!(f.cost_formula.w_length, 1.5, epsilon = 1e-12);
    assert_abs_diff_eq!(f.cost_formula.w_excavation, 2.0, epsilon = 1e-12);
    assert_abs_diff_eq!(f.cost_formula.w_violations, 200.0, epsilon = 1e-12);
    assert_abs_diff_eq!(f.cost_formula.w_structures, 500.0, epsilon = 1e-12);

    let eo = &f.evaluate_only;
    let structure_count = eo.details.structure_count as f64;
    let computed_cost = f.cost_formula.w_length * eo.total_length
        + f.cost_formula.w_excavation * eo.total_excavation
        + f.cost_formula.w_violations * eo.norm_violations as f64
        + f.cost_formula.w_structures * structure_count;

    assert_abs_diff_eq!(computed_cost, f.cost_formula.expected_cost, epsilon = 1e-4);
}

// ── T-5.3b.c: pump_count == 0 invariant ──────────────────────────────────────

/// T-5.3b.c: evaluate() always returns pump_count==0 for conveyance networks.
#[test]
fn conv_pump_count_is_always_zero() {
    let f = load_conveyance_golden();
    let terrain = make_conv_terrain(&f.terrain.points, f.terrain.grid_res);
    let constraints = DesignConstraints::default();
    let solver = ConveyanceSolver::new(terrain, constraints);
    let network = reconstruct_conv_network(&f);

    let score = solver.evaluate(&network).expect("evaluate");
    assert_eq!(
        score.pump_count, 0,
        "conveyance pump_count must always be 0"
    );
    assert_eq!(f.evaluate_only.pump_count, 0, "oracle pump_count must be 0");
}

// ── T-5.3b.d: primary E2E parity ─────────────────────────────────────────────

/// T-5.3b.d: solve_conveyance() on the fixture terrain produces a network with
/// exact structural parity (node_count, pipe_count, total_length) and cost
/// within 1e-4 of oracle.
#[test]
fn conv_solve_primary_end_to_end_parity() {
    let f = load_conveyance_golden();
    let terrain = make_conv_terrain(&f.terrain.points, f.terrain.grid_res);
    let constraints = DesignConstraints::default();
    let mut solver = ConveyanceSolver::new(terrain, constraints);

    let params = SolverParams {
        source_head: f.solver_params.source_head,
        design_flow: f.solver_params.design_flow,
        num_alternatives: f.solver_params.num_alternatives,
        grid_resolution: f.solver_params.grid_resolution,
        route_variant: f.solver_params.route_variant,
        cover_factor: f.solver_params.cover_factor,
        valve_spacing: f.solver_params.valve_spacing,
        ..SolverParams::default()
    };

    let source = (f.solver_params.source[0], f.solver_params.source[1]);
    let destination = (
        f.solver_params.destination[0],
        f.solver_params.destination[1],
    );

    let solutions = solver
        .solve_conveyance(
            source,
            destination,
            f.solver_params.design_flow,
            f.solver_params.source_head,
            "ConveyanceTest",
            PipeMaterial::Pvc,
            &params,
        )
        .expect("solve_conveyance must succeed on fixture terrain");

    assert!(
        !solutions.is_empty(),
        "solve_conveyance must return at least one solution"
    );
    let network = &solutions[0].network;

    // ── Structural EXACT parity ───────────────────────────────────────────────
    assert_eq!(
        network.node_count(),
        f.node_count,
        "node_count {} does not match oracle {} (EXACT gate)",
        network.node_count(),
        f.node_count
    );
    assert_eq!(
        network.pipe_count(),
        f.pipe_count,
        "pipe_count {} does not match oracle {} (EXACT gate)",
        network.pipe_count(),
        f.pipe_count
    );
    assert_abs_diff_eq!(network.total_length, f.total_length, epsilon = 1e-4);

    // ── Cost parity ───────────────────────────────────────────────────────────
    let score = solver
        .evaluate(network)
        .expect("evaluate must not error on solver-built conveyance network");

    let oracle_cost = f.evaluate_only.total_cost;
    let abs_diff = (score.total_cost - oracle_cost).abs();

    println!(
        "Conv E2E parity: Rust_cost={:.6}, oracle_cost={:.6}, |diff|={:.2e}",
        score.total_cost, oracle_cost, abs_diff
    );

    assert!(
        abs_diff < 1e-4,
        "total_cost parity FAILED: Rust={:.6}, oracle={:.6}, |diff|={:.2e} >= 1e-4",
        score.total_cost,
        oracle_cost,
        abs_diff
    );

    assert_eq!(
        score.pump_count, 0,
        "conveyance solve must return pump_count == 0"
    );
}

// ── T-5.3b.e: structure_count == 0 on primary fixture ────────────────────────

/// T-5.3b.e: Primary fixture (monotonic terrain) produces structure_count == 0.
/// Monotonically increasing z → no interior high/low points → no air/blowoff valves.
#[test]
fn conv_primary_structure_count_is_zero() {
    let f = load_conveyance_golden();
    let terrain = make_conv_terrain(&f.terrain.points, f.terrain.grid_res);
    let constraints = DesignConstraints::default();
    let mut solver = ConveyanceSolver::new(terrain, constraints);

    let params = SolverParams {
        source_head: f.solver_params.source_head,
        design_flow: f.solver_params.design_flow,
        num_alternatives: f.solver_params.num_alternatives,
        grid_resolution: f.solver_params.grid_resolution,
        route_variant: f.solver_params.route_variant,
        cover_factor: f.solver_params.cover_factor,
        valve_spacing: f.solver_params.valve_spacing,
        ..SolverParams::default()
    };

    let source = (f.solver_params.source[0], f.solver_params.source[1]);
    let destination = (
        f.solver_params.destination[0],
        f.solver_params.destination[1],
    );

    let solutions = solver
        .solve_conveyance(
            source,
            destination,
            f.solver_params.design_flow,
            f.solver_params.source_head,
            "ConveyanceTest",
            PipeMaterial::Pvc,
            &params,
        )
        .expect("solve_conveyance must succeed");

    let network = &solutions[0].network;

    // Count VALVE nodes in the network (structure_count)
    let valve_count = network
        .nodes
        .iter()
        .filter(|n| n.node_type == NodeType::Valve)
        .count();

    let oracle_structure_count = f.evaluate_only.details.structure_count as usize;

    assert_eq!(
        valve_count, oracle_structure_count,
        "primary fixture must have structure_count=={} (monotonic terrain, no high/low points), got {}",
        oracle_structure_count, valve_count
    );
}

// ── T-5.3b.f: per-pipe parity (diameter + inverts) ───────────────────────────

/// T-5.3b.f: Per-pipe diameter is EXACT (same f64), inverts within 1e-9.
/// ConveyanceSolver selects ONE diameter for the whole pipeline.
#[test]
fn conv_per_pipe_diameter_and_inverts() {
    let f = load_conveyance_golden();
    let terrain = make_conv_terrain(&f.terrain.points, f.terrain.grid_res);
    let constraints = DesignConstraints::default();
    let mut solver = ConveyanceSolver::new(terrain, constraints);

    let params = SolverParams {
        source_head: f.solver_params.source_head,
        design_flow: f.solver_params.design_flow,
        num_alternatives: f.solver_params.num_alternatives,
        grid_resolution: f.solver_params.grid_resolution,
        route_variant: f.solver_params.route_variant,
        cover_factor: f.solver_params.cover_factor,
        valve_spacing: f.solver_params.valve_spacing,
        ..SolverParams::default()
    };

    let source = (f.solver_params.source[0], f.solver_params.source[1]);
    let destination = (
        f.solver_params.destination[0],
        f.solver_params.destination[1],
    );

    let solutions = solver
        .solve_conveyance(
            source,
            destination,
            f.solver_params.design_flow,
            f.solver_params.source_head,
            "ConveyanceTest",
            PipeMaterial::Pvc,
            &params,
        )
        .expect("solve_conveyance must succeed");

    let network = &solutions[0].network;

    // Build oracle pipe map by ID
    let oracle_pipes: std::collections::HashMap<String, &oracle::solvers::ConvPipeGolden> =
        f.pipes.iter().map(|p| (p.id.clone(), p)).collect();

    for pipe in &network.pipes {
        let pid = pipe.id.0.clone();
        if let Some(oracle_pipe) = oracle_pipes.get(&pid) {
            // Diameter: EXACT equality (same algorithm, same diameter selection)
            assert_eq!(
                pipe.diameter, oracle_pipe.diameter,
                "pipe {} diameter mismatch: Rust={} oracle={}",
                pid, pipe.diameter, oracle_pipe.diameter
            );
            // Inverts: within 1e-9
            assert_abs_diff_eq!(pipe.start_invert, oracle_pipe.start_invert, epsilon = 1e-9);
            assert_abs_diff_eq!(pipe.end_invert, oracle_pipe.end_invert, epsilon = 1e-9);
        }
    }
}

// ── T-5.3b.g: per-node pressure parity ──────────────────────────────────────

/// T-5.3b.g: pressure_mca at each node (from inline propagation in _build_pipeline)
/// matches oracle within 1e-6. Source node has pressure == source_head.
#[test]
fn conv_per_node_pressure_parity() {
    let f = load_conveyance_golden();
    let terrain = make_conv_terrain(&f.terrain.points, f.terrain.grid_res);
    let constraints = DesignConstraints::default();
    let mut solver = ConveyanceSolver::new(terrain, constraints);

    let params = SolverParams {
        source_head: f.solver_params.source_head,
        design_flow: f.solver_params.design_flow,
        num_alternatives: f.solver_params.num_alternatives,
        grid_resolution: f.solver_params.grid_resolution,
        route_variant: f.solver_params.route_variant,
        cover_factor: f.solver_params.cover_factor,
        valve_spacing: f.solver_params.valve_spacing,
        ..SolverParams::default()
    };

    let source = (f.solver_params.source[0], f.solver_params.source[1]);
    let destination = (
        f.solver_params.destination[0],
        f.solver_params.destination[1],
    );

    let solutions = solver
        .solve_conveyance(
            source,
            destination,
            f.solver_params.design_flow,
            f.solver_params.source_head,
            "ConveyanceTest",
            PipeMaterial::Pvc,
            &params,
        )
        .expect("solve_conveyance must succeed");

    let network = &solutions[0].network;

    // Build oracle node map by ID
    let oracle_nodes: std::collections::HashMap<String, &oracle::solvers::ConvNodeGolden> =
        f.nodes.iter().map(|n| (n.id.clone(), n)).collect();

    for node in &network.nodes {
        let nid = node.id.0.clone();
        let rust_pressure = node.metadata.get("pressure_mca").and_then(|v| v.as_f64());

        if let Some(oracle_node) = oracle_nodes.get(&nid) {
            if let (Some(rp), Some(op)) = (rust_pressure, oracle_node.pressure_mca) {
                assert!(
                    (rp - op).abs() < 1e-6,
                    "pressure_mca at node {} mismatch: Rust={:.8} oracle={:.8}",
                    nid,
                    rp,
                    op
                );
            }
        }
    }
}

// ── T-5.3b.h: domain invariants ──────────────────────────────────────────────

/// T-5.3b.h: Solved conveyance network satisfies domain invariants:
///   - Exactly one TANK node (the source).
///   - Exactly one RESERVOIR node (the destination).
///   - All pipes have positive length.
///   - All nodes have pressure_mca in metadata (propagated inline).
#[test]
fn conv_domain_invariants() {
    let f = load_conveyance_golden();
    let terrain = make_conv_terrain(&f.terrain.points, f.terrain.grid_res);
    let constraints = DesignConstraints::default();
    let mut solver = ConveyanceSolver::new(terrain, constraints);

    let params = SolverParams {
        source_head: f.solver_params.source_head,
        design_flow: f.solver_params.design_flow,
        num_alternatives: f.solver_params.num_alternatives,
        grid_resolution: f.solver_params.grid_resolution,
        route_variant: f.solver_params.route_variant,
        cover_factor: f.solver_params.cover_factor,
        valve_spacing: f.solver_params.valve_spacing,
        ..SolverParams::default()
    };

    let source = (f.solver_params.source[0], f.solver_params.source[1]);
    let destination = (
        f.solver_params.destination[0],
        f.solver_params.destination[1],
    );

    let solutions = solver
        .solve_conveyance(
            source,
            destination,
            f.solver_params.design_flow,
            f.solver_params.source_head,
            "ConveyanceTest",
            PipeMaterial::Pvc,
            &params,
        )
        .expect("solve_conveyance must succeed");

    let network = &solutions[0].network;

    // Exactly one TANK node
    let tank_count = network
        .nodes
        .iter()
        .filter(|n| n.node_type == NodeType::Tank)
        .count();
    assert_eq!(tank_count, 1, "must have exactly one TANK node (source)");

    // Exactly one RESERVOIR node
    let reservoir_count = network
        .nodes
        .iter()
        .filter(|n| n.node_type == NodeType::Reservoir)
        .count();
    assert_eq!(
        reservoir_count, 1,
        "must have exactly one RESERVOIR node (destination)"
    );

    // All pipes have positive length
    for pipe in &network.pipes {
        assert!(
            pipe.length > 0.0,
            "pipe {} must have positive length",
            pipe.id
        );
    }

    // All nodes have pressure_mca set (source gets it first, then BFS-inline propagation)
    for node in &network.nodes {
        assert!(
            node.metadata.contains_key("pressure_mca"),
            "node {} must have pressure_mca set",
            node.id
        );
    }
}

// ── T-5.3b.i: valve-placement fixture — structure_count > 0 ──────────────────

/// T-5.3b.i: Valve fixture with deliberate peak/valley terrain produces
/// structure_count > 0, with VALVE node_types containing air_valve and blowoff_valve.
#[test]
fn conv_valve_placement_structure_count() {
    let f = load_conveyance_valves();
    let terrain = make_conv_terrain(&f.terrain.points, f.terrain.grid_res);
    let constraints = DesignConstraints::default();
    let mut solver = ConveyanceSolver::new(terrain, constraints);

    let params = SolverParams {
        source_head: f.solver_params.source_head,
        design_flow: f.solver_params.design_flow,
        num_alternatives: f.solver_params.num_alternatives,
        grid_resolution: f.solver_params.grid_resolution,
        route_variant: f.solver_params.route_variant,
        cover_factor: f.solver_params.cover_factor,
        valve_spacing: f.solver_params.valve_spacing,
        ..SolverParams::default()
    };

    let source = (f.solver_params.source[0], f.solver_params.source[1]);
    let destination = (
        f.solver_params.destination[0],
        f.solver_params.destination[1],
    );

    let solutions = solver
        .solve_conveyance(
            source,
            destination,
            f.solver_params.design_flow,
            f.solver_params.source_head,
            "ConveyanceValves",
            PipeMaterial::Pvc,
            &params,
        )
        .expect("solve_conveyance (valve fixture) must succeed");

    assert!(!solutions.is_empty(), "must return at least one solution");
    let network = &solutions[0].network;

    // structure_count must match oracle
    let valve_count = network
        .nodes
        .iter()
        .filter(|n| n.node_type == NodeType::Valve)
        .count();

    let oracle_structure_count = f.structure_count;
    assert!(
        oracle_structure_count > 0,
        "oracle fixture must have structure_count > 0 (non-vacuous)"
    );
    assert_eq!(
        valve_count, oracle_structure_count,
        "structure_count mismatch: Rust={} oracle={}",
        valve_count, oracle_structure_count
    );

    // Verify VALVE nodes have correct valve_type metadata
    for oracle_vn in &f.valve_nodes {
        let rust_node = network.nodes.iter().find(|n| n.id.0 == oracle_vn.id);

        assert!(
            rust_node.is_some(),
            "valve node {} not found in Rust network",
            oracle_vn.id
        );

        let rust_node = rust_node.unwrap();
        assert_eq!(
            rust_node.node_type,
            NodeType::Valve,
            "node {} should be VALVE type",
            oracle_vn.id
        );

        let rust_valve_type = rust_node
            .metadata
            .get("valve_type")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        assert_eq!(
            rust_valve_type,
            oracle_vn.valve_type.as_deref().unwrap_or(""),
            "node {} valve_type mismatch: Rust='{}' oracle='{}'",
            oracle_vn.id,
            rust_valve_type,
            oracle_vn.valve_type.as_deref().unwrap_or("")
        );
    }

    println!(
        "Valve fixture: structure_count={}, valve_types={:?}",
        valve_count,
        f.valve_nodes
            .iter()
            .map(|n| n.valve_type.as_deref().unwrap_or("none"))
            .collect::<Vec<_>>()
    );
}

// ── T-5.3b.j: valve fixture evaluate parity ──────────────────────────────────

/// T-5.3b.j: evaluate() on the oracle-reconstructed valve network returns the
/// exact oracle score including structure_count contribution.
#[test]
fn conv_valve_evaluate_parity() {
    let f = load_conveyance_valves();
    let terrain = make_conv_terrain(&f.terrain.points, f.terrain.grid_res);
    let constraints = DesignConstraints::default();
    let solver = ConveyanceSolver::new(terrain, constraints);
    let network = reconstruct_conv_network_valves(&f);

    let score = solver
        .evaluate(&network)
        .expect("evaluate must not error on valve oracle network");

    let oracle = &f.evaluate_only;
    assert_abs_diff_eq!(score.total_cost, oracle.total_cost, epsilon = 1e-6);
    assert_abs_diff_eq!(score.total_length, oracle.total_length, epsilon = 1e-6);
    assert_eq!(
        score.norm_violations, oracle.norm_violations,
        "valve fixture norm_violations must match oracle"
    );
    assert_eq!(score.pump_count, 0);
}

// ── T-5.3b.k: alternatives fixture — Yen integration ─────────────────────────

/// T-5.3b.k: With num_alternatives=2, solve_conveyance() returns exactly 2 solutions
/// with pairwise-distinct costs matching the oracle, sorted ascending.
#[test]
fn conv_alternatives_yen_integration() {
    let f = load_conveyance_alternatives();

    let terrain = make_conv_terrain(&f.terrain.points, f.terrain.grid_res);
    let constraints = DesignConstraints::default();
    let mut solver = ConveyanceSolver::new(terrain, constraints);

    let params = SolverParams {
        source_head: f.solver_params.source_head,
        design_flow: f.solver_params.design_flow,
        num_alternatives: f.solver_params.num_alternatives,
        grid_resolution: f.solver_params.grid_resolution,
        route_variant: f.solver_params.route_variant,
        cover_factor: f.solver_params.cover_factor,
        valve_spacing: f.solver_params.valve_spacing,
        ..SolverParams::default()
    };

    let source = (f.solver_params.source[0], f.solver_params.source[1]);
    let destination = (
        f.solver_params.destination[0],
        f.solver_params.destination[1],
    );

    let solutions = solver
        .solve_conveyance(
            source,
            destination,
            f.solver_params.design_flow,
            f.solver_params.source_head,
            "ConveyanceAlt",
            PipeMaterial::Pvc,
            &params,
        )
        .expect("solve_conveyance (alternatives) must succeed");

    // Count must match oracle
    assert_eq!(
        solutions.len(),
        f.num_solutions_returned,
        "solution count {} does not match oracle {}",
        solutions.len(),
        f.num_solutions_returned
    );

    // Sorted by total_cost ascending
    for i in 0..solutions.len().saturating_sub(1) {
        assert!(
            solutions[i].score.total_cost <= solutions[i + 1].score.total_cost + 1e-9,
            "solutions must be sorted by cost ascending: [{}]={:.6} > [{}]={:.6}",
            i,
            solutions[i].score.total_cost,
            i + 1,
            solutions[i + 1].score.total_cost
        );
    }

    // Each solution cost matches oracle EXACTLY (within 1e-4)
    for (i, (sol, oracle_sol)) in solutions.iter().zip(f.solutions.iter()).enumerate() {
        let abs_diff = (sol.score.total_cost - oracle_sol.total_cost).abs();
        assert!(
            abs_diff < 1e-4,
            "alternatives[{}] cost parity FAILED: Rust={:.6} oracle={:.6} |diff|={:.2e}",
            i,
            sol.score.total_cost,
            oracle_sol.total_cost,
            abs_diff
        );
    }

    // Pairwise-distinct costs (no ties — fixture guarantees this)
    let costs: Vec<f64> = solutions.iter().map(|s| s.score.total_cost).collect();
    for i in 0..costs.len() {
        for j in (i + 1)..costs.len() {
            assert!(
                (costs[i] - costs[j]).abs() >= 1e-3,
                "alternatives costs[{}]={:.6} and costs[{}]={:.6} must be strictly distinct",
                i,
                costs[i],
                j,
                costs[j]
            );
        }
    }

    // All alternatives have pump_count == 0
    for sol in &solutions {
        assert_eq!(
            sol.score.pump_count, 0,
            "all alternatives must have pump_count==0"
        );
    }

    println!(
        "Conv alternatives Yen: {} solutions, costs={:?}",
        solutions.len(),
        costs
    );
}

// ── Valve fixture reconstruction helper ───────────────────────────────────────

fn reconstruct_conv_network_valves(f: &oracle::solvers::ConvValvesGolden) -> PipeNetwork {
    let nodes: Vec<NetworkNode> = f
        .nodes
        .iter()
        .map(|n| {
            let node_type = parse_node_type(&n.node_type);
            let mut node = NetworkNode::new(n.id.as_str(), n.x, n.y, n.z, node_type);
            node.rim_elevation = n.rim_elevation;
            node.sump_elevation = n.sump_elevation;
            if let Some(p) = n.pressure_mca {
                node.metadata
                    .insert("pressure_mca".to_string(), serde_json::Value::from(p));
            }
            if let Some(ref vt) = n.valve_type {
                node.metadata.insert(
                    "valve_type".to_string(),
                    serde_json::Value::String(vt.clone()),
                );
            }
            node
        })
        .collect();

    let pipes: Vec<NetworkPipe> = f
        .pipes
        .iter()
        .map(|p| {
            let length = p.length;
            let mut meta = HashMap::new();
            meta.insert(
                "design_flow".to_string(),
                serde_json::Value::from(p.design_flow),
            );
            NetworkPipe {
                id: PipeId::new(p.id.as_str()),
                start_node_id: NodeId::new(p.start_node_id.as_str()),
                end_node_id: NodeId::new(p.end_node_id.as_str()),
                length,
                effective_length: length,
                diameter: p.diameter,
                material: PipeMaterial::Pvc,
                roughness: 0.009,
                start_invert: p.start_invert,
                end_invert: p.end_invert,
                slope: p.slope,
                design_flow: p.design_flow,
                waypoints: vec![],
                metadata: meta,
            }
        })
        .collect();

    PipeNetwork::new("ConveyanceValvesReconstructed", nodes, pipes)
}
