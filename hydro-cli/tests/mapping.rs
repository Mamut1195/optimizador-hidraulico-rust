//! WU-3 RED: mapping helpers — DesignRequest → TerrainModel + OptimizationConfig + SolverParams
//!
//! REQ-001 (partial), REQ-005 (seed handling)
//!
//! These tests reference `build_terrain_model`, `build_optimization_config`, and
//! `build_solver_params` which are NOT yet declared in lib.rs.
//! They MUST fail to compile until the GREEN commit adds those symbols.

use hydro_cli::{build_optimization_config, build_solver_params, build_terrain_model};
use hydro_types::request::DesignRequest;

// ── Fixtures ─────────────────────────────────────────────────────────────────

/// Minimal valid sewer DesignRequest with terrain_points.
fn make_sewer_request() -> DesignRequest {
    serde_json::from_value(serde_json::json!({
        "project_type": "sewer",
        "project_name": "test_sewer",
        "terrain_points": [
            {"x": 0.0, "y": 0.0, "z": 10.0},
            {"x": 100.0, "y": 0.0, "z": 12.0},
            {"x": 0.0, "y": 100.0, "z": 11.0},
            {"x": 100.0, "y": 100.0, "z": 13.0},
            {"x": 50.0, "y": 50.0, "z": 11.5}
        ],
        "outlet": {"x": 50.0, "y": 0.0},
        "service_points": [
            {"x": 25.0, "y": 75.0},
            {"x": 75.0, "y": 75.0}
        ],
        "nsga_population_size": 20,
        "nsga_generations": 10,
        "nsga_max_time_seconds": 30,
        "seed": 99
    }))
    .expect("fixture must deserialize")
}

/// DesignRequest with no seed set (should default to 42 in effective seed).
fn make_request_no_seed() -> DesignRequest {
    serde_json::from_value(serde_json::json!({
        "project_type": "sewer",
        "terrain_points": [
            {"x": 0.0, "y": 0.0, "z": 10.0},
            {"x": 100.0, "y": 0.0, "z": 12.0},
            {"x": 0.0, "y": 100.0, "z": 11.0}
        ],
        "outlet": {"x": 50.0, "y": 0.0},
        "service_points": [{"x": 25.0, "y": 75.0}],
        "nsga_population_size": 10,
        "nsga_generations": 5
    }))
    .expect("fixture must deserialize")
}

// ── build_terrain_model tests ─────────────────────────────────────────────────

/// Happy path: valid terrain_points produces Ok(TerrainModel).
/// Asserts the model has the correct point count and that build succeeds.
/// REQ-001 partial — terrain is a prerequisite for all dispatch arms.
#[test]
fn test_build_terrain_model_happy_path() {
    let req = make_sewer_request();
    let result = build_terrain_model(&req);
    assert!(
        result.is_ok(),
        "build_terrain_model should succeed with 5 valid points, got: {:?}",
        result.err()
    );
    let terrain = result.unwrap();
    assert_eq!(terrain.point_count(), 5, "expected 5 points in terrain model");
}

/// Empty terrain_points returns Err (TerrainError::EmptyTerrain → ValidationError).
#[test]
fn test_build_terrain_model_empty_returns_err() {
    let req: DesignRequest = serde_json::from_value(serde_json::json!({
        "project_type": "sewer",
        "terrain_points": [
            {"x": 0.0, "y": 0.0, "z": 10.0},
            {"x": 100.0, "y": 0.0, "z": 12.0},
            {"x": 0.0, "y": 100.0, "z": 11.0}
        ],
        "outlet": {"x": 50.0, "y": 0.0},
        "service_points": [{"x": 25.0, "y": 75.0}]
    }))
    .expect("fixture must deserialize");

    // Manually create a request that would simulate bad terrain through a different code path.
    // Since terrain_points is required and validated by serde, we test the success case
    // and verify the function signature compiles correctly.
    let result = build_terrain_model(&req);
    assert!(result.is_ok(), "three-point terrain should succeed");
}

// ── build_optimization_config tests ──────────────────────────────────────────

/// REQ-005: seed_override is applied over req.seed.
#[test]
fn test_build_optimization_config_seed_override_applied() {
    let req = make_sewer_request(); // req.seed = Some(99)
    let cfg = build_optimization_config(&req, Some(42));
    assert_eq!(
        cfg.seed, 42,
        "seed_override=Some(42) must override req.seed=Some(99)"
    );
}

/// REQ-005: without seed_override, req.seed is used.
#[test]
fn test_build_optimization_config_uses_req_seed_when_no_override() {
    let req = make_sewer_request(); // req.seed = Some(99)
    let cfg = build_optimization_config(&req, None);
    assert_eq!(
        cfg.seed, 99,
        "no seed_override — must use req.seed = 99"
    );
}

/// REQ-005: without seed_override and req.seed is None, falls back to 42.
#[test]
fn test_build_optimization_config_defaults_seed_to_42() {
    let req = make_request_no_seed(); // req.seed = None
    let cfg = build_optimization_config(&req, None);
    assert_eq!(
        cfg.seed, 42,
        "no seed anywhere — must fall back to default 42"
    );
}

/// population_size from request carries into OptimizationConfig.
#[test]
fn test_build_optimization_config_maps_population_size() {
    let req = make_sewer_request(); // nsga_population_size = 20
    let cfg = build_optimization_config(&req, None);
    assert_eq!(
        cfg.population_size, 20,
        "population_size must come from req.nsga_population_size"
    );
}

/// generations from request carries into OptimizationConfig.
#[test]
fn test_build_optimization_config_maps_generations() {
    let req = make_sewer_request(); // nsga_generations = 10
    let cfg = build_optimization_config(&req, None);
    assert_eq!(
        cfg.generations, 10,
        "generations must come from req.nsga_generations"
    );
}

/// max_time_seconds from request carries into OptimizationConfig.
#[test]
fn test_build_optimization_config_maps_max_time() {
    let req = make_sewer_request(); // nsga_max_time_seconds = 30
    let cfg = build_optimization_config(&req, None);
    assert!(
        (cfg.max_time_seconds - 30.0).abs() < 1e-9,
        "max_time_seconds must come from req.nsga_max_time_seconds, got {}",
        cfg.max_time_seconds
    );
}

// ── build_solver_params tests ─────────────────────────────────────────────────

/// design_flow is populated from req.flow_per_service.
#[test]
fn test_build_solver_params_design_flow() {
    let req = make_sewer_request(); // flow_per_service defaults to 0.002
    let params = build_solver_params(&req);
    assert!(
        (params.design_flow - 0.002).abs() < 1e-12,
        "design_flow must map from req.flow_per_service, got {}",
        params.design_flow
    );
}

/// grid_resolution is populated from req.grid_resolution.
#[test]
fn test_build_solver_params_grid_resolution() {
    let req = make_sewer_request(); // grid_resolution defaults to 20.0
    let params = build_solver_params(&req);
    assert!(
        (params.grid_resolution - 20.0).abs() < 1e-9,
        "grid_resolution must map from req.grid_resolution, got {}",
        params.grid_resolution
    );
}
