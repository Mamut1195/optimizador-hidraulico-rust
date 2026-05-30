//! T-5.2 — SewerSolver oracle-parity integration tests.
//!
//! Tests follow strict TDD order:
//!   RED (sewer.rs written before sewer.rs is fully implemented)
//!   → GREEN (SewerSolver implementation lands)
//!   → REFACTOR (optional cleanup)

use approx::assert_abs_diff_eq;
use hydro_solvers::{SewerSolver, Solver, SolverParams};
use hydro_terrain::TerrainModel;
use hydro_types::{
    DesignConstraints, NetworkNode, NetworkPipe, NodeId, NodeType, PipeId, PipeMaterial,
    PipeNetwork,
};
use oracle::solvers::{load_sewer_golden, SewerGolden};
use std::collections::HashMap;

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Reconstruct the terrain model from the fixture parameters.
fn make_fixture_terrain(f: &SewerGolden) -> TerrainModel {
    let points: Vec<[f64; 3]> = f.terrain.points.clone();
    let mut terrain = TerrainModel::from_xyz_list(&points).expect("terrain from fixture points");
    terrain.build_grid(f.terrain.grid_res).expect("build grid");
    terrain
}

/// Reconstruct the oracle-built PipeNetwork from fixture node/pipe data.
///
/// This network is used for the evaluate-only sub-fixture tests (T-5.2.a, T-5.2.c).
fn reconstruct_network(f: &SewerGolden) -> PipeNetwork {
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
            let roughness = p.roughness;
            let diameter = p.diameter;
            let slope = p.slope;
            let mut meta = HashMap::new();
            if p.bypassed_by_pump {
                meta.insert(
                    "bypassed_by_pump".to_string(),
                    serde_json::Value::Bool(true),
                );
            }
            NetworkPipe {
                id: PipeId::new(p.id.as_str()),
                start_node_id: NodeId::new(p.start_node_id.as_str()),
                end_node_id: NodeId::new(p.end_node_id.as_str()),
                length,
                effective_length: length,
                diameter,
                material: PipeMaterial::Pvc,
                roughness,
                start_invert: p.start_invert,
                end_invert: p.end_invert,
                slope,
                design_flow: p.design_flow,
                waypoints: vec![],
                metadata: meta,
            }
        })
        .collect();

    PipeNetwork::new("SewerTest", nodes, pipes)
}

/// Parse a node type string from the oracle fixture (e.g., "MANHOLE" → NodeType::Manhole).
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
        _ => panic!("Unknown node type in fixture: {}", s),
    }
}

// ── T-5.2.a: sewer_evaluate_exact_parity ─────────────────────────────────────

/// T-5.2.a: evaluate() on the oracle-reconstructed network returns the exact
/// oracle score (total_cost, total_length, total_excavation, norm_violations, pump_count).
///
/// RED: fails until SewerSolver::evaluate() is implemented.
#[test]
fn sewer_evaluate_exact_parity() {
    let f = load_sewer_golden();
    let terrain = make_fixture_terrain(&f);
    let constraints = DesignConstraints::default();
    let solver = SewerSolver::new(terrain, constraints);
    let network = reconstruct_network(&f);

    let score = solver
        .evaluate(&network)
        .expect("evaluate must not error on valid oracle network");

    let oracle = &f.evaluate_only;

    // EXACT tolerance ≤ 1e-6 (Phase 5 parity gate)
    assert_abs_diff_eq!(score.total_cost, oracle.total_cost, epsilon = 1e-6,);
    assert_abs_diff_eq!(score.total_length, oracle.total_length, epsilon = 1e-6,);
    assert_abs_diff_eq!(
        score.total_excavation,
        oracle.total_excavation,
        epsilon = 1e-6,
    );
    assert_eq!(
        score.norm_violations, oracle.norm_violations,
        "norm_violations must match oracle exactly"
    );
    assert_eq!(
        score.pump_count, oracle.pump_count,
        "pump_count must match oracle exactly"
    );
}

// ── T-5.2.c: sewer_score_formula_exact ───────────────────────────────────────

/// T-5.2.c: The cost formula is cost = 1.0*L + 2.0*excav + 100.0*violations.
/// Also verifies avg_excavation = total_excavation / max(total_length, 1.0).
///
/// RED: fails until SewerSolver::evaluate() is implemented.
#[test]
fn sewer_score_formula_exact() {
    let f = load_sewer_golden();
    let terrain = make_fixture_terrain(&f);
    let constraints = DesignConstraints::default();
    let solver = SewerSolver::new(terrain, constraints);
    let network = reconstruct_network(&f);

    let score = solver.evaluate(&network).expect("evaluate must not error");

    let oracle = &f.evaluate_only;

    // Cost formula: 1.0*L + 2.0*excav + 100.0*violations
    let expected_cost = 1.0 * oracle.total_length
        + 2.0 * oracle.total_excavation
        + 100.0 * oracle.norm_violations as f64;

    assert_abs_diff_eq!(score.total_cost, expected_cost, epsilon = 1e-6,);

    // avg_excavation formula: total_excavation / max(total_length, 1.0)
    let expected_avg_excav = oracle.total_excavation / oracle.total_length.max(1.0);

    // Extract details from score
    let details = &score.details;
    let avg_excav = details
        .get("avg_excavation")
        .and_then(|v| v.as_f64())
        .expect("details must contain avg_excavation");

    assert_abs_diff_eq!(avg_excav, expected_avg_excav, epsilon = 1e-4);
}

// ── T-5.2.b: sewer_network_topology_structural ────────────────────────────────

