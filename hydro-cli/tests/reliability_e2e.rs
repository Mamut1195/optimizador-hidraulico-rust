//! Reliability end-to-end tests for release-readiness request fields.
//!
//! These tests exercise the public `run()` path, not only helper mapping, so the
//! JSON-facing fields remain safe when they are combined in production-shaped
//! requests.

use hydro_cli::run;
use hydro_types::request::{
    DesignRequest, PointXY, PointXYZ, PolygonConstraint, ProjectTypeStr, RouteConstraint,
};
use oracle::solvers::{load_intake_golden, load_sewer_golden};

fn build_intake_request_with_strict_norm_and_workers() -> DesignRequest {
    let f = load_intake_golden();
    let source_elev = f.solver_params.source_elevation;

    let mut terrain_points = Vec::new();
    for &x in &[0.0_f64, 5.0, 10.0, 15.0, 20.0] {
        for &y in &[0.0_f64, 5.0, 10.0, 15.0, 20.0] {
            terrain_points.push(PointXYZ {
                x,
                y,
                z: source_elev,
            });
        }
    }

    DesignRequest {
        project_type: ProjectTypeStr("intake".to_string()),
        project_name: "reliability_intake_strict_parallel".to_string(),
        terrain_points,
        service_points: None,
        source: Some(PointXY { x: 0.0, y: 0.0 }),
        outlet: None,
        source_head: None,
        constraints: None,
        forbidden_zones: vec![],
        mandatory_routes: vec![],
        existing_networks: vec![],
        material: "Concrete".to_string(),
        num_alternatives: 1,
        norm: "CONAGUA_MX".to_string(),
        strict_norm_compliance: true,
        flow_per_service: f.solver_params.design_flow,
        grid_resolution: 5.0,
        seed: Some(42),
        nsga_population_size: 20,
        nsga_generations: 10,
        nsga_max_time_seconds: 30,
        nsga_num_workers: Some(2),
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
    }
}

fn build_sewer_request_with_spatial_constraints() -> DesignRequest {
    let f = load_sewer_golden();

    let terrain_points = f
        .terrain
        .points
        .iter()
        .map(|p| PointXYZ {
            x: p[0],
            y: p[1],
            z: p[2],
        })
        .collect();

    let service_points: Vec<PointXY> = f
        .solver_params
        .service_points
        .iter()
        .map(|p| PointXY { x: p[0], y: p[1] })
        .collect();

    let outlet = PointXY {
        x: f.solver_params.outlet[0],
        y: f.solver_params.outlet[1],
    };
    let first_service = service_points[0].clone();

    DesignRequest {
        project_type: ProjectTypeStr("sewer".to_string()),
        project_name: "reliability_sewer_spatial_constraints".to_string(),
        terrain_points,
        service_points: Some(service_points),
        outlet: Some(outlet.clone()),
        source: None,
        source_head: None,
        constraints: None,
        forbidden_zones: vec![PolygonConstraint {
            coords: vec![
                PointXY {
                    x: 10_000.0,
                    y: 10_000.0,
                },
                PointXY {
                    x: 10_100.0,
                    y: 10_000.0,
                },
                PointXY {
                    x: 10_100.0,
                    y: 10_100.0,
                },
            ],
            buffer: Some(0.0),
        }],
        mandatory_routes: vec![RouteConstraint {
            coords: vec![outlet, first_service],
            corridor_width: Some(200.0),
        }],
        existing_networks: vec![],
        material: "PVC".to_string(),
        num_alternatives: 1,
        norm: "CONAGUA_MX".to_string(),
        strict_norm_compliance: false,
        flow_per_service: f.solver_params.flow_per_service,
        grid_resolution: f.terrain.grid_res,
        seed: Some(42),
        nsga_population_size: 20,
        nsga_generations: 10,
        nsga_max_time_seconds: 30,
        nsga_num_workers: Some(2),
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
    }
}

#[test]
fn test_run_intake_strict_norm_compliance_with_parallel_workers() {
    let req = build_intake_request_with_strict_norm_and_workers();

    let result = run(req, Some(42))
        .expect("strict norm compliance plus nsga_num_workers > 1 must run end-to-end for intake");

    assert!(result.success);
    assert!(!result.solutions.is_empty());
    assert_eq!(result.norm, "CONAGUA_MX");
    assert_eq!(result.diagnostics.audit_hash.len(), 64);
}

#[test]
fn test_run_sewer_with_forbidden_zone_and_mandatory_route() {
    let req = build_sewer_request_with_spatial_constraints();

    let result = run(req, Some(42))
        .expect("forbidden_zones, mandatory_routes, and nsga_num_workers > 1 must run end-to-end");

    assert!(result.success);
    assert!(!result.solutions.is_empty());
    assert_eq!(result.diagnostics.audit_hash.len(), 64);
}
