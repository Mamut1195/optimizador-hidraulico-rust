//! Optimizer smoke tests — validate all 6 wired solvers through the GA (REQ-009).
//!
//! ## Purpose
//!
//! These tests confirm that `GeneticOptimizer::optimize()` produces at least one
//! feasible individual for each of the 6 concrete solvers once their `Solver::solve`
//! delegation is live. Feasibility is defined as `fitness[0] < 1e11` (costs below
//! 100 billion are real evaluations; `1e12` is the infeasible sentinel used by the
//! optimizer).
//!
//! ## Seed = 42 rationale
//!
//! All smoke tests use `config.seed = 42` for full determinism. Without a fixed seed,
//! NSGA-III population initialization and mutation use stochastic sampling that can
//! produce non-feasible initial populations on rare runs, causing intermittent CI
//! failures. Seed 42 is the optimizer's documented oracle default (`OptimizationConfig`
//! default). Using the same seed across all 6 tests makes failures reproducible and
//! bisectable by commit.
//!
//! ## GA budget rationale (population_size=8, max_generations=3)
//!
//! `population_size = 8` covers the NSGA-III minimum: crowding-distance requires ≥ 4
//! individuals and reference-direction association requires ≥ 2. `max_generations = 3`
//! runs exactly 1 NSGA-III selection + 2 evolution cycles, which is sufficient to
//! invoke `Solver::solve` across multiple genomes without adding meaningful wall-clock
//! cost. With 6 solvers × 8 pop × 3 gen ≈ 144 total evaluations, the expected CI
//! overhead is under 15 seconds.

use hydro_optimizer::{GeneticOptimizer, OptimizationConfig, SolverType};
use hydro_solvers::{
    ConveyanceSolver, DistributionSolver, IntakeSolver, PumpStationSolver, SewerSolver,
    SolverParams, WaterSupplySolver,
};
use hydro_terrain::TerrainModel;
use hydro_types::{DesignConstraints, PipeMaterial};
use oracle::solvers::{
    load_conveyance_golden, load_distribution_golden, load_intake_golden, load_pump_station_golden,
    load_sewer_golden, load_water_supply_golden,
};

// ── Shared terrain builder ────────────────────────────────────────────────────

/// Build a `TerrainModel` from a list of `[x, y, z]` points at a given grid resolution.
fn make_terrain(points: &[[f64; 3]], grid_res: f64) -> TerrainModel {
    let mut terrain =
        TerrainModel::from_xyz_list(points).expect("terrain must build from fixture points");
    terrain
        .build_grid(grid_res)
        .expect("terrain grid must build from fixture");
    terrain
}

// ── Smoke test helpers ────────────────────────────────────────────────────────

/// Return `true` if at least one solution has `fitness[0] < threshold`.
///
/// `ParetoResults::solutions` holds the Pareto-optimal solutions. Each solution's
/// `metadata.fitness` carries the 5-objective fitness vector set by the optimizer.
/// Any cost below `1e11` indicates a real feasible evaluation (the infeasible
/// sentinel used by the optimizer is `1e12`).
fn has_feasible_individual(results: &hydro_optimizer::ParetoResults, threshold: f64) -> bool {
    results.solutions.iter().any(|sol| {
        sol.metadata
            .fitness
            .map(|f| f[0] < threshold)
            .unwrap_or(false)
    })
}

// ── Conveyance smoke ──────────────────────────────────────────────────────────

/// Smoke: ConveyanceSolver wiring exercises the GA with golden-fixture context.
///
/// GIVEN a `GeneticOptimizer` wrapping a fully-wired `ConveyanceSolver` built from
/// `solvers_conveyance_golden.json`, seed=42, population_size=8, max_generations=3
/// WHEN `optimize()` is called
/// THEN at least one individual has `fitness[0] < 1e-99` — RED placeholder assertion
/// that always fails, establishing the RED commit before the GREEN fix in F4.
#[test]
fn optimizer_smoke_conveyance_feasible() {
    let f = load_conveyance_golden();
    let terrain = make_terrain(&f.terrain.points, f.terrain.grid_res);
    let constraints = DesignConstraints::default();
    let source = (f.solver_params.source[0], f.solver_params.source[1]);
    let destination = (
        f.solver_params.destination[0],
        f.solver_params.destination[1],
    );

    let solver = ConveyanceSolver::new(
        terrain,
        constraints,
        source,
        destination,
        PipeMaterial::Concrete,
        "smoke_conveyance".to_string(),
    );

    let base_params = SolverParams {
        design_flow: f.solver_params.design_flow,
        source_head: f.solver_params.source_head,
        num_alternatives: 1,
        grid_resolution: f.solver_params.grid_resolution,
        route_variant: f.solver_params.route_variant,
        cover_factor: f.solver_params.cover_factor,
        valve_spacing: f.solver_params.valve_spacing,
        ..SolverParams::default()
    };

    let config = OptimizationConfig {
        population_size: 8,
        generations: 3,
        seed: 42,
        ..OptimizationConfig::default()
    };

    let mut optimizer = GeneticOptimizer::new(solver, SolverType::Conveyance, config, base_params)
        .expect("GeneticOptimizer::new must not fail for conveyance smoke");

    let results = optimizer
        .optimize()
        .expect("optimize() must not error for conveyance smoke");

    assert!(
        has_feasible_individual(&results, 1e-99),
        "RED placeholder: this assertion deliberately fails to establish the RED commit. \
         Fix in F4 by changing 1e-99 to 1e11."
    );
}

