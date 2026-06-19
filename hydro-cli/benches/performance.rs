use std::hint::black_box;
use std::time::Duration;

use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use hydro_cli::validate_request;
use hydro_optimizer::{GeneticOptimizer, OptimizationConfig, SolverType};
use hydro_solvers::{PumpStationSolver, Solver, SolverParams};
use hydro_terrain::{CostFunction, TerrainGraph, TerrainModel};
use hydro_types::request::{DesignRequest, PointXY, PointXYZ, ProjectTypeStr};
use hydro_types::{DesignConstraints, PipeMaterial};
use oracle::solvers::{load_pump_station_golden, load_sewer_golden};

fn make_terrain(points: &[[f64; 3]], grid_resolution: f64) -> TerrainModel {
    let mut terrain =
        TerrainModel::from_xyz_list(points).expect("benchmark terrain fixture must be valid");
    terrain
        .build_grid(grid_resolution)
        .expect("benchmark terrain grid must build");
    terrain
}

fn build_pump_case() -> (PumpStationSolver, SolverParams) {
    let fixture = load_pump_station_golden();
    let solver = PumpStationSolver::new(
        None,
        DesignConstraints::default(),
        fixture.solver_params.suction_elevation,
        fixture.solver_params.discharge_elevation,
        fixture.solver_params.suction_pipe_length,
        fixture.solver_params.discharge_pipe_length,
        fixture.solver_params.suction_diameter,
        fixture.solver_params.discharge_diameter,
        PipeMaterial::Steel,
        1.0,
        5.0,
        None,
        None,
        None,
    );
    let params = SolverParams {
        design_flow: fixture.solver_params.design_flow,
        num_alternatives: 1,
        ..SolverParams::default()
    };
    (solver, params)
}

fn build_pump_optimizer() -> GeneticOptimizer<PumpStationSolver> {
    let (solver, params) = build_pump_case();
    let config = OptimizationConfig {
        population_size: 8,
        generations: 3,
        seed: 42,
        ..OptimizationConfig::default()
    };

    GeneticOptimizer::new(solver, SolverType::PumpStation, config, params)
        .expect("benchmark optimizer fixture must be valid")
}

fn build_valid_sewer_request() -> DesignRequest {
    let fixture = load_sewer_golden();
    let terrain_points = fixture
        .terrain
        .points
        .iter()
        .map(|point| PointXYZ {
            x: point[0],
            y: point[1],
            z: point[2],
        })
        .collect();
    let service_points = fixture
        .solver_params
        .service_points
        .iter()
        .map(|point| PointXY {
            x: point[0],
            y: point[1],
        })
        .collect();

    DesignRequest {
        project_type: ProjectTypeStr("sewer".to_string()),
        project_name: "benchmark_validate_only".to_string(),
        terrain_points,
        service_points: Some(service_points),
        outlet: Some(PointXY {
            x: fixture.solver_params.outlet[0],
            y: fixture.solver_params.outlet[1],
        }),
        source: None,
        source_head: None,
        constraints: None,
        forbidden_zones: vec![],
        mandatory_routes: vec![],
        existing_networks: vec![],
        material: "PVC".to_string(),
        num_alternatives: 1,
        norm: "CONAGUA_MX".to_string(),
        strict_norm_compliance: false,
        flow_per_service: fixture.solver_params.flow_per_service,
        grid_resolution: fixture.terrain.grid_res,
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
    }
}

fn performance_benchmarks(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("performance");
    group
        .sample_size(20)
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(3))
        .noise_threshold(0.10);

    let sewer_fixture = load_sewer_golden();
    let terrain = make_terrain(
        &sewer_fixture.terrain.points,
        sewer_fixture.terrain.grid_res,
    );
    let grid_resolution = sewer_fixture.terrain.grid_res;
    group.bench_function("terrain_graph_from_grid", |bencher| {
        bencher.iter(|| {
            let mut graph = TerrainGraph::new(CostFunction::default());
            graph.from_grid(black_box(grid_resolution), black_box(&terrain));
            black_box(graph)
        });
    });

    group.bench_function("pump_station_solver", |bencher| {
        bencher.iter_batched(
            build_pump_case,
            |(mut solver, params)| {
                black_box(
                    solver
                        .solve(black_box(&params))
                        .expect("benchmark solver execution must succeed"),
                )
            },
            BatchSize::SmallInput,
        );
    });

    group.bench_function("pump_station_optimizer_8x3", |bencher| {
        bencher.iter_batched(
            build_pump_optimizer,
            |mut optimizer| {
                black_box(
                    optimizer
                        .optimize()
                        .expect("benchmark optimizer execution must succeed"),
                )
            },
            BatchSize::SmallInput,
        );
    });

    let request_json =
        serde_json::to_vec(&build_valid_sewer_request()).expect("benchmark request must serialize");
    group.bench_function("validate_only_json", |bencher| {
        bencher.iter(|| {
            let request: DesignRequest = serde_json::from_slice(black_box(&request_json))
                .expect("benchmark request must deserialize");
            validate_request(black_box(&request)).expect("benchmark request must validate");
            black_box(serde_json::json!({
                "status": "valid",
                "project_type": request.project_type.as_str(),
            }))
        });
    });

    group.finish();
}

criterion_group!(benches, performance_benchmarks);
criterion_main!(benches);