/// T-5.2.b: solve() on the fixture terrain produces a network with the expected
/// structural properties (node_count, pipe_count, node types, domain invariants).
///
/// RED: fails until SewerSolver::solve() is implemented.
#[test]
fn sewer_network_topology_structural() {
    let f = load_sewer_golden();
    let terrain = make_fixture_terrain(&f);
    let constraints = DesignConstraints::default();
    let mut solver = SewerSolver::new(terrain, constraints);

    let params = SolverParams {
        route_variant: f.solver_params.route_variant,
        slope_factor: f.solver_params.slope_factor,
        cover_factor: f.solver_params.cover_factor,
        diameter_offset: f.solver_params.diameter_offset,
        manhole_spacing: f.solver_params.manhole_spacing,
        num_alternatives: f.solver_params.num_alternatives,
        grid_resolution: f.solver_params.grid_resolution,
        design_flow: f.solver_params.flow_per_service,
        ..SolverParams::default()
    };

    // We need to pass service_points and outlet — they are in solver_params field
    // For this test, we inject them via a solve-direct call (see SewerSolverExt below)
    let outlet = (f.solver_params.outlet[0], f.solver_params.outlet[1]);
    let service_points: Vec<(f64, f64)> = f
        .solver_params
        .service_points
        .iter()
        .map(|sp| (sp[0], sp[1]))
        .collect();

    let solutions = solver
        .solve_sewer(
            &service_points,
            outlet,
            "SewerTest",
            f.solver_params.flow_per_service,
            PipeMaterial::Pvc,
            &params,
        )
        .expect("solve must succeed on valid fixture terrain");

    assert!(
        !solutions.is_empty(),
        "solve must return at least one solution"
    );

    let network = &solutions[0].network;

    // STRUCTURAL: node count within ±1 of oracle (Mehlhorn cross-language may route
    // through slightly different intermediate nodes; parity is network-level, not bit-exact)
    let node_diff = (network.node_count() as i64 - f.node_count as i64).abs();
    assert!(
        node_diff <= 1,
        "node_count {} differs from oracle {} by {} (> ±1 tolerance)",
        network.node_count(),
        f.node_count,
        node_diff
    );
    // STRUCTURAL: pipe count within ±1 of oracle
    let pipe_diff = (network.pipe_count() as i64 - f.pipe_count as i64).abs();
    assert!(
        pipe_diff <= 1,
        "pipe_count {} differs from oracle {} by {} (> ±1 tolerance)",
        network.pipe_count(),
        f.pipe_count,
        pipe_diff
    );

    // Domain invariant: for every non-pump pipe, end_invert <= start_invert + 1e-9
    for pipe in &network.pipes {
        let bypassed = pipe
            .metadata
            .get("bypassed_by_pump")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if !bypassed {
            assert!(
                pipe.end_invert <= pipe.start_invert + 1e-9,
                "gravity pipe {} violates invert continuity: start={} end={}",
                pipe.id,
                pipe.start_invert,
                pipe.end_invert
            );
        }
    }

    // Domain invariant: for every pipe-start node, z - start_invert >= min_cover - 1e-6
    let min_cover = DesignConstraints::default().min_cover;
    for pipe in &network.pipes {
        if let Some(start_node) = network.get_node(&pipe.start_node_id) {
            let cover_depth = start_node.z - pipe.start_invert;
            assert!(
                cover_depth >= min_cover - 1e-6,
                "pipe {} violates min_cover at start node {}: cover={:.4} min={:.4}",
                pipe.id,
                start_node.id,
                cover_depth,
                min_cover
            );
        }
    }
}

// ── T-5.2.e: proptest invert continuity + cover depth ────────────────────────

/// T-5.2.e (deterministic variant): on the fixture terrain with default params,
/// all gravity pipes have end_invert ≤ start_invert and all nodes have adequate cover.
///
/// Uses deterministic default params (no proptest; proptest added in a follow-on).
#[test]
fn sewer_invert_and_cover_domain_invariants() {
    let f = load_sewer_golden();
    let terrain = make_fixture_terrain(&f);
    let constraints = DesignConstraints::default();
    let mut solver = SewerSolver::new(terrain, constraints.clone());

    let outlet = (f.solver_params.outlet[0], f.solver_params.outlet[1]);
    let service_points: Vec<(f64, f64)> = f
        .solver_params
        .service_points
        .iter()
        .map(|sp| (sp[0], sp[1]))
        .collect();

    let params = SolverParams::default();
    let solutions = solver
        .solve_sewer(
            &service_points,
            outlet,
            "SewerTest",
            f.solver_params.flow_per_service,
            PipeMaterial::Pvc,
            &params,
        )
        .expect("solve must succeed");

    assert!(!solutions.is_empty());
    let network = &solutions[0].network;
    let min_cover = constraints.min_cover;

    for pipe in &network.pipes {
        let bypassed = pipe
            .metadata
            .get("bypassed_by_pump")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if !bypassed {
            // Gravity pipe: end_invert ≤ start_invert + ε
            assert!(
                pipe.end_invert <= pipe.start_invert + 1e-9,
                "gravity invert violation in pipe {}: end={} > start={}",
                pipe.id,
                pipe.end_invert,
                pipe.start_invert
            );
        }

        // Cover depth at start node
        if let Some(sn) = network.get_node(&pipe.start_node_id) {
            let cover = sn.z - pipe.start_invert;
            assert!(
                cover >= min_cover - 1e-6,
                "cover violation at start of pipe {}: cover={:.4} min={:.4}",
                pipe.id,
                cover,
                min_cover
            );
        }
    }
}
