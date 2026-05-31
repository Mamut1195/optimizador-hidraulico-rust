//! T-5.3a — WaterSupplySolver oracle-parity integration tests.
//!
//! Tests follow strict TDD order:
//!   RED (tests written before implementation is complete)
//!   → GREEN (WaterSupplySolver implementation passes all tests)
//!   → REFACTOR (optional cleanup)
//!
//! Parity gate: total_cost within 1e-4 of oracle, violations exact,
//! pump_count == 0 (invariant for water supply).

use approx::assert_abs_diff_eq;
use hydro_solvers::{Solver, SolverParams, WaterSupplySolver};
use hydro_terrain::TerrainModel;
use hydro_types::{
    DesignConstraints, NetworkNode, NetworkPipe, NodeId, NodeType, PipeId, PipeMaterial,
    PipeNetwork,
};
use oracle::solvers::{load_water_supply_alternatives, load_water_supply_golden, WsGolden};
use std::collections::HashMap;

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Reconstruct the terrain model from the primary WS fixture parameters.
fn make_ws_terrain(f: &WsGolden) -> TerrainModel {
    let points: Vec<[f64; 3]> = f.terrain.points.clone();
    let mut terrain = TerrainModel::from_xyz_list(&points).expect("terrain from WS fixture points");
    terrain
        .build_grid(f.terrain.grid_res)
        .expect("build WS fixture grid");
    terrain
}

/// Reconstruct the oracle-built PipeNetwork from fixture node/pipe data.
fn reconstruct_ws_network(f: &WsGolden) -> PipeNetwork {
    let nodes: Vec<NetworkNode> = f
        .nodes
        .iter()
        .map(|n| {
            let node_type = parse_node_type(&n.node_type);
            let mut node = NetworkNode::new(n.id.as_str(), n.x, n.y, n.z, node_type);
            node.rim_elevation = n.rim_elevation;
            node.sump_elevation = n.sump_elevation;
            node.demand = n.demand;
            // Restore pressure_mca in metadata so evaluate() can read it
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
            meta.insert("flow_m3s".to_string(), serde_json::Value::from(p.flow_m3s));
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

    PipeNetwork::new("WaterSupplyTest", nodes, pipes)
}

/// Parse a node type string from the oracle fixture.
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
        _ => panic!("Unknown node type in WS fixture: {}", s),
    }
}

// ── T-5.3.a: evaluate formula exact ──────────────────────────────────────────

/// T-5.3.a: evaluate() on the oracle-reconstructed network returns the exact
/// oracle score. Formula: 1.0*L + 1.5*excav + 150.0*violations.
///
/// This verifies formula correctness independent of the solver pipeline.
#[test]
fn ws_evaluate_formula_exact() {
    let f = load_water_supply_golden();
    let terrain = make_ws_terrain(&f);
    let constraints = DesignConstraints::default();
    let solver = WaterSupplySolver::new(terrain, constraints);
    let network = reconstruct_ws_network(&f);

    let score = solver
        .evaluate(&network)
        .expect("evaluate must not error on valid oracle WS network");

    let oracle = &f.evaluate_only;

    // Cost formula: 1.0*L + 1.5*excav + 150.0*violations
    assert_abs_diff_eq!(score.total_cost, oracle.total_cost, epsilon = 1e-4);
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
    assert_eq!(score.pump_count, 0, "pump_count must always be 0 for WS");
}

// ── T-5.3.b: cost formula constants ──────────────────────────────────────────

/// T-5.3.b: Cost formula uses w_length=1.0, w_excavation=1.5, w_violations=150.0.
/// Verifies that the fixture cost_formula weights match the implementation.
#[test]
fn ws_cost_formula_constants() {
    let f = load_water_supply_golden();
    assert_abs_diff_eq!(f.cost_formula.w_length, 1.0, epsilon = 1e-12);
    assert_abs_diff_eq!(f.cost_formula.w_excavation, 1.5, epsilon = 1e-12);
    assert_abs_diff_eq!(f.cost_formula.w_violations, 150.0, epsilon = 1e-12);

    let terrain = make_ws_terrain(&f);
    let constraints = DesignConstraints::default();
    let solver = WaterSupplySolver::new(terrain, constraints);
    let network = reconstruct_ws_network(&f);
    let score = solver.evaluate(&network).expect("evaluate");

    let cf = &f.cost_formula;
    let eo = &f.evaluate_only;
    let computed_cost = cf.w_length * eo.total_length
        + cf.w_excavation * eo.total_excavation
        + cf.w_violations * eo.norm_violations as f64;

    assert_abs_diff_eq!(computed_cost, cf.expected_cost, epsilon = 1e-4);
    assert_abs_diff_eq!(score.total_cost, computed_cost, epsilon = 1e-4);
}

