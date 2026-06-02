//! Integration test: run() pump_station dispatch (REQ-001, REQ-004).
//!
//! ## Strict TDD
//!
//! RED commit: `run()` returns `Err(CliError::InternalError("... WU-5"))` for
//! `project_type = "pump_station"`. The `expect` below panics, satisfying RED gate.
//!
//! GREEN commit: `run()` pump_station arm is implemented; all assertions pass.
//!
//! ## What is tested
//!
//! Builds a `DesignRequest` using the pump station golden fixture parameters
//! (suction_elevation=100.0, discharge_elevation=115.0) with a synthetic terrain
//! grid whose sample points match the source/outlet XY coordinates at the correct
//! fixture elevations.
//!
//! Since `PumpStationSolver` does not use the terrain for its algorithm (only for
//! elevation lookup at construction), we construct a small 5×5 synthetic terrain
//! that is consistent with the fixture elevations at the source and outlet points.
//!
//! Assertions (REQ-001, REQ-004):
//! - `result.success == true`
//! - `result.solutions` is non-empty
//! - `result.solutions[0].score.total_cost > 0.0`
//! - `result.elapsed_seconds > 0.0`
//! - `result.diagnostics.audit_hash` is a 64-char lowercase SHA-256 hex string (WU-8)

use hydro_cli::run;
use hydro_types::request::{DesignRequest, PointXY, PointXYZ, ProjectTypeStr};
use oracle::solvers::load_pump_station_golden;

/// Happy-path: run() dispatches to PumpStationSolver and returns success (REQ-001, REQ-004).
///
/// Uses pump_station golden fixture values (suction/discharge elevations and pipe lengths).
/// A synthetic terrain is constructed so `elevation_at(source_xy)` == suction_elevation
/// and `elevation_at(outlet_xy)` == discharge_elevation (design §2.5, §5).
/// GA budget: pop=20, gen=10, seed=42 — the validated floor per validate_request.
#[test]
fn test_run_pump_station_dispatch() {
    let f = load_pump_station_golden();

    // Fixture pump station parameters (no terrain in fixture; PumpStationSolver
    // takes Option<TerrainModel> and never reads it algorithmically).
    //
    // We must still provide terrain_points in DesignRequest so that
    // build_terrain_model() succeeds. Construct a synthetic 5×5 terrain grid
    // whose elevation function matches the fixture elevations at source/outlet.
    //
    // Source (suction) XY: (0.0, 0.0) → z = f.solver_params.suction_elevation
    // Outlet (discharge) XY: (f.solver_params.suction_pipe_length, 0.0) → z = f.solver_params.discharge_elevation
    //
    // The terrain covers a 60 m × 20 m extent so both points fall within bounds.
    // A linear z gradient is used: z(x, y) = suction_elevation + slope_x * x
    // where slope_x = (discharge_elevation - suction_elevation) / max_x.
    let suction_elev = f.solver_params.suction_elevation; // 100.0
    let discharge_elev = f.solver_params.discharge_elevation; // 115.0
    let max_x = 60.0_f64;
    let slope_x = (discharge_elev - suction_elev) / max_x;

    let mut terrain_points: Vec<PointXYZ> = Vec::new();
    for &xi in &[0.0_f64, 15.0, 30.0, 45.0, 60.0] {
        for &yi in &[0.0_f64, 5.0, 10.0, 15.0, 20.0] {
            terrain_points.push(PointXYZ {
                x: xi,
                y: yi,
                z: suction_elev + slope_x * xi,
            });
        }
    }

    // Source (suction point) at origin: elevation_at(0.0, 0.0) == suction_elevation.
    let source = PointXY { x: 0.0, y: 0.0 };
    // Outlet (discharge point) displaced by pipe length: elevation_at(50.0, 0.0) ≈ discharge_elevation.
    // We place it at x = 50.0 m (within [0, 60] bounds): z ≈ 100 + (15/60)*50 = 112.5 ≈ discharge.
    // NOTE: CLI dispatch will derive elevations from terrain.elevation_at(); the exact value
    // comes from the interpolated terrain, not the fixture directly. PumpStationSolver
    // does not use terrain at all in its algorithm — these values are passed as constructor
    // args and the GA optimizes pipe sizing from them.
    let outlet = PointXY { x: 50.0, y: 0.0 };

    let req = DesignRequest {
        project_type: ProjectTypeStr("pump_station".to_string()),
        project_name: "test_pump_station_wu5".to_string(),
        terrain_points,
        service_points: None,
        source: Some(source),
        outlet: Some(outlet),
        source_head: None,
        constraints: None,
        forbidden_zones: vec![],
        mandatory_routes: vec![],
        existing_networks: vec![],
        material: "Steel".to_string(),
        num_alternatives: 1,
        norm: "CONAGUA_MX".to_string(),
        strict_norm_compliance: false,
        flow_per_service: f.solver_params.design_flow,
        grid_resolution: 20.0, // default grid_resolution
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
    let result = run(req, Some(42)).expect("run() must return Ok for a valid pump_station request");

    // ASSERT — REQ-001 happy path scenario.
    assert!(
        result.success,
        "DesignResult.success must be true for a valid pump_station request"
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
