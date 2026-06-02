//! Integration test: run() sewer dispatch (REQ-001, REQ-004).
//!
//! ## Strict TDD
//!
//! RED commit: this file exists but `run()` body is `todo!()`, so the test
//! panics with "not yet implemented". The file MUST be committed before any
//! implementation is added to `lib.rs`.
//!
//! GREEN commit: `run()` sewer arm is implemented; assertions below all pass.
//!
//! ## What is tested
//!
//! Builds a full `DesignRequest` from the sewer golden fixture
//! (`tests/oracle/fixtures/solvers_sewer_golden.json`) with a fast GA budget
//! (population_size=8, generations=3, seed=42) and calls `hydro_cli::run`.
//!
//! Assertions (REQ-001, REQ-004):
//! - `result.success == true`
//! - `result.solutions` is non-empty
//! - `result.solutions[0].score.total_cost > 0.0`
//! - `result.elapsed_seconds > 0.0`
//! - `result.diagnostics.audit_hash` is a 64-char lowercase SHA-256 hex string (WU-8)

use hydro_cli::run;
use hydro_types::request::{DesignRequest, PointXY, PointXYZ, ProjectTypeStr};
use oracle::solvers::load_sewer_golden;

/// Happy-path: run() dispatches to SewerSolver and returns success (REQ-001, REQ-004).
///
/// Uses the sewer golden fixture for terrain + service topology.
/// GA budget: pop=8, gen=3, seed=42 — expected wall time < 2 s on CI.
#[test]
fn test_run_sewer_dispatch() {
    let f = load_sewer_golden();

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

    // Build service_points from the fixture.
    let service_points: Vec<PointXY> = f
        .solver_params
        .service_points
        .iter()
        .map(|p| PointXY { x: p[0], y: p[1] })
        .collect();

    // Build outlet from the fixture.
    let outlet = PointXY {
        x: f.solver_params.outlet[0],
        y: f.solver_params.outlet[1],
    };

    // Construct the DesignRequest with a fast GA budget.
    let req = DesignRequest {
        project_type: ProjectTypeStr("sewer".to_string()),
        project_name: "test_sewer_wu4".to_string(),
        terrain_points,
        service_points: Some(service_points),
        outlet: Some(outlet),
        source: None,
        source_head: None,
        constraints: None,
        forbidden_zones: vec![],
        mandatory_routes: vec![],
        existing_networks: vec![],
        // Material is a canonical string after deserialization; "PVC" is valid.
        material: "PVC".to_string(),
        num_alternatives: 1,
        norm: "CONAGUA_MX".to_string(),
        strict_norm_compliance: false,
        flow_per_service: f.solver_params.flow_per_service,
        grid_resolution: f.terrain.grid_res,
        seed: Some(42),
        // Fast GA budget constrained by validate_request ranges:
        //   population_size in [20, 500], generations in [10, 500], max_time in [30, 7200].
        // Using minimums: pop=20, gen=10, max_time=30 keeps expected wall time ~ 3-5 s on CI.
        // Phase 6.5 smoke used pop=8 gen=3 directly (bypassed validate_request); the
        // integration test must go through the full validation path.
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

    // ACT — run() will panic with todo!() on RED commit; passes on GREEN.
    let result = run(req, Some(42)).expect("run() must return Ok for a valid sewer request");

    // ASSERT — REQ-001 happy path scenario.
    assert!(
        result.success,
        "DesignResult.success must be true for a valid sewer request"
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