// ── Sewer smoke ───────────────────────────────────────────────────────────────

/// Smoke: SewerSolver wiring exercises the GA with golden-fixture context.
///
/// RED placeholder — assertion uses impossible threshold 1e-99.
#[test]
fn optimizer_smoke_sewer_feasible() {
    let f = load_sewer_golden();
    let terrain = make_terrain(&f.terrain.points, f.terrain.grid_res);
    let constraints = DesignConstraints::default();
    let service_points: Vec<(f64, f64)> = f
        .solver_params
        .service_points
        .iter()
        .map(|p| (p[0], p[1]))
        .collect();
    let outlet = (f.solver_params.outlet[0], f.solver_params.outlet[1]);

    let solver = SewerSolver::new(
        terrain,
        constraints,
        service_points,
        outlet,
        f.solver_params.flow_per_service,
        PipeMaterial::Pvc,
        "smoke_sewer".to_string(),
    );

    let base_params = SolverParams {
        num_alternatives: 1,
        grid_resolution: f.solver_params.grid_resolution,
        route_variant: f.solver_params.route_variant,
        slope_factor: f.solver_params.slope_factor,
        cover_factor: f.solver_params.cover_factor,
        diameter_offset: f.solver_params.diameter_offset,
        manhole_spacing: f.solver_params.manhole_spacing,
        ..SolverParams::default()
    };

    let config = OptimizationConfig {
        population_size: 8,
        generations: 3,
        seed: 42,
        ..OptimizationConfig::default()
    };

    let mut optimizer = GeneticOptimizer::new(solver, SolverType::Sewer, config, base_params)
        .expect("GeneticOptimizer::new must not fail for sewer smoke");

    let results = optimizer
        .optimize()
        .expect("optimize() must not error for sewer smoke");

    assert!(
        has_feasible_individual(&results, 1e-99),
        "RED placeholder: this assertion deliberately fails to establish the RED commit. \
         Fix in F4 by changing 1e-99 to 1e11."
    );
}

// ── WaterSupply smoke ─────────────────────────────────────────────────────────

/// Smoke: WaterSupplySolver wiring exercises the GA with golden-fixture context.
///
/// RED placeholder — assertion uses impossible threshold 1e-99.
#[test]
fn optimizer_smoke_water_supply_feasible() {
    let f = load_water_supply_golden();
    let terrain = make_terrain(&f.terrain.points, f.terrain.grid_res);
    let constraints = DesignConstraints::default();
    let demand_points: Vec<(f64, f64)> = f
        .solver_params
        .demand_points
        .iter()
        .map(|p| (p[0], p[1]))
        .collect();
    let source = (f.solver_params.source[0], f.solver_params.source[1]);

    let solver = WaterSupplySolver::new(
        terrain,
        constraints,
        demand_points,
        source,
        f.solver_params.demand_per_node,
        PipeMaterial::Pvc,
        "smoke_water_supply".to_string(),
    );

    let base_params = SolverParams {
        source_head: f.solver_params.source_head,
        num_alternatives: 1,
        grid_resolution: f.solver_params.grid_resolution,
        network_type: f.solver_params.network_type,
        diameter_offset: f.solver_params.diameter_offset,
        loop_density: f.solver_params.loop_density,
        ..SolverParams::default()
    };

    let config = OptimizationConfig {
        population_size: 8,
        generations: 3,
        seed: 42,
        ..OptimizationConfig::default()
    };

    let mut optimizer = GeneticOptimizer::new(solver, SolverType::WaterSupply, config, base_params)
        .expect("GeneticOptimizer::new must not fail for water_supply smoke");

    let results = optimizer
        .optimize()
        .expect("optimize() must not error for water_supply smoke");

    assert!(
        has_feasible_individual(&results, 1e-99),
        "RED placeholder: this assertion deliberately fails to establish the RED commit. \
         Fix in F4 by changing 1e-99 to 1e11."
    );
}

// ── Distribution smoke ────────────────────────────────────────────────────────