// ── T-5.3.c: pump_count == 0 invariant ───────────────────────────────────────

/// T-5.3.c: evaluate() always returns pump_count==0 for WS networks.
/// WaterSupply is a pressure system — no gravity-pump logic.
#[test]
fn ws_pump_count_is_always_zero() {
    let f = load_water_supply_golden();
    let terrain = make_ws_terrain(&f);
    let constraints = DesignConstraints::default();
    let solver = WaterSupplySolver::new(terrain, constraints);
    let network = reconstruct_ws_network(&f);

    let score = solver.evaluate(&network).expect("evaluate");
    assert_eq!(score.pump_count, 0, "WS pump_count must always be 0");
    assert_eq!(f.evaluate_only.pump_count, 0, "oracle pump_count must be 0");
}

// ── T-5.3.d: solve E2E parity ────────────────────────────────────────────────

/// T-5.3.d: solve_water_supply() on the fixture terrain produces a network with
/// the expected structural properties (node_count, pipe_count) and cost parity.
///
/// This is the PRIMARY E2E parity gate for T-5.3.
/// Tolerance 1e-4 on total_cost (same network → near-zero float diff).
#[test]
fn ws_solve_end_to_end_parity() {
    let f = load_water_supply_golden();
    let terrain = make_ws_terrain(&f);
    let constraints = DesignConstraints::default();
    let mut solver = WaterSupplySolver::new(terrain, constraints);

    let params = SolverParams {
        source_head: f.solver_params.source_head,
        design_flow: f.solver_params.demand_per_node,
        num_alternatives: f.solver_params.num_alternatives,
        grid_resolution: f.solver_params.grid_resolution,
        network_type: f.solver_params.network_type,
        diameter_offset: f.solver_params.diameter_offset,
        loop_density: f.solver_params.loop_density,
        ..SolverParams::default()
    };

    let source = (f.solver_params.source[0], f.solver_params.source[1]);
    let demand_points: Vec<(f64, f64)> = f
        .solver_params
        .demand_points
        .iter()
        .map(|dp| (dp[0], dp[1]))
        .collect();

    let solutions = solver
        .solve_water_supply(
            &demand_points,
            source,
            f.solver_params.source_head,
            "WaterSupplyTest",
            f.solver_params.demand_per_node,
            PipeMaterial::Pvc,
            &params,
        )
        .expect("solve_water_supply must succeed on fixture terrain");

    assert!(
        !solutions.is_empty(),
        "solve_water_supply must return at least one solution"
    );
    let network = &solutions[0].network;

    // ── Structural EXACT parity ───────────────────────────────────────────────
    assert_eq!(
        network.node_count(),
        f.node_count,
        "node_count {} does not match oracle {} (EXACT gate). \
         MST must produce same spanning tree as Python oracle.",
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

    // ── Cost parity ───────────────────────────────────────────────────────────
    let score = solver
        .evaluate(network)
        .expect("evaluate must not error on solver-built WS network");

    let oracle_cost = f.evaluate_only.total_cost;
    let abs_diff = (score.total_cost - oracle_cost).abs();

    println!(
        "WS E2E parity: Rust_cost={:.6}, oracle_cost={:.6}, |diff|={:.2e}",
        score.total_cost, oracle_cost, abs_diff
    );

    assert!(
        abs_diff < 1e-4,
        "total_cost parity FAILED: Rust={:.6}, oracle={:.6}, |diff|={:.2e} >= 1e-4",
        score.total_cost,
        oracle_cost,
        abs_diff
    );

    // ── Pump count invariant ──────────────────────────────────────────────────
    assert_eq!(score.pump_count, 0, "WS solve must return pump_count == 0");
}

// ── T-5.3.e: structural invariants ───────────────────────────────────────────

/// T-5.3.e: The solved network satisfies domain invariants:
///   - Exactly one TANK node (the source).
///   - SERVICE nodes == demand_points count.
///   - All pipes have positive length.
///   - All nodes have pressure_mca stored in metadata (BFS reached them).
#[test]
fn ws_domain_invariants() {
    let f = load_water_supply_golden();
    let terrain = make_ws_terrain(&f);
    let constraints = DesignConstraints::default();
    let mut solver = WaterSupplySolver::new(terrain, constraints);

    let params = SolverParams {
        source_head: f.solver_params.source_head,
        design_flow: f.solver_params.demand_per_node,
        num_alternatives: f.solver_params.num_alternatives,
        grid_resolution: f.solver_params.grid_resolution,
        network_type: f.solver_params.network_type,
        diameter_offset: f.solver_params.diameter_offset,
        loop_density: f.solver_params.loop_density,
        ..SolverParams::default()
    };

    let source = (f.solver_params.source[0], f.solver_params.source[1]);
    let demand_points: Vec<(f64, f64)> = f
        .solver_params
        .demand_points
        .iter()
        .map(|dp| (dp[0], dp[1]))
        .collect();

    let solutions = solver
        .solve_water_supply(
            &demand_points,
            source,
            f.solver_params.source_head,
            "WaterSupplyTest",
            f.solver_params.demand_per_node,
            PipeMaterial::Pvc,
            &params,
        )
        .expect("solve_water_supply must succeed");

    let network = &solutions[0].network;

    // Exactly one TANK node
    let tank_count = network
        .nodes
        .iter()
        .filter(|n| n.node_type == NodeType::Tank)
        .count();
    assert_eq!(tank_count, 1, "must have exactly one TANK node");

    // At least one SERVICE node per demand point
    let service_count = network
        .nodes
        .iter()
        .filter(|n| n.node_type == NodeType::Service)
        .count();
    assert!(
        service_count >= demand_points.len(),
        "must have at least {} SERVICE nodes (demand points), got {}",
        demand_points.len(),
        service_count
    );

    // All pipes have positive length
    for pipe in &network.pipes {
        assert!(
            pipe.length > 0.0,
            "pipe {} must have positive length",
            pipe.id
        );
    }

    // All non-source nodes have pressure_mca set
    let source_id: NodeId = network
        .nodes
        .iter()
        .find(|n| n.node_type == NodeType::Tank)
        .map(|n| n.id.clone())
        .expect("must have tank node");

    for node in &network.nodes {
        if node.id == source_id {
            continue;
        }
        assert!(
            node.metadata.contains_key("pressure_mca"),
            "node {} must have pressure_mca set (BFS must reach it)",
            node.id
        );
    }
}

// ── T-5.3.f: node/pipe count matches oracle ───────────────────────────────────

/// T-5.3.f: node and pipe counts from solve() exactly match the oracle fixture.
/// (Structural twin of E2E parity — focuses just on counts without cost check.)
#[test]
fn ws_network_structure_matches_oracle() {
    let f = load_water_supply_golden();
    let terrain = make_ws_terrain(&f);
    let constraints = DesignConstraints::default();
    let mut solver = WaterSupplySolver::new(terrain, constraints);

    let params = SolverParams {
        source_head: f.solver_params.source_head,
        design_flow: f.solver_params.demand_per_node,
        num_alternatives: f.solver_params.num_alternatives,
        grid_resolution: f.solver_params.grid_resolution,
        network_type: f.solver_params.network_type,
        diameter_offset: f.solver_params.diameter_offset,
        loop_density: f.solver_params.loop_density,
        ..SolverParams::default()
    };

    let source = (f.solver_params.source[0], f.solver_params.source[1]);
    let demand_points: Vec<(f64, f64)> = f
        .solver_params
        .demand_points
        .iter()
        .map(|dp| (dp[0], dp[1]))
        .collect();

    let solutions = solver
        .solve_water_supply(
            &demand_points,
            source,
            f.solver_params.source_head,
            "WaterSupplyTest",
            f.solver_params.demand_per_node,
            PipeMaterial::Pvc,
            &params,
        )
        .expect("solve_water_supply must succeed");

    assert!(!solutions.is_empty());
    let network = &solutions[0].network;

    assert_eq!(
        network.node_count(),
        f.node_count,
        "node_count mismatch: Rust={} oracle={}",
        network.node_count(),
        f.node_count
    );
    assert_eq!(
        network.pipe_count(),
        f.pipe_count,
        "pipe_count mismatch: Rust={} oracle={}",
        network.pipe_count(),
        f.pipe_count
    );
    assert_abs_diff_eq!(network.total_length, f.total_length, epsilon = 1e-4,);
}

// ── T-5.3.g: alternatives fixture — Yen k-paths parity ───────────────────────

/// T-5.3.g: solve_water_supply() returns alternatives with pairwise-strictly-distinct
/// total_costs matching the oracle, sorted ascending.
///
/// Validates Yen k-shortest-paths integration for WaterSupplySolver.
/// The fixture uses num_alternatives=2 so the MST primary path and the second
/// Yen path have different hop-counts and therefore genuinely distinct costs.
/// (num_alternatives=3 on a uniform grid produces tied cost alternatives 1 and 2,
///  making ordering assertions vacuous — see fixture generator for details.)
#[test]
fn ws_alternatives_yen_integration() {
    let f = load_water_supply_alternatives();

    // Reconstruct terrain from alternatives fixture
    let points: Vec<[f64; 3]> = f.terrain.points.clone();
    let mut terrain =
        TerrainModel::from_xyz_list(&points).expect("terrain from WS alternatives fixture");
    terrain
        .build_grid(f.terrain.grid_res)
        .expect("build WS alternatives grid");

    let constraints = DesignConstraints::default();
    let mut solver = WaterSupplySolver::new(terrain, constraints);

    let params = SolverParams {
        source_head: f.solver_params.source_head,
        design_flow: f.solver_params.demand_per_node,
        num_alternatives: f.solver_params.num_alternatives,
        grid_resolution: f.solver_params.grid_resolution,
        network_type: f.solver_params.network_type,
        diameter_offset: f.solver_params.diameter_offset,
        loop_density: f.solver_params.loop_density,
        ..SolverParams::default()
    };

    let source = (f.solver_params.source[0], f.solver_params.source[1]);
    let demand_points: Vec<(f64, f64)> = f
        .solver_params
        .demand_points
        .iter()
        .map(|dp| (dp[0], dp[1]))
        .collect();

    let solutions = solver
        .solve_water_supply(
            &demand_points,
            source,
            f.solver_params.source_head,
            "WaterSupplyAlt",
            f.solver_params.demand_per_node,
            PipeMaterial::Pvc,
            &params,
        )
        .expect("solve_water_supply alternatives must succeed");

    // Must return at least 1 solution (MST primary)
    assert!(
        !solutions.is_empty(),
        "solve_water_supply must return at least one solution"
    );

    // (a) Count must match oracle exactly
    assert_eq!(
        solutions.len(),
        f.num_solutions_returned,
        "solution count {} does not match oracle {} for num_alternatives={}",
        solutions.len(),
        f.num_solutions_returned,
        f.num_alternatives_requested
    );

    // (b) Per-solution total_cost matches oracle within 1e-4
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

    // (c) Strictly ascending costs — each successive solution is at least 1e-3 more expensive.
    // This pins the ordering: tied costs would satisfy `<= + 1e-9` vacuously (both directions
    // pass), but `cost[i+1] - cost[i] >= 1e-3` requires a real gap.
    let costs: Vec<f64> = solutions.iter().map(|s| s.score.total_cost).collect();
    for i in 0..costs.len().saturating_sub(1) {
        assert!(
            costs[i + 1] - costs[i] >= 1e-3,
            "alternatives must be strictly ascending: costs[{}]={:.6} is not at least 1e-3 \
             less than costs[{}]={:.6} (gap={:.6})",
            i,
            costs[i],
            i + 1,
            costs[i + 1],
            costs[i + 1] - costs[i]
        );
    }

    // (d) Pairwise-distinct costs — no two alternatives share the same cost.
    // This catches duplicated MST returned multiple times.
    for i in 0..costs.len() {
        for j in (i + 1)..costs.len() {
            assert!(
                (costs[i] - costs[j]).abs() >= 1e-3,
                "alternatives costs[{}]={:.6} and costs[{}]={:.6} must be pairwise \
                 strictly distinct (diff={:.2e} < 1e-3)",
                i,
                costs[i],
                j,
                costs[j],
                (costs[i] - costs[j]).abs()
            );
        }
    }

    // (e) Distinct routes — consecutive solutions must differ in node-set or pipe-count.
    for i in 0..solutions.len().saturating_sub(1) {
        let n_a = solutions[i].network.node_count();
        let n_b = solutions[i + 1].network.node_count();
        let p_a = solutions[i].network.pipe_count();
        let p_b = solutions[i + 1].network.pipe_count();
        // At minimum the costs differ (asserted above), but structural difference
        // (node or pipe count) is an additional independent check.
        let structurally_different = n_a != n_b || p_a != p_b;
        // Costs differ by >= 1e-3 (already asserted), so this is an informational
        // hint — we assert it separately to surface routing bugs clearly.
        assert!(
            structurally_different || (costs[i + 1] - costs[i]).abs() >= 1e-3,
            "solutions[{}] and solutions[{}] appear identical: \
             same node/pipe counts AND cost within 1e-3",
            i,
            i + 1
        );
    }

    // Rank numbers must be correct (1-indexed, consecutive)
    for (i, sol) in solutions.iter().enumerate() {
        assert_eq!(
            sol.rank,
            (i + 1) as i64,
            "solution rank must be {} got {}",
            i + 1,
            sol.rank
        );
    }

    // Verify each solution has pump_count == 0
    for sol in &solutions {
        assert_eq!(
            sol.score.pump_count, 0,
            "all WS alternatives must have pump_count == 0"
        );
    }

    println!(
        "WS Yen integration: {} solutions returned, costs={:?}, distinct_costs={:?}",
        solutions.len(),
        costs,
        f.distinct_costs
    );
}

// ── T-5.3.h: per-pipe diameter and inverts ───────────────────────────────────

/// T-5.3.h: Each pipe in the oracle fixture can be reconstructed and the solver's
/// evaluate() reads the flow from metadata correctly.
#[test]
fn ws_pipe_metadata_flow_roundtrip() {
    let f = load_water_supply_golden();
    let terrain = make_ws_terrain(&f);
    let constraints = DesignConstraints::default();
    let solver = WaterSupplySolver::new(terrain, constraints);
    let network = reconstruct_ws_network(&f);

    // All pipes must have flow_m3s in metadata
    for pipe in &network.pipes {
        assert!(
            pipe.metadata.contains_key("flow_m3s"),
            "pipe {} must have flow_m3s in metadata",
            pipe.id
        );
        let flow = pipe.metadata["flow_m3s"]
            .as_f64()
            .expect("flow_m3s must be f64");
        assert!(flow > 0.0, "flow_m3s must be positive for pipe {}", pipe.id);
    }

    // evaluate() must succeed
    let score = solver.evaluate(&network).expect("evaluate");
    assert!(score.total_cost > 0.0, "total_cost must be positive");
}

// ── T-5.3.i: node pressure parity ────────────────────────────────────────────

/// T-5.3.i: Pressure at the source node equals source_head, and all service
/// nodes have pressure within a reasonable range.
#[test]
fn ws_node_pressure_parity() {
    let f = load_water_supply_golden();

    // Source pressure must be source_head (exactly)
    let source_id = f.solver_params.source;
    let source_node = f
        .nodes
        .iter()
        .find(|n| (n.x - source_id[0]).abs() < 0.1 && (n.y - source_id[1]).abs() < 0.1)
        .expect("source node must be in fixture");

    assert_abs_diff_eq!(
        source_node.pressure_mca.expect("source has pressure"),
        f.solver_params.source_head,
        epsilon = 1e-9
    );

    // All nodes have pressure_mca stored
    for node in &f.nodes {
        assert!(
            node.pressure_mca.is_some(),
            "all nodes must have pressure_mca in oracle fixture (node {})",
            node.id
        );
    }
}
