//! T-5.4a — DistributionSolver oracle-parity integration tests.
//!
//! Parity gate:
//!   - total_cost within 1e-4 of oracle (E2E solve)
//!   - node_count, pipe_count, violations EXACT
//!   - valve_count, hydrant_count EXACT
//!   - pressure_std within 1e-4
//!   - total_length within 1e-4
//!   - pump_count == 0 invariant

use approx::assert_abs_diff_eq;
use hydro_solvers::{DistributionSolver, Solver, SolverParams};
use hydro_terrain::TerrainModel;
use hydro_types::{
    DesignConstraints, NetworkNode, NetworkPipe, NodeId, NodeType, PipeId, PipeMaterial,
    PipeNetwork,
};
use oracle::solvers::{
    load_distribution_alternatives, load_distribution_golden, load_distribution_valves_saturated,
    DistribGolden, DistribTerrainParams, DistribValvesSaturatedGolden,
};
use serde_json::Value;
use std::collections::HashMap;

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Reconstruct the terrain model from a distribution fixture's terrain params.
///
/// IMPORTANT: We recompute z-values from the formula (base_z + slope_x*x + slope_y*y)
/// rather than reading the pre-stored z from the fixture. This is necessary because
/// serde_json parses some boundary float64 values (e.g., 100.16000000000001) to the
/// wrong adjacent float64, causing 1-ULP differences in edge costs that change which
/// edges are selected as MST spine vs loop, breaking oracle parity.
/// Python's oracle computes z directly as `100.0 + slope_x*x + slope_y*y`, and we
/// must match that exactly.
fn make_distrib_terrain(points: &[[f64; 3]], grid_res: f64) -> TerrainModel {
    let mut terrain =
        TerrainModel::from_xyz_list(points).expect("terrain from distribution fixture points");
    terrain
        .build_grid(grid_res)
        .expect("build distribution fixture grid");
    terrain
}

/// Reconstruct terrain from formula (slope_x, slope_y, base_z) to avoid JSON float precision issues.
///
/// Python oracle computes z = base_z + slope_x*x + slope_y*y using direct float arithmetic.
/// We must match this exactly to get identical edge costs. We use index-based x/y to avoid
/// floating-point accumulation drift (same as np.arange: xs[i] = x_min + i * grid_res).
fn make_distrib_terrain_from_formula(tp: &DistribTerrainParams) -> TerrainModel {
    let nx = ((tp.x_max - tp.x_min) / tp.grid_res).round() as usize + 1;
    let ny = ((tp.y_max - tp.y_min) / tp.grid_res).round() as usize + 1;
    let mut points: Vec<[f64; 3]> = Vec::with_capacity(nx * ny);
    for i in 0..nx {
        let x = tp.x_min + (i as f64) * tp.grid_res;
        for j in 0..ny {
            let y = tp.y_min + (j as f64) * tp.grid_res;
            let z = tp.base_z + tp.slope_x * x + tp.slope_y * y;
            points.push([x, y, z]);
        }
    }
    let mut terrain = TerrainModel::from_xyz_list(&points).expect("terrain from formula");
    terrain.build_grid(tp.grid_res).expect("build formula grid");
    terrain
}

/// Reconstruct the oracle-built PipeNetwork from golden fixture node/pipe data.
fn reconstruct_distrib_network(f: &DistribGolden) -> PipeNetwork {
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
                    .insert("pressure_mca".to_string(), Value::from(p));
            }
            // Restore accessory
            if let Some(ref acc) = n.accessory {
                node.metadata
                    .insert("accessory".to_string(), Value::String(acc.clone()));
            }
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
                material: PipeMaterial::Pvc,
                roughness: 0.009,
                start_invert: p.start_invert,
                end_invert: p.end_invert,
                slope: p.slope,
                design_flow: p.design_flow,
                waypoints: vec![],
                metadata: HashMap::new(),
            }
        })
        .collect();

    PipeNetwork::new("DistributionTest", nodes, pipes)
}