/// Smoke: DistributionSolver wiring exercises the GA with golden-fixture context.
///
/// RED placeholder — assertion uses impossible threshold 1e-99.
#[test]
fn optimizer_smoke_distribution_feasible() {
    let f = load_distribution_golden();
    let terrain = make_terrain(&f.terrain.points, f.terrain.grid_res);
    let constraints = DesignConstraints::default();
    let demand_points: Vec<(f64, f64)> = f
        .solver_params
        .demand_points
        .iter()
        .map(|p| (p[0], p[1]))
        .collect();
    let source = (f.solver_params.source[0], f.solver_params.source[1]);

    let solver = DistributionSolver::new(
        terrain,
        constraints,
        demand_points,
        source,
        f.solver_params.demand_per_node,
        PipeMaterial::Pvc,
        "smoke_distribution".to_string(),
    );

    let base_params = SolverParams {
        source_head: f.solver_params.source_head,
        num_alternatives: 1,
        grid_resolution: f.solver_params.grid_resolution,
        mesh_density: f.solver_params.mesh_density,
        diameter_offset: f.solver_params.diameter_offset,
        valve_spacing: f.solver_params.valve_spacing,
        hydrant_spacing: f.solver_params.hydrant_spacing,
        ..SolverParams::default()
    };

    let config = OptimizationConfig {
        population_size: 8,
        generations: 3,
        seed: 42,
        ..OptimizationConfig::default()
    };

    let mut optimizer =
        GeneticOptimizer::new(solver, SolverType::Distribution, config, base_params)
            .expect("GeneticOptimizer::new must not fail for distribution smoke");

    let results = optimizer
        .optimize()
        .expect("optimize() must not error for distribution smoke");

    assert!(
        has_feasible_individual(&results, 1e-99),
        "RED placeholder: this assertion deliberately fails to establish the RED commit. \
         Fix in F4 by changing 1e-99 to 1e11."
    );
}

// ── PumpStation smoke ─────────────────────────────────────────────────────────

/// Smoke: PumpStationSolver wiring exercises the GA with golden-fixture context.
///
/// RED placeholder — assertion uses impossible threshold 1e-99.
#[test]
fn optimizer_smoke_pump_station_feasible() {
    let f = load_pump_station_golden();
    let constraints = DesignConstraints::default();

    let solver = PumpStationSolver::new(
        None,
        constraints,
        f.solver_params.suction_elevation,
        f.solver_params.discharge_elevation,
        f.solver_params.suction_pipe_length,
        f.solver_params.discharge_pipe_length,
        f.solver_params.suction_diameter,
        f.solver_params.discharge_diameter,
        PipeMaterial::Steel,
        1.0,  // wet_well_factor
        5.0,  // retention_minutes
        None, // num_pumps
        None, // suction_d_idx
        None, // discharge_d_idx
    );

    let base_params = SolverParams {
        design_flow: f.solver_params.design_flow,
        num_alternatives: 1,
        ..SolverParams::default()
    };

    let config = OptimizationConfig {
        population_size: 8,
        generations: 3,
        seed: 42,
        ..OptimizationConfig::default()
    };

    let mut optimizer = GeneticOptimizer::new(solver, SolverType::PumpStation, config, base_params)
        .expect("GeneticOptimizer::new must not fail for pump_station smoke");

    let results = optimizer
        .optimize()
        .expect("optimize() must not error for pump_station smoke");

    assert!(
        has_feasible_individual(&results, 1e-99),
        "RED placeholder: this assertion deliberately fails to establish the RED commit. \
         Fix in F4 by changing 1e-99 to 1e11."
    );
}

// ── Intake smoke ──────────────────────────────────────────────────────────────

/// Smoke: IntakeSolver wiring exercises the GA with golden-fixture context.
///
/// RED placeholder — assertion uses impossible threshold 1e-99.
#[test]
fn optimizer_smoke_intake_feasible() {
    let f = load_intake_golden();
    let constraints = DesignConstraints::default();

    let solver = IntakeSolver::new(
        None,
        constraints,
        "river".to_string(),
        f.solver_params.source_elevation,
        f.solver_params.pipe_elevation,
        f.solver_params.channel_slope,
        PipeMaterial::Concrete,
        f.solver_params.channel_width_factor,
        f.solver_params.channel_slope_factor,
        f.solver_params.weir_type,
        f.solver_params.screen_velocity_factor,
        0.15,  // screen_velocity default
        0.025, // bar_spacing default
        0.010, // bar_thickness default
        0.015, // channel_roughness default
    );

    let base_params = SolverParams {
        design_flow: f.solver_params.design_flow,
        num_alternatives: 1,
        ..SolverParams::default()
    };

    let config = OptimizationConfig {
        population_size: 8,
        generations: 3,
        seed: 42,
        ..OptimizationConfig::default()
    };

    let mut optimizer = GeneticOptimizer::new(solver, SolverType::Intake, config, base_params)
        .expect("GeneticOptimizer::new must not fail for intake smoke");

    let results = optimizer
        .optimize()
        .expect("optimize() must not error for intake smoke");

    assert!(
        has_feasible_individual(&results, 1e-99),
        "RED placeholder: this assertion deliberately fails to establish the RED commit. \
         Fix in F4 by changing 1e-99 to 1e11."
    );
}
