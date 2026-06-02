//! Integration test: run() distribution dispatch (REQ-001, REQ-004).
//!
//! ## Strict TDD
//!
//! RED commit: `run()` returns `Err(CliError::InternalError("... WU-5"))` for
//! `project_type = "distribution"`. The `expect` below panics, satisfying RED gate.
//!
//! GREEN commit: `run()` distribution arm is implemented; all assertions pass.
//!
//! ## What is tested
//!
//! Builds a `DesignRequest` from the distribution golden fixture with a fast GA
//! budget (population_size=20, generations=10, seed=42) and calls `hydro_cli::run`.
//!
//! The distribution golden fixture has >= 2 demand points, satisfying the
//! `validate_request` guard (design §2 rule 3: >= 2 demand points required).
//!
//! Assertions (REQ-001, REQ-004):
//! - `result.success == true`
//! - `result.solutions` is non-empty
//! - `result.solutions[0].score.total_cost > 0.0`
//! - `result.elapsed_seconds > 0.0`
//! - `result.diagnostics.audit_hash` is a 64-char lowercase SHA-256 hex string (WU-8)

use hydro_cli::run;
use hydro_types::request::{DesignRequest, PointXY, PointXYZ, ProjectTypeStr};
use oracle::solvers::load_distribution_golden;

/// Happy-path: run() dispatches to DistributionSolver and returns success (REQ-001, REQ-004).
///
/// Uses the distribution golden fixture for terrain + source/demand topology.
/// GA budget: pop=20, gen=10, seed=42 — the validated floor per validate_request.
/// The fixture has >= 2 demand points (validate_request enforces len() >= 2).
#[test]
fn test_run_distribution_dispatch() {
    let f = load_distribution_golden();

    let terrain_points: Vec<PointXYZ> = f
        .terrain
        .points
        .iter()
        .map(|p| PointXYZ {
            x: p[0],
            y: p[1],
            z: p[2],
        })
        .collect();

    // service_points = demand_points in the Distribution model.
    let service_points: Vec<PointXY> = f
        .solver_params
        .demand_points
        .iter()
        .map(|p| PointXY { x: p[0], y: p[1] })
        .collect();

    // The fixture must have >= 2 demand points; validate_request will enforce this.
    assert!(
        service_points.len() >= 2,
        "distribution fixture must have >= 2 demand points; got {}",
        service_points.len()
    );

    let source = PointXY {
        x: f.solver_params.source[0],
        y: f.solver_params.source[1],
    };

    let req = DesignRequest {
        project_type: ProjectTypeStr("distribution".to_string()),
        project_name: "test_distribution_wu5".to_string(),
        terrain_points,
        service_points: Some(service_points),
        source: Some(source),
        outlet: None,
        source_head: Some(f.solver_params.source_head),
        constraints: None,
        forbidden_zones: vec![],
        mandatory_routes: vec![],
        existing_networks: vec![],
        material: "PVC".to_string(),
        num_alternatives: 1,
        norm: "CONAGUA_MX".to_string(),
        strict_norm_compliance: false,
        // flow_per_service doubles as demand_per_node for Distribution (design §2.5).
        flow_per_service: f.solver_params.demand_per_node,
        grid_resolution: f.terrain.grid_res,
        seed: Some(42),
        nsga_population_size: 20,
        nsga_generations: 10,
        nsga_max_time_seconds: 30,
        nsga_num_workers: None,
        enforce_cover_depth: false,
        enforce_existing_clearance: false,
        enforce_segment_slopes: false,
        enable_path_smoothing: false,
        weight_cost: 0.30,
        weight_excavation: 0.25,
        weight_pumping: 0.15,
        weight_interference: 0.10,
        weight_resilience: 0.20,
        min_vertical_separation: 0.3,
        min_horizontal_separation: 1.0,
        project_crs: None,
    };

    // ACT — returns Err(InternalError("... WU-5")) on RED commit; Ok on GREEN.
    let result = run(req, Some(42)).expect("run() must return Ok for a valid distribution request");

    // ASSERT — REQ-001 happy path scenario.
    assert!(
        result.success,
        "DesignResult.success must be true for a valid distribution request"
    );
    assert!(
        !result.solutions.is_empty(),
        "DesignResult.solutions must be non-empty after a successful run"
    );
    assert!(
        result.solutions[0].score.total_cost > 0.0,
        "best solution total_cost must be > 0.0; got {}",
        result.solutions[0].score.total_cost
    );
    assert!(
        result.elapsed_seconds > 0.0,
        "elapsed_seconds must be > 0.0 after a real optimization run"
    );
    // WU-8: audit_hash is now a 64-char lowercase SHA-256 hex string (REQ-005).
    assert_eq!(
        result.diagnostics.audit_hash.len(),
        64,
        "audit_hash must be exactly 64 chars after WU-8"
    );
    assert!(
        result
            .diagnostics
            .audit_hash
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_uppercase()),
        "audit_hash must be lowercase hex [0-9a-f], got: '{}'",
        result.diagnostics.audit_hash
    );
}