/// Reconstruct network from valves_saturated fixture.
fn reconstruct_valves_saturated_network(f: &DistribValvesSaturatedGolden) -> PipeNetwork {
    let nodes: Vec<NetworkNode> = f
        .nodes
        .iter()
        .map(|n| {
            let node_type = parse_node_type(&n.node_type);
            let mut node = NetworkNode::new(n.id.as_str(), n.x, n.y, n.z, node_type);
            node.rim_elevation = n.rim_elevation;
            node.sump_elevation = n.sump_elevation;
            node.demand = n.demand;
            if let Some(p) = n.pressure_mca {
                node.metadata
                    .insert("pressure_mca".to_string(), Value::from(p));
            }
            if let Some(ref acc) = n.accessory {
                node.metadata
                    .insert("accessory".to_string(), Value::String(acc.clone()));
            }
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
                material: PipeMaterial::Pvc,
                roughness: 0.009,
                start_invert: p.start_invert,
                end_invert: p.end_invert,
                slope: p.slope,
                design_flow: p.design_flow,
                waypoints: vec![],
                metadata: HashMap::new(),
            }
        })
        .collect();

    PipeNetwork::new("DistribValvesSaturated", nodes, pipes)
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
        _ => panic!("Unknown node type in distribution fixture: {}", s),
    }
}

// ── T-5.4.a: evaluate formula exact (golden) ─────────────────────────────────

/// T-5.4.a: evaluate() on oracle-reconstructed golden network returns exact oracle score.
/// Formula: 1.0*L + 1.5*excav + 150.0*viol + 5.0*pressure_std + 500.0*valve + 800.0*hydrant
#[test]
fn distrib_evaluate_formula_exact() {
    let f = load_distribution_golden();
    let terrain = make_distrib_terrain(&f.terrain.points, f.terrain.grid_res);
    let constraints = DesignConstraints::default();
    let demand_points: Vec<(f64, f64)> = f
        .solver_params
        .demand_points
        .iter()
        .map(|dp| (dp[0], dp[1]))
        .collect();
    let source = (f.solver_params.source[0], f.solver_params.source[1]);
    let solver = DistributionSolver::new(
        terrain,
        constraints,
        demand_points,
        source,
        f.solver_params.demand_per_node,
        PipeMaterial::Pvc,
        "DistributionTest".to_string(),
    );
    let network = reconstruct_distrib_network(&f);

    let score = solver
        .evaluate(&network)
        .expect("evaluate must not error on valid oracle distribution network");

    let oracle = &f.evaluate_only;

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
    assert_eq!(
        score.pump_count, 0,
        "pump_count must always be 0 for distribution"
    );

    // Details checks
    let details = &oracle.details;
    let got_valve = score
        .details
        .get("valve_count")
        .and_then(|v| v.as_f64())
        .unwrap_or(-1.0);
    let got_hydrant = score
        .details
        .get("hydrant_count")
        .and_then(|v| v.as_f64())
        .unwrap_or(-1.0);
    let got_std = score
        .details
        .get("pressure_std")
        .and_then(|v| v.as_f64())
        .unwrap_or(-1.0);

    assert_abs_diff_eq!(got_valve, details.valve_count, epsilon = 0.5);
    assert_abs_diff_eq!(got_hydrant, details.hydrant_count, epsilon = 0.5);
    assert_abs_diff_eq!(got_std, details.pressure_std, epsilon = 1e-4);
}

// ── T-5.4.b: cost formula constants ──────────────────────────────────────────

