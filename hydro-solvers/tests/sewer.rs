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

// ── T-5.2.a: sewer_evaluate_formula_exact ────────────────────────────────────
//
// Formerly named sewer_evaluate_exact_parity. Renamed because the test verifies
// that evaluate() computes the correct formula on the ORACLE-reconstructed network —
// it does NOT test solve() and is NOT end-to-end parity. See sewer_solve_end_to_end_parity
// below for the genuine end-to-end test.

/// T-5.2.a: evaluate() on the oracle-reconstructed network returns the exact
/// oracle score (total_cost, total_length, total_excavation, norm_violations, pump_count).
///
/// This verifies formula correctness independent of the solver pipeline.
/// It is NOT a substitute for end-to-end solve() parity — see sewer_solve_end_to_end_parity.
#[test]
fn sewer_evaluate_formula_exact() {
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

    // STRUCTURAL EXACT parity: the Steiner tie-break fix (CRITICAL-1) ensures Rust
    // builds the same 7-node tree as the oracle. No ±1 tolerance is needed or
    // acceptable — this is the primary structural gate for T-5.2.
    assert_eq!(
        network.node_count(),
        f.node_count,
        "node_count {} does not match oracle {} (EXACT gate — ±1 tolerance removed)",
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

// ── T-5.2.f: sewer_solve_end_to_end_parity ───────────────────────────────────

/// T-5.2.f (RED phase): solve() builds the SAME network as the oracle and evaluate()
/// returns the oracle's exact cost.
///
/// This is the genuine end-to-end parity test. It:
///   1. Calls Rust's `solve_sewer()` (the real solver entry) on fixture inputs.
///   2. Evaluates the Rust-built network.
///   3. Asserts that the Rust solver reproduced the oracle's exact network structure:
///      - node_count == oracle node_count (EXACT, no ±1 tolerance)
///      - pipe_count == oracle pipe_count (EXACT, no ±1 tolerance)
///   4. Asserts total_cost parity within 1e-4 (justified: same network structure →
///      same pipe lengths and elevations → cost differs only by float rounding).
///
/// RED: fails until CRITICAL-1 (Steiner tie-break) is fixed so that Rust builds
/// the same 7-node Steiner tree as the oracle.
///
/// GATE: This test MUST pass for PR-6 to merge. It is the primary parity gate for T-5.2.
#[test]
fn sewer_solve_end_to_end_parity() {
    let f = load_sewer_golden();
    let terrain = make_fixture_terrain(&f);
    let constraints = DesignConstraints::default();
    let mut solver = SewerSolver::new(terrain, constraints.clone());

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
        .expect("solve_sewer must succeed on fixture terrain");

    assert!(
        !solutions.is_empty(),
        "solve_sewer must return at least one solution"
    );
    let network = &solutions[0].network;

    // ── Structural EXACT parity (no ±1 tolerance) ─────────────────────────
    assert_eq!(
        network.node_count(),
        f.node_count,
        "node_count {} does not match oracle {} (EXACT gate — no ±1 tolerance). \
         The Rust Steiner tree must reproduce the oracle's exact 7-node tree. \
         Fix CRITICAL-1 (deterministic Steiner tie-break) before this can pass.",
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

    // ── Cost parity ────────────────────────────────────────────────────────
    // The Rust solver builds the same network as the oracle → evaluate() must
    // return the same total_cost. Tolerance 1e-4 accounts for float accumulation
    // over 6 pipes; if the networks are truly identical, |diff| will be < 1e-9.
    let score = solver
        .evaluate(network)
        .expect("evaluate must not error on solver-built network");

    let oracle_cost = f.evaluate_only.total_cost;
    let abs_diff = (score.total_cost - oracle_cost).abs();

    println!(
        "E2E parity: Rust_cost={:.6}, oracle_cost={:.6}, |diff|={:.2e}",
        score.total_cost, oracle_cost, abs_diff
    );

    assert!(
        abs_diff < 1e-4,
        "total_cost parity FAILED: Rust={:.6}, oracle={:.6}, |diff|={:.2e} >= 1e-4. \
         If the Steiner tree is now identical, this difference should be < 1e-9. \
         Tolerance 1e-4 is the outer bound; aim for < 1e-9.",
        score.total_cost,
        oracle_cost,
        abs_diff
    );

    // ── Node identity check ────────────────────────────────────────────────
    // Verify the Rust network contains the same node IDs as the oracle.
    let oracle_node_ids: std::collections::HashSet<String> =
        f.nodes.iter().map(|n| n.id.clone()).collect();
    let rust_node_ids: std::collections::HashSet<String> =
        network.nodes.iter().map(|n| n.id.to_string()).collect();
    let missing: Vec<_> = oracle_node_ids.difference(&rust_node_ids).collect();
    let extra: Vec<_> = rust_node_ids.difference(&oracle_node_ids).collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "Node ID mismatch: missing_from_rust={:?}, extra_in_rust={:?}. \
         Oracle nodes: {:?}. Rust nodes: {:?}.",
        missing,
        extra,
        {
            let mut v: Vec<_> = oracle_node_ids.iter().collect();
            v.sort();
            v
        },
        {
            let mut v: Vec<_> = rust_node_ids.iter().collect();
            v.sort();
            v
        }
    );
}

// ── T-6.5.B: sewer_solve_via_trait_roundtrip ─────────────────────────────────

/// T-6.5.B (RED → GREEN via PR-B): `Solver::solve` delegates to `solve_sewer`
/// and returns a feasible solution for the golden fixture.
///
/// Scenario 1 — happy path: construct with fixture service_points/outlet/material/
/// flow_per_service/network_name, call `Solver::solve`, assert `Ok(solutions)` with
/// `len >= 1` and `solutions[0].score.total_cost > 0.0`.
///
/// Scenario 2 — empty service_points: construct with `service_points = vec![]`,
/// call `Solver::solve`, assert `Err(_)` — NOT `Ok(vec![])`.
#[test]
fn sewer_solve_via_trait_roundtrip() {
    let f = load_sewer_golden();

    // ── Scenario 1: happy path via trait ─────────────────────────────────────
    {
        let terrain = make_fixture_terrain(&f);
        let constraints = DesignConstraints::default();
        let service_points: Vec<(f64, f64)> = f
            .solver_params
            .service_points
            .iter()
            .map(|sp| (sp[0], sp[1]))
            .collect();
        let outlet = (f.solver_params.outlet[0], f.solver_params.outlet[1]);

        let mut solver = SewerSolver::new(
            terrain,
            constraints,
            service_points,
            outlet,
            f.solver_params.flow_per_service,
            PipeMaterial::Concrete,
            "test_sewer".to_string(),
        );

        let params = SolverParams {
            grid_resolution: f.solver_params.grid_resolution,
            route_variant: f.solver_params.route_variant,
            slope_factor: f.solver_params.slope_factor,
            cover_factor: f.solver_params.cover_factor,
            diameter_offset: f.solver_params.diameter_offset,
            manhole_spacing: f.solver_params.manhole_spacing,
            num_alternatives: f.solver_params.num_alternatives,
            design_flow: f.solver_params.flow_per_service,
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

    // ── Scenario 2: empty service_points must yield Err ──────────────────────
    {
        let f2 = load_sewer_golden();
        let terrain = make_fixture_terrain(&f2);
        let constraints = DesignConstraints::default();
        let outlet = (f2.solver_params.outlet[0], f2.solver_params.outlet[1]);

        let mut solver = SewerSolver::new(
            terrain,
            constraints,
            vec![], // empty service_points — must trigger Err
            outlet,
            f2.solver_params.flow_per_service,
            PipeMaterial::Concrete,
            "test_sewer_empty".to_string(),
        );

        let params = SolverParams::default();

        let result = Solver::solve(&mut solver, &params);
        assert!(
            result.is_err(),
            "Solver::solve with empty service_points must return Err, got Ok"
        );
    }
}
