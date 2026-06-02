//! Integration test: run() intake dispatch (REQ-001, REQ-004).
//!
//! ## Strict TDD
//!
//! RED commit: `run()` returns `Err(CliError::InternalError("... WU-5"))` for
//! `project_type = "intake"`. The `expect` below panics, satisfying RED gate.
//!
//! GREEN commit: `run()` intake arm is implemented; all assertions pass.
//!
//! ## What is tested
//!
//! Builds a `DesignRequest` using the intake golden fixture parameters
//! (source_elevation=120.0, pipe_elevation=118.0) with a synthetic terrain
//! grid so that `elevation_at(source_xy)` returns a value close to `source_elevation`.
//!
//! Since `IntakeSolver` does not use terrain algorithmically (only for elevation
//! lookup at construction), a small synthetic terrain is sufficient.
//!
//! `source_type` defaults to `"river"` (CLI v1 constraint, design §2 table, §2.7 note).
//! All intake channel/screen constants use the Phase 6.5 smoke-test defaults documented
//! in `lib.rs` (screen_velocity=0.15, bar_spacing=0.025, bar_thickness=0.010,
//! channel_roughness=0.015).
//!
//! Assertions (REQ-001, REQ-004):
//! - `result.success == true`
//! - `result.solutions` is non-empty
//! - `result.solutions[0].score.total_cost > 0.0`
//! - `result.elapsed_seconds > 0.0`
//! - `result.diagnostics.audit_hash` is empty string (WU-8 fills it)

use hydro_cli::run;
use hydro_types::request::{DesignRequest, PointXY, PointXYZ, ProjectTypeStr};
use oracle::solvers::load_intake_golden;

/// Happy-path: run() dispatches to IntakeSolver and returns success (REQ-001, REQ-004).
///
/// Uses intake golden fixture values (source/pipe elevations, channel/screen params).
/// A synthetic terrain is constructed so `elevation_at(source_xy)` ≈ source_elevation.
/// GA budget: pop=20, gen=10, seed=42 — the validated floor per validate_request.
/// source_type defaults to "river" (CLI v1, design §2.7, VALID_SOURCE_TYPES).
#[test]
fn test_run_intake_dispatch() {
    let f = load_intake_golden();

    // Fixture intake parameters (no terrain in fixture; IntakeSolver takes
    // Option<TerrainModel> and never reads it algorithmically).
    //
    // Construct a small synthetic terrain so build_terrain_model() succeeds.
    // Source XY: (0.0, 0.0) → z = source_elevation = 120.0.
    // We build a flat 5×5 grid at z = source_elevation so that
    // elevation_at(0.0, 0.0) == 120.0 exactly (on-sample NN result).
    //
    // pipe_elevation is passed as source_elevation - 2.0 (= 118.0) per design §5
    // placeholder: "1 m below ground" is illustrative; we use the fixture value
    // source_elevation - (source_elevation - pipe_elevation) = pipe_elevation directly
    // in the dispatch arm.
    let source_elev = f.solver_params.source_elevation; // 120.0

    let mut terrain_points: Vec<PointXYZ> = Vec::new();
    for &xi in &[0.0_f64, 5.0, 10.0, 15.0, 20.0] {
        for &yi in &[0.0_f64, 5.0, 10.0, 15.0, 20.0] {
            terrain_points.push(PointXYZ {
                x: xi,
                y: yi,
                z: source_elev,
            });
        }
    }

    // Source (intake point) at origin — elevation_at(0.0, 0.0) == source_elevation.
    let source = PointXY { x: 0.0, y: 0.0 };

    let req = DesignRequest {
        project_type: ProjectTypeStr("intake".to_string()),
        project_name: "test_intake_wu5".to_string(),
        terrain_points,
        service_points: None,
        source: Some(source),
        outlet: None,
        source_head: None,
        constraints: None,
        forbidden_zones: vec![],
        mandatory_routes: vec![],
        existing_networks: vec![],
        material: "Concrete".to_string(),
        num_alternatives: 1,
        norm: "CONAGUA_MX".to_string(),
        strict_norm_compliance: false,
        flow_per_service: f.solver_params.design_flow,
        grid_resolution: 5.0, // small grid for the synthetic 20×20 terrain
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
    let result = run(req, Some(42)).expect("run() must return Ok for a valid intake request");

    // ASSERT — REQ-001 happy path scenario.
    assert!(
        result.success,
        "DesignResult.success must be true for a valid intake request"
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
    assert_eq!(
        result.diagnostics.audit_hash, "",
        "audit_hash must be empty string in WU-5 (WU-8 fills it)"
    );
}