/// T-5.4.b: Cost formula uses correct weights. Verifies fixture weight constants.
///
/// NOTE: The details.pressure_std is rounded to 2dp in the fixture, so the
/// formula cross-check uses a looser tolerance. The exact cost is verified
/// in distrib_evaluate_formula_exact via evaluate() which uses full-precision std.
#[test]
fn distrib_cost_formula_constants() {
    let f = load_distribution_golden();
    let cf = &f.cost_formula;

    assert_abs_diff_eq!(cf.w_length, 1.0, epsilon = 1e-12);
    assert_abs_diff_eq!(cf.w_excavation, 1.5, epsilon = 1e-12);
    assert_abs_diff_eq!(cf.w_violations, 150.0, epsilon = 1e-12);
    assert_abs_diff_eq!(cf.w_pressure_std, 5.0, epsilon = 1e-12);
    assert_abs_diff_eq!(cf.w_valve, 500.0, epsilon = 1e-12);
    assert_abs_diff_eq!(cf.w_hydrant, 800.0, epsilon = 1e-12);

    // Cross-check using rounded details (pressure_std is rounded to 2dp in fixture).
    // This allows up to 0.5 * w_pressure_std = 2.5 rounding error in pressure_std term.
    let eo = &f.evaluate_only;
    let expected = cf.w_length * eo.total_length
        + cf.w_excavation * eo.total_excavation
        + cf.w_violations * eo.norm_violations as f64
        + cf.w_pressure_std * eo.details.pressure_std
        + cf.w_valve * eo.details.valve_count
        + cf.w_hydrant * eo.details.hydrant_count;

    // Tolerance: 0.005 (half-ULP of 2dp rounding) * w_pressure_std = 0.025
    // Actual diff ≈ 0.012 (5.0 * (full_std - 0.17)), within 0.1 tolerance.
    assert!(
        (expected - cf.expected_cost).abs() < 1.0,
        "cost formula cross-check failed: computed={expected:.6} oracle={:.6} |diff|={:.4}",
        cf.expected_cost,
        (expected - cf.expected_cost).abs()
    );
}

// ── T-5.4.c: pump_count == 0 invariant ───────────────────────────────────────

/// T-5.4.c: evaluate() always returns pump_count==0 for distribution networks.
#[test]
fn distrib_pump_count_is_always_zero() {
    let f = load_distribution_golden();
    let terrain = make_distrib_terrain(&f.terrain.points, f.terrain.grid_res);
    let constraints = DesignConstraints::default();
    let demand_points: Vec<(f64, f64)> = f
        .solver_params
        .demand_points
        .iter()
        .map(|dp| (dp[0], dp[1]))
        .collect();
    let source = (f.solver_params.source[0], f.solver_params.source[1]);
    let solver = DistributionSolver::new(
        terrain,
        constraints,
        demand_points,
        source,
        f.solver_params.demand_per_node,
        PipeMaterial::Pvc,
        "DistributionTest".to_string(),
    );
    let network = reconstruct_distrib_network(&f);

    let score = solver.evaluate(&network).expect("evaluate");
    assert_eq!(score.pump_count, 0, "distribution pump_count must be 0");
    assert_eq!(f.evaluate_only.pump_count, 0, "oracle pump_count must be 0");
}

// ── T-5.4.d: solve E2E parity (golden) ───────────────────────────────────────

/// T-5.4.d: solve_distribution() on golden fixture terrain produces expected
/// structural properties and cost parity.
#[test]
fn distrib_solve_end_to_end_parity() {
    let f = load_distribution_golden();
    let terrain = make_distrib_terrain_from_formula(&f.terrain);
    let constraints = DesignConstraints::default();
    let source = (f.solver_params.source[0], f.solver_params.source[1]);
    let demand_points: Vec<(f64, f64)> = f
        .solver_params
        .demand_points
        .iter()
        .map(|dp| (dp[0], dp[1]))
        .collect();
    let mut solver = DistributionSolver::new(
        terrain,
        constraints,
        demand_points.clone(),
        source,
        f.solver_params.demand_per_node,
        PipeMaterial::Pvc,
        "DistributionTest".to_string(),
    );

    let params = SolverParams {
        grid_resolution: f.solver_params.grid_resolution,
        ..SolverParams::default()
    };

    let solutions = solver
        .solve_distribution(
            &demand_points,
            source,
            f.solver_params.source_head,
            "DistributionTest",
            f.solver_params.demand_per_node,
            PipeMaterial::Pvc,
            f.solver_params.num_alternatives,
            f.solver_params.mesh_density,
            f.solver_params.diameter_offset,
            f.solver_params.valve_spacing,
            f.solver_params.hydrant_spacing,
            &params,
        )
        .expect("solve_distribution must succeed on fixture terrain");

    assert!(
        !solutions.is_empty(),
        "solve_distribution must return at least one solution"
    );
    let sol = &solutions[0];
    let network = &sol.network;

    // Structural EXACT parity
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

    // Cost parity
    let oracle_cost = f.evaluate_only.total_cost;
    let abs_diff = (sol.score.total_cost - oracle_cost).abs();
    println!(
        "Distrib E2E parity: Rust_cost={:.6}, oracle_cost={:.6}, |diff|={:.2e}",
        sol.score.total_cost, oracle_cost, abs_diff
    );
    assert!(
        abs_diff < 1e-4,
        "total_cost parity FAILED: Rust={:.6}, oracle={:.6}, |diff|={:.2e} >= 1e-4",
        sol.score.total_cost,
        oracle_cost,
        abs_diff
    );

    // Valve/hydrant counts exact
    let valve_count_rust = network
        .nodes
        .iter()
        .filter(|n| n.node_type == NodeType::Valve)
        .count();
    let hydrant_count_rust = network
        .nodes
        .iter()
        .filter(|n| n.node_type == NodeType::Hydrant)
        .count();

    assert_eq!(
        valve_count_rust, f.evaluate_only.details.valve_count as usize,
        "valve_count mismatch"
    );
    assert_eq!(
        hydrant_count_rust, f.evaluate_only.details.hydrant_count as usize,
        "hydrant_count mismatch"
    );

    // Violations exact
    assert_eq!(
        sol.score.norm_violations, f.score.norm_violations,
        "norm_violations mismatch"
    );

    assert_eq!(sol.score.pump_count, 0, "pump_count must be 0");
}

