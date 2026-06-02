//! Integration test: run() water_supply dispatch (REQ-001, REQ-004).
//!
//! ## Strict TDD
//!
//! RED commit: `run()` returns `Err(CliError::InternalError("... WU-5"))` for
//! `project_type = "water_supply"`. The `result.success` assertion below therefore
//! fails (the test panics via `expect`), satisfying the RED gate.
//!
//! GREEN commit: `run()` water_supply arm is implemented; all assertions pass.
//!
//! ## What is tested
//!
//! Builds a full `DesignRequest` from the water supply golden fixture
//! (`tests/oracle/fixtures/solvers_water_supply_golden.json`) with a fast GA
//! budget (population_size=20, generations=10, seed=42) and calls `hydro_cli::run`.
//!
//! Assertions (REQ-001, REQ-004):
//! - `result.success == true`
//! - `result.solutions` is non-empty
//! - `result.solutions[0].score.total_cost > 0.0`
//! - `result.elapsed_seconds > 0.0`
//! - `result.diagnostics.audit_hash` is a 64-char lowercase SHA-256 hex string (WU-8)

use hydro_cli::run;
use hydro_types::request::{DesignRequest, PointXY, PointXYZ, ProjectTypeStr};
use oracle::solvers::load_water_supply_golden;

/// Happy-path: run() dispatches to WaterSupplySolver and returns success (REQ-001, REQ-004).
///
/// Uses the water_supply golden fixture for terrain + service topology.
/// GA budget: pop=20, gen=10, seed=42 — the validated floor per validate_request.
/// Expected wall time < 5 s on CI (Phase 6.5 evidence: ~0.2 s per eval at pop=8 gen=3).
#[test]
fn test_run_water_supply_dispatch() {
    let f = load_water_supply_golden();

    // Build terrain_points from the fixture.
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

    // service_points = demand_points in the WaterSupply model.
    let service_points: Vec<PointXY> = f
        .solver_params
        .demand_points
        .iter()
        .map(|p| PointXY { x: p[0], y: p[1] })
        .collect();

    // source = tank / reservoir coordinate.
    let source = PointXY {
        x: f.solver_params.source[0],
        y: f.solver_params.source[1],
    };

    let req = DesignRequest {
        project_type: ProjectTypeStr("water_supply".to_string()),
        project_name: "test_water_supply_wu5".to_string(),
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
        // flow_per_service doubles as demand_per_node for WaterSupply (design §2.5).
        flow_per_service: f.solver_params.demand_per_node,
        grid_resolution: f.terrain.grid_res,
        seed: Some(42),
        // Minimum GA budget allowed by validate_request: pop in [20,500], gen in [10,500],
        // max_time in [30,7200]. Using the lower bounds to keep CI wall time low.
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
    let result = run(req, Some(42)).expect("run() must return Ok for a valid water_supply request");

    // ASSERT — REQ-001 happy path scenario.
    assert!(
        result.success,
        "DesignResult.success must be true for a valid water_supply request"
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