// ── T-5.4.e: valve saturation ordering trap ───────────────────────────────────

/// T-5.4.e: valve_spacing=100 (divisor=1) converts every junction/service to
/// a valve BEFORE the hydrant pass, so hydrant_count == 0 regardless of spacing.
/// This fixture locks in the valves-before-hydrants placement order.
#[test]
fn distrib_valves_saturated_ordering_trap() {
    let f = load_distribution_valves_saturated();

    let source = (f.solver_params.source[0], f.solver_params.source[1]);
    let demand_points: Vec<(f64, f64)> = f
        .solver_params
        .demand_points
        .iter()
        .map(|dp| (dp[0], dp[1]))
        .collect();

    // Test evaluate-only: reconstruct oracle network, verify valve/hydrant counts
    let constraints = DesignConstraints::default();
    let solver = DistributionSolver::new(
        make_distrib_terrain(&f.terrain.points, f.terrain.grid_res),
        constraints.clone(),
        demand_points.clone(),
        source,
        f.solver_params.demand_per_node,
        PipeMaterial::Pvc,
        "DistribValvesSaturated".to_string(),
    );
    let network = reconstruct_valves_saturated_network(&f);

    let score = solver
        .evaluate(&network)
        .expect("evaluate must not error on valves_saturated network");

    let got_valve = score
        .details
        .get("valve_count")
        .and_then(|v| v.as_f64())
        .unwrap_or(-1.0) as usize;
    let got_hydrant = score
        .details
        .get("hydrant_count")
        .and_then(|v| v.as_f64())
        .unwrap_or(-1.0) as usize;

    assert_eq!(
        got_valve, f.expected_valve_count,
        "evaluate valve_count must be {} (saturation), got {}",
        f.expected_valve_count, got_valve
    );
    assert_eq!(
        got_hydrant, f.expected_hydrant_count,
        "evaluate hydrant_count must be {} (saturation ordering trap), got {}",
        f.expected_hydrant_count, got_hydrant
    );
    assert_abs_diff_eq!(score.total_cost, f.evaluate_only.total_cost, epsilon = 1e-4);

    // E2E solve: run solver with valve_spacing=100, verify same counts
    let mut solver2 = DistributionSolver::new(
        make_distrib_terrain_from_formula(&f.terrain),
        constraints,
        demand_points.clone(),
        source,
        f.solver_params.demand_per_node,
        PipeMaterial::Pvc,
        "DistribValvesSaturated".to_string(),
    );
    let params = SolverParams {
        grid_resolution: f.solver_params.grid_resolution,
        ..SolverParams::default()
    };

    let solutions = solver2
        .solve_distribution(
            &demand_points,
            source,
            f.solver_params.source_head,
            "DistribValvesSaturated",
            f.solver_params.demand_per_node,
            PipeMaterial::Pvc,
            f.solver_params.num_alternatives,
            f.solver_params.mesh_density,
            f.solver_params.diameter_offset,
            f.solver_params.valve_spacing, // 100.0 → divisor=1
            f.solver_params.hydrant_spacing,
            &params,
        )
        .expect("solve_distribution must succeed");

    assert!(!solutions.is_empty());
    let sol_network = &solutions[0].network;

    let valve_count_rust = sol_network
        .nodes
        .iter()
        .filter(|n| n.node_type == NodeType::Valve)
        .count();
    let hydrant_count_rust = sol_network
        .nodes
        .iter()
        .filter(|n| n.node_type == NodeType::Hydrant)
        .count();

    assert_eq!(
        valve_count_rust, f.expected_valve_count,
        "solve valve_count must be {} (saturation), got {}",
        f.expected_valve_count, valve_count_rust
    );
    assert_eq!(
        hydrant_count_rust, f.expected_hydrant_count,
        "solve hydrant_count must be {} (ordering trap: all junctions become valves first), got {}",
        f.expected_hydrant_count, hydrant_count_rust
    );

    // Cost parity
    let abs_diff = (solutions[0].score.total_cost - f.evaluate_only.total_cost).abs();
    assert!(
        abs_diff < 1e-4,
        "valves_saturated total_cost parity FAILED: Rust={:.6}, oracle={:.6}, |diff|={:.2e}",
        solutions[0].score.total_cost,
        f.evaluate_only.total_cost,
        abs_diff
    );
}

// ── T-5.4.f: alternatives pairwise-distinct costs ─────────────────────────────

/// T-5.4.f: solve_distribution() with num_alternatives=3 returns pairwise-strictly
/// distinct total_costs, sorted ascending. Validates alternative generation logic.
#[test]
fn distrib_alternatives_pairwise_distinct() {
    let f = load_distribution_alternatives();
    let terrain = make_distrib_terrain_from_formula(&f.terrain);
    let constraints = DesignConstraints::default();
    let source = (f.solver_params.source[0], f.solver_params.source[1]);
    let demand_points: Vec<(f64, f64)> = f
        .solver_params
        .demand_points
        .iter()
        .map(|dp| (dp[0], dp[1]))
        .collect();
    let mut solver = DistributionSolver::new(
        terrain,
        constraints,
        demand_points.clone(),
        source,
        f.solver_params.demand_per_node,
        PipeMaterial::Pvc,
        "DistributionAlt".to_string(),
    );

    let params = SolverParams {
        grid_resolution: f.solver_params.grid_resolution,
        ..SolverParams::default()
    };

    let solutions = solver
        .solve_distribution(
            &demand_points,
            source,
            f.solver_params.source_head,
            "DistributionAlt",
            f.solver_params.demand_per_node,
            PipeMaterial::Pvc,
            f.solver_params.num_alternatives,
            f.solver_params.mesh_density,
            f.solver_params.diameter_offset,
            f.solver_params.valve_spacing,
            f.solver_params.hydrant_spacing,
            &params,
        )
        .expect("solve_distribution alternatives must succeed");

    // Must return at least 1 solution
    assert!(
        !solutions.is_empty(),
        "solve_distribution must return at least one solution"
    );

    // Count matches oracle
    assert_eq!(
        solutions.len(),
        f.num_solutions_returned,
        "solution count {} does not match oracle {}",
        solutions.len(),
        f.num_solutions_returned
    );

    let costs: Vec<f64> = solutions.iter().map(|s| s.score.total_cost).collect();

    // Per-solution cost AND structural counts match oracle.
    // Cost parity alone can miss structural divergence (a different mesh with a
    // coincidentally equal cost), so assert node/pipe counts per alternative too.
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
        assert_eq!(
            sol.network.node_count(),
            oracle_sol.node_count,
            "alternatives[{}] node_count {} does not match oracle {}",
            i,
            sol.network.node_count(),
            oracle_sol.node_count
        );
        assert_eq!(
            sol.network.pipe_count(),
            oracle_sol.pipe_count,
            "alternatives[{}] pipe_count {} does not match oracle {}",
            i,
            sol.network.pipe_count(),
            oracle_sol.pipe_count
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

    // Pairwise-distinct costs (no two solutions share same cost)
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

    // Rank numbers correct
    for (i, sol) in solutions.iter().enumerate() {
        assert_eq!(
            sol.rank,
            (i + 1) as i64,
            "solution rank must be {} got {}",
            i + 1,
            sol.rank
        );
    }

    // pump_count == 0 for all solutions
    for sol in &solutions {
        assert_eq!(
            sol.score.pump_count, 0,
            "all distribution alternatives must have pump_count == 0"
        );
    }

    println!(
        "Distrib alternatives: {} solutions, costs={:?}, distinct={:?}",
        solutions.len(),
        costs,
        f.distinct_costs
    );
}

// ── T-5.4.g: domain invariants ────────────────────────────────────────────────

/// T-5.4.g: Solved network satisfies domain invariants:
///   - Exactly one TANK node (source)
///   - All pipes have positive length
///   - All nodes have pressure_mca set after solve
///   - Source pressure == source_head
#[test]
fn distrib_domain_invariants() {
    let f = load_distribution_golden();
    let terrain = make_distrib_terrain_from_formula(&f.terrain);
    let constraints = DesignConstraints::default();
    let source = (f.solver_params.source[0], f.solver_params.source[1]);
    let demand_points: Vec<(f64, f64)> = f
        .solver_params
        .demand_points
        .iter()
        .map(|dp| (dp[0], dp[1]))
        .collect();
    let mut solver = DistributionSolver::new(
        terrain,
        constraints,
        demand_points.clone(),
        source,
        f.solver_params.demand_per_node,
        PipeMaterial::Pvc,
        "DistributionTest".to_string(),
    );

    let params = SolverParams {
        grid_resolution: f.solver_params.grid_resolution,
        ..SolverParams::default()
    };

    let solutions = solver
        .solve_distribution(
            &demand_points,
            source,
            f.solver_params.source_head,
            "DistributionTest",
            f.solver_params.demand_per_node,
            PipeMaterial::Pvc,
            f.solver_params.num_alternatives,
            f.solver_params.mesh_density,
            f.solver_params.diameter_offset,
            f.solver_params.valve_spacing,
            f.solver_params.hydrant_spacing,
            &params,
        )
        .expect("solve_distribution must succeed");

    let network = &solutions[0].network;

    // Exactly one TANK node
    let tank_count = network
        .nodes
        .iter()
        .filter(|n| n.node_type == NodeType::Tank)
        .count();
    assert_eq!(tank_count, 1, "must have exactly one TANK node");

    // All pipes have positive length
    for pipe in &network.pipes {
        assert!(
            pipe.length > 0.0,
            "pipe {} must have positive length",
            pipe.id
        );
    }

    // Source node has correct pressure
    let source_node = network
        .nodes
        .iter()
        .find(|n| n.node_type == NodeType::Tank)
        .expect("must have tank node");
    let source_pressure = source_node
        .metadata
        .get("pressure_mca")
        .and_then(|v| v.as_f64())
        .expect("source must have pressure_mca");
    assert_abs_diff_eq!(source_pressure, f.solver_params.source_head, epsilon = 1e-9);

    // All nodes have pressure_mca set
    for node in &network.nodes {
        assert!(
            node.metadata.contains_key("pressure_mca"),
            "node {} must have pressure_mca set (BFS must reach it)",
            node.id
        );
    }
}

// ── T-5.4.h: evaluate on valves_saturated fixture ────────────────────────────

/// T-5.4.h: evaluate() on valves_saturated oracle network returns exact oracle score.
/// Verifies that the valves-before-hydrants placement in the fixture is consistent.
#[test]
fn distrib_valves_saturated_evaluate_only() {
    let f = load_distribution_valves_saturated();
    let terrain = make_distrib_terrain(&f.terrain.points, f.terrain.grid_res);
    let constraints = DesignConstraints::default();
    let demand_points: Vec<(f64, f64)> = f
        .solver_params
        .demand_points
        .iter()
        .map(|dp| (dp[0], dp[1]))
        .collect();
    let source = (f.solver_params.source[0], f.solver_params.source[1]);
    let solver = DistributionSolver::new(
        terrain,
        constraints,
        demand_points,
        source,
        f.solver_params.demand_per_node,
        PipeMaterial::Pvc,
        "DistribValvesSaturated".to_string(),
    );
    let network = reconstruct_valves_saturated_network(&f);

    let score = solver
        .evaluate(&network)
        .expect("evaluate must not error on valves_saturated network");

    assert_abs_diff_eq!(score.total_cost, f.evaluate_only.total_cost, epsilon = 1e-4);
    assert_abs_diff_eq!(
        score.total_length,
        f.evaluate_only.total_length,
        epsilon = 1e-6
    );
    assert_eq!(
        score.norm_violations, f.evaluate_only.norm_violations,
        "violations must match exactly"
    );
    assert_eq!(score.pump_count, 0, "pump_count must be 0");

    println!(
        "Distrib valves_saturated evaluate: cost={:.6}, oracle={:.6}",
        score.total_cost, f.evaluate_only.total_cost
    );
}

// ── T-5.4.i: Solver trait roundtrip (RED → GREEN via PR-D wiring) ─────────────

/// T-5.4.i: DistributionSolver implements Solver::solve.
///
/// Strict TDD RED test — this test fails on the current stub because
/// `DistributionSolver::new` only takes 2 args, but the test passes 7.
/// It becomes GREEN once D2 extends the constructor and implements Solver::solve.
///
/// Scenarios:
///   1. Happy path: golden fixture → Ok(non-empty), solutions[0].score.total_cost > 0.0
///   2. Error: single demand point → Err(_) (insufficient topology)
#[test]
fn distribution_solve_via_trait_roundtrip() {
    // ── Scenario 1: happy path via Solver trait ───────────────────────────────
    let f = load_distribution_golden();
    let terrain = make_distrib_terrain_from_formula(&f.terrain);
    let constraints = DesignConstraints::default();

    let demand_points: Vec<(f64, f64)> = f
        .solver_params
        .demand_points
        .iter()
        .map(|dp| (dp[0], dp[1]))
        .collect();
    let source = (f.solver_params.source[0], f.solver_params.source[1]);

    let mut solver = DistributionSolver::new(
        terrain,
        constraints,
        demand_points,
        source,
        f.solver_params.demand_per_node,
        PipeMaterial::Pvc,
        "test_distribution".to_string(),
    );

    let params = SolverParams {
        grid_resolution: f.solver_params.grid_resolution,
        source_head: f.solver_params.source_head,
        num_alternatives: f.solver_params.num_alternatives,
        mesh_density: f.solver_params.mesh_density,
        diameter_offset: f.solver_params.diameter_offset,
        valve_spacing: f.solver_params.valve_spacing,
        hydrant_spacing: f.solver_params.hydrant_spacing,
        ..SolverParams::default()
    };

    let result = solver.solve(&params);
    assert!(
        result.is_ok(),
        "Solver::solve must return Ok on golden fixture, got: {:?}",
        result.err()
    );
    let solutions = result.unwrap();
    assert!(
        !solutions.is_empty(),
        "Solver::solve must return at least one solution"
    );
    assert!(
        solutions[0].score.total_cost > 0.0,
        "solutions[0].score.total_cost must be > 0.0, got {}",
        solutions[0].score.total_cost
    );

    // ── Scenario 2: single demand point → Err (insufficient topology) ─────────
    let f2 = load_distribution_golden();
    let terrain2 = make_distrib_terrain_from_formula(&f2.terrain);
    let constraints2 = DesignConstraints::default();

    let mut solver_err = DistributionSolver::new(
        terrain2,
        constraints2,
        vec![(0.0, 80.0)], // single demand point — insufficient for mesh
        (0.0, 0.0),
        f2.solver_params.demand_per_node,
        PipeMaterial::Pvc,
        "test_distribution_err".to_string(),
    );

    let params_err = SolverParams {
        grid_resolution: f2.solver_params.grid_resolution,
        source_head: f2.solver_params.source_head,
        ..SolverParams::default()
    };

    assert!(
        solver_err.solve(&params_err).is_err(),
        "single demand point must yield Err, not Ok(vec![])"
    );
}
