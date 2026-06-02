//! hydro-cli library — public API for the JSON-in / JSON-out engine shell.
//!
//! # Public surface (REQ-001, REQ-010)
//!
//! - [`CliError`] — typed error enum with [`CliError::exit_code`] (REQ-003)
//! - [`run`] — full optimization dispatch (all 6 solvers, WU-4/WU-5)
//! - [`validate_request`] — structural + solver-specific validation stub (WU-2)
//!
//! All items are re-exported at the crate root so integration tests can call
//! them directly without spawning a subprocess (REQ-010).

use std::time::Instant;

use hydro_optimizer::{GeneticOptimizer, OptimizationConfig, OptimizationError, SolverType};
use hydro_solvers::{
    ConveyanceSolver, DistributionSolver, IntakeSolver, PumpStationSolver, SewerSolver, Solver,
    SolverError, SolverParams, WaterSupplySolver,
};
use hydro_terrain::TerrainModel;
use hydro_types::{
    error::HydroTypesError,
    request::{DesignRequest, PointXY},
    response::{DesignResult, Diagnostics},
    PipeMaterial,
};

// ── CliError ────────────────────────────────────────────────────────────────

/// Typed error for the `hydro-cli` library and binary.
///
/// Four variants map to four documented exit codes (REQ-003). The variant set
/// is closed: no additional variants are permitted to preserve the exit-code
/// contract.
///
/// # Exit code table
///
/// | Variant                    | `exit_code()` | Trigger                                                        |
/// |----------------------------|---------------|----------------------------------------------------------------|
/// | `ValidationError(_)`       | 1             | JSON parse error, hydro-types validation, missing solver field |
/// | `NoFeasibleSolution`       | 2             | `OptimizationError::AllInfeasible`                             |
/// | `NormComplianceFailed(_)`  | 3             | `OptimizationError::NormValidationFailure(_)`                  |
/// | `InternalError(_)`         | 4             | All other optimizer/solver failures                            |
#[derive(Debug)]
pub enum CliError {
    /// A request field failed validation (exit 1).
    ///
    /// Covers JSON parse errors (wrapped via `CrossFieldViolation`),
    /// hydro-types range/cross-field violations, and missing solver-required
    /// `Option<_>` fields.
    ValidationError(HydroTypesError),

    /// The optimizer ran to completion but found no feasible solution (exit 2).
    ///
    /// Produced when `OptimizationError::AllInfeasible` is returned.
    NoFeasibleSolution,

    /// All solutions violate norm compliance in strict mode (exit 3).
    ///
    /// Produced when `OptimizationError::NormValidationFailure` is returned.
    /// The inner `String` carries the norm-violation message for `stderr`.
    NormComplianceFailed(String),

    /// An unrecoverable internal engine error occurred (exit 4).
    ///
    /// Covers `InvalidConfig`, `EvaluatorFailure`, `Solver(_)`, `Internal`,
    /// and `TimeBudgetExceeded` from `OptimizationError`.
    InternalError(String),
}

impl CliError {
    /// Returns the process exit code for this error variant.
    ///
    /// Maps exactly to the REQ-003 table:
    /// - `ValidationError` → 1
    /// - `NoFeasibleSolution` → 2
    /// - `NormComplianceFailed` → 3
    /// - `InternalError` → 4
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::ValidationError(_) => 1,
            Self::NoFeasibleSolution => 2,
            Self::NormComplianceFailed(_) => 3,
            Self::InternalError(_) => 4,
        }
    }
}

// ── From impls ──────────────────────────────────────────────────────────────

impl From<HydroTypesError> for CliError {
    /// Wraps a hydro-types domain error as a `ValidationError` (exit 1).
    fn from(e: HydroTypesError) -> Self {
        Self::ValidationError(e)
    }
}

impl From<serde_json::Error> for CliError {
    /// Wraps a JSON parse/serialize error into `ValidationError` via
    /// `HydroTypesError::CrossFieldViolation` (exit 1).
    ///
    /// Design §4 ADR: avoids a separate `CliError::Json` variant; keeps the
    /// exit-code table at exactly four entries.
    fn from(e: serde_json::Error) -> Self {
        Self::ValidationError(HydroTypesError::CrossFieldViolation {
            message: e.to_string(),
        })
    }
}

impl From<OptimizationError> for CliError {
    /// Splits `OptimizationError` across the four `CliError` exit codes per
    /// design §4 arm-by-arm table.
    fn from(err: OptimizationError) -> Self {
        match err {
            // exit 2
            OptimizationError::AllInfeasible => Self::NoFeasibleSolution,

            // exit 3
            OptimizationError::NormValidationFailure(msg) => Self::NormComplianceFailed(msg),

            // exit 4 — config / evaluator / time-budget failures
            OptimizationError::InvalidConfig(msg) => {
                Self::InternalError(format!("invalid config: {msg}"))
            }
            OptimizationError::EvaluatorFailure(msg) => {
                Self::InternalError(format!("evaluator failure: {msg}"))
            }
            OptimizationError::TimeBudgetExceeded(secs) => {
                Self::InternalError(format!("time budget exceeded: {secs:.1}s"))
            }
            OptimizationError::Internal(msg) => Self::InternalError(msg),

            // exit 4 — solver-level errors (defense-in-depth; should be caught by
            // validate_request upstream, but mapped here for robustness)
            OptimizationError::Solver(se) => map_solver_error(se),
        }
    }
}

/// Maps a `SolverError` to a `CliError::InternalError` (exit 4).
///
/// Per design §4: most solver errors should be caught by `validate_request`
/// before reaching the optimizer. Reaching this function indicates a validator
/// bug (for `InsufficientTerminals`, `BuildFailed("unknown source_type")`), or
/// a genuine engine failure for the remaining variants.
fn map_solver_error(e: SolverError) -> CliError {
    match e {
        // Should have been caught by validate_request (§2 rule 3).
        SolverError::NoFeasibleSolution => CliError::NoFeasibleSolution,
        // All other solver errors are treated as internal engine failures.
        SolverError::InsufficientTerminals => CliError::InternalError(
            "insufficient terminal nodes — validate_request should have caught this".into(),
        ),
        SolverError::BuildFailed(msg) => {
            CliError::InternalError(format!("solver build failed: {msg}"))
        }
        SolverError::NoOutlet => {
            CliError::InternalError("solver: no outlet found in network".into())
        }
        SolverError::NoSource => {
            CliError::InternalError("solver: no source found in network".into())
        }
        SolverError::NoRoute => {
            CliError::InternalError("solver: no route found in terrain graph".into())
        }
        SolverError::SteinerFailed(msg) => {
            CliError::InternalError(format!("solver: Steiner tree failed: {msg}"))
        }
    }
}

// ── WU-3: Mapping helpers ────────────────────────────────────────────────────
//
// Pure functions that convert a validated `DesignRequest` into the typed
// structs required by the optimizer and solver constructors. These are pub
// so integration tests can exercise them without spawning the optimizer.

/// Build a `TerrainModel` from the `terrain_points` of a `DesignRequest`.
///
/// Uses `TerrainModel::from_xyz_list` then `build_grid` with the default
/// resolution (20.0 m, from `SolverParams::default().grid_resolution`) per
/// design §5 and §9.
///
/// # Errors
///
/// Returns `Err(CliError::ValidationError(_))` if:
/// - `terrain_points` is empty or has fewer than 3 points
///   (`TerrainError::EmptyTerrain` / `TerrainError::InsufficientPoints`)
/// - `build_grid` fails (degenerate scatter geometry)
pub fn build_terrain_model(req: &DesignRequest) -> Result<TerrainModel, CliError> {
    // Convert PointXYZ list → [f64; 3] slice expected by TerrainModel.
    let xyz: Vec<[f64; 3]> = req.terrain_points.iter().map(|p| [p.x, p.y, p.z]).collect();

    let mut terrain = TerrainModel::from_xyz_list(&xyz).map_err(|e| {
        CliError::ValidationError(HydroTypesError::CrossFieldViolation {
            message: e.to_string(),
        })
    })?;

    // Default grid resolution from SolverParams; may be overridden in a future
    // spec revision that adds a `grid_resolution` override to DesignRequest.
    let resolution = req.grid_resolution; // default 20.0 from DesignRequest
    terrain.build_grid(resolution).map_err(|e| {
        CliError::ValidationError(HydroTypesError::CrossFieldViolation {
            message: e.to_string(),
        })
    })?;

    Ok(terrain)
}

/// Build an `OptimizationConfig` from a `DesignRequest` and an optional seed
/// override.
///
/// Effective seed resolution (design §3 canonical input, REQ-005):
/// `seed_override.unwrap_or(req.seed.unwrap_or(42))`
///
/// Fields mapped: `population_size`, `generations`, `max_time_seconds`,
/// `num_workers`, `enforce_cover_depth`, `enforce_clearance` (slope),
/// `enable_path_smoothing`, and the five objective weights.
/// All other fields keep `OptimizationConfig::default()` values.
pub fn build_optimization_config(
    req: &DesignRequest,
    seed_override: Option<u64>,
) -> OptimizationConfig {
    let effective_seed = seed_override.unwrap_or_else(|| req.seed.unwrap_or(42));

    OptimizationConfig {
        population_size: req.nsga_population_size,
        generations: req.nsga_generations,
        max_time_seconds: req.nsga_max_time_seconds as f64,
        num_workers: req.nsga_num_workers,
        seed: effective_seed,
        // Constraint enforcement flags from request.
        enforce_cover_depth: req.enforce_cover_depth,
        enforce_slope: req.enforce_segment_slopes,
        enforce_clearance: req.enforce_existing_clearance,
        // Path smoothing flag from request.
        enable_path_smoothing: req.enable_path_smoothing,
        // Objective weights from request.
        weight_cost: req.weight_cost,
        weight_excavation: req.weight_excavation,
        weight_pumping: req.weight_pumping,
        weight_interference: req.weight_interference,
        weight_resilience: req.weight_resilience,
        // All other fields use oracle defaults.
        ..OptimizationConfig::default()
    }
}

/// Build a `SolverParams` from a `DesignRequest`.
///
/// Starts from `SolverParams::default()` and overrides fields the request
/// carries. Per design §5, this is the "GA-driven params" struct passed to
/// every solver arm at dispatch time.
pub fn build_solver_params(req: &DesignRequest) -> SolverParams {
    SolverParams {
        design_flow: req.flow_per_service,
        grid_resolution: req.grid_resolution,
        source_head: req
            .source_head
            .unwrap_or(SolverParams::default().source_head),
        num_alternatives: req.num_alternatives,
        // All other topology / routing / network params keep defaults
        // until a future spec revision adds request fields for them.
        ..SolverParams::default()
    }
}

// ── run() helpers ───────────────────────────────────────────────────────────

/// Convert an `Option<PointXY>` reference to a `(f64, f64)` tuple.
///
/// Panics if `None` — callers must only call this after `validate_request`
/// has confirmed the field is `Some`.
fn xy(p: &PointXY) -> (f64, f64) {
    (p.x, p.y)
}

/// Resolve the canonical `PipeMaterial` enum from `req.material` (a pre-validated
/// canonical String stored by the serde deserializer).
///
/// `req.material` is resolved by `deserialize_material_str` at parse time,
/// so unknown strings are already rejected as exit 1 before reaching here.
/// This function should never fail in practice.
fn resolve_material(req: &DesignRequest) -> Result<PipeMaterial, CliError> {
    PipeMaterial::from_str_with_aliases(&req.material).ok_or_else(|| {
        CliError::InternalError(format!(
            "material '{}' passed validation but failed enum resolution — this is a bug",
            req.material
        ))
    })
}

/// Run the genetic optimizer for a single concrete solver, then assemble
/// a `DesignResult` from the Pareto results.
///
/// The optimizer is constructed, run, and the result is shaped here.
/// `audit_hash` is left empty (empty string); WU-8 fills it after run().
///
/// # Type parameters
/// `S` must implement `Solver + Send + Sync + 'static` per the optimizer API.
fn run_optimizer<S: Solver + Send + Sync + 'static>(
    solver: S,
    solver_type: SolverType,
    opt_config: OptimizationConfig,
    base_params: SolverParams,
    req: &DesignRequest,
    start: Instant,
) -> Result<DesignResult, CliError> {
    let mut optimizer = GeneticOptimizer::new(solver, solver_type, opt_config, base_params)
        .map_err(CliError::from)?;

    let pareto = optimizer.optimize().map_err(CliError::from)?;

    Ok(assemble_design_result(pareto, req, start))
}

/// Shape a `ParetoResults` into the public `DesignResult` type.
///
/// - Moves solutions from the Pareto front.
/// - Sets `best_solution` to `solutions[0]` (sorted by total_cost ascending
///   by the optimizer).
/// - `audit_hash` is intentionally left as empty string — WU-8 fills it.
fn assemble_design_result(
    pareto: hydro_optimizer::ParetoResults,
    req: &DesignRequest,
    start: Instant,
) -> DesignResult {
    let elapsed = start.elapsed().as_secs_f64();
    let success = !pareto.solutions.is_empty();
    let best_solution = pareto.solutions.first().cloned();

    // Copy diagnostics from the optimizer's Pareto output. audit_hash is
    // intentionally empty here — WU-8 will populate it post-optimize.
    let diagnostics = Diagnostics {
        audit_hash: String::new(), // WU-8 placeholder
        elapsed_seconds: elapsed,
        ..pareto.diagnostics
    };

    DesignResult {
        success,
        project_name: req.project_name.clone(),
        project_type: req.project_type.as_str().to_string(),
        elapsed_seconds: elapsed,
        seed: req.seed.map(|s| s as i64),
        norm: req.norm.clone(),
        best_solution,
        solutions: pareto.solutions,
        diagnostics,
    }
}

// ── Public run() ─────────────────────────────────────────────────────────────

/// Run the full optimization pipeline for the given `DesignRequest`.
///
/// # Parameters
/// - `req` — fully-deserialised design request; all fields present per the
///   hydro-types schema.
/// - `seed_override` — when `Some(s)`, overrides `req.seed` before passing to
///   the optimizer. Used by `--seed` CLI flag and deterministic test fixtures.
///
/// # Returns
/// - `Ok(DesignResult)` on success — `solutions` non-empty, `success == true`.
/// - `Err(CliError)` — see [`CliError`] exit-code table.
///
/// # Implementation note (WU-4 / WU-5)
/// WU-4 implemented the `sewer` dispatch arm. WU-5 implements the remaining
/// 5 project_types: `water_supply`, `conveyance`, `distribution`,
/// `pump_station`, and `intake`. All 6 arms are now live.
pub fn run(req: DesignRequest, seed_override: Option<u64>) -> Result<DesignResult, CliError> {
    let start = Instant::now();

    // REQ-002: structural + solver-specific validation first.
    validate_request(&req)?;

    // Build shared optimizer inputs from the validated request.
    let opt_config = build_optimization_config(&req, seed_override);
    let base_params = build_solver_params(&req);

    match req.project_type.as_str() {
        // ── WU-4: Sewer dispatch ──────────────────────────────────────────
        //
        // REQ-004: monomorphized arm — no Box<dyn Solver>.
        // TerrainModel is built here (not in build_terrain_model) so that
        // the sewer arm owns it directly without an extra clone.
        "sewer" => {
            let terrain = build_terrain_model(&req)?;
            let constraints = req.effective_constraints();
            let material = resolve_material(&req)?;

            let service_pts: Vec<(f64, f64)> = req
                .service_points
                .as_ref()
                .unwrap() // validated: Some, non-empty
                .iter()
                .map(xy)
                .collect();
            let outlet = xy(req.outlet.as_ref().unwrap()); // validated: Some

            let solver = SewerSolver::new(
                terrain,
                constraints,
                service_pts,
                outlet,
                base_params.design_flow, // flow_per_service
                material,
                req.project_name.clone(),
            );

            run_optimizer(
                solver,
                SolverType::Sewer,
                opt_config,
                base_params,
                &req,
                start,
            )
        }

        // ── WU-5: WaterSupply dispatch ────────────────────────────────────
        //
        // REQ-004: monomorphized arm — no Box<dyn Solver>.
        // service_points are treated as demand_points (design §2 table).
        // demand_per_node falls back to base_params.design_flow (design §2.5).
        "water_supply" => {
            let terrain = build_terrain_model(&req)?;
            let constraints = req.effective_constraints();
            let material = resolve_material(&req)?;

            let demand_pts: Vec<(f64, f64)> = req
                .service_points
                .as_ref()
                .unwrap() // validated: Some, non-empty
                .iter()
                .map(xy)
                .collect();
            let source = xy(req.source.as_ref().unwrap()); // validated: Some

            let solver = WaterSupplySolver::new(
                terrain,
                constraints,
                demand_pts,
                source,
                base_params.design_flow, // demand_per_node — design §2.5
                material,
                req.project_name.clone(),
            );

            run_optimizer(
                solver,
                SolverType::WaterSupply,
                opt_config,
                base_params,
                &req,
                start,
            )
        }

        // ── WU-5: Conveyance dispatch ─────────────────────────────────────
        //
        // outlet is reused as destination (design §2 table, ConveyanceSolver signature).
        "conveyance" => {
            let terrain = build_terrain_model(&req)?;
            let constraints = req.effective_constraints();
            let material = resolve_material(&req)?;

            let source = xy(req.source.as_ref().unwrap()); // validated: Some
            let destination = xy(req.outlet.as_ref().unwrap()); // validated: Some (outlet = destination)

            let solver = ConveyanceSolver::new(
                terrain,
                constraints,
                source,
                destination,
                material,
                req.project_name.clone(),
            );

            run_optimizer(
                solver,
                SolverType::Conveyance,
                opt_config,
                base_params,
                &req,
                start,
            )
        }

        // ── WU-5: Distribution dispatch ───────────────────────────────────
        //
        // service_points are treated as demand_points (design §2 table).
        // validate_request already enforces len() >= 2 (design §2 rule 3).
        "distribution" => {
            let terrain = build_terrain_model(&req)?;
            let constraints = req.effective_constraints();
            let material = resolve_material(&req)?;

            let demand_pts: Vec<(f64, f64)> = req
                .service_points
                .as_ref()
                .unwrap() // validated: Some, len >= 2
                .iter()
                .map(xy)
                .collect();
            let source = xy(req.source.as_ref().unwrap()); // validated: Some

            let solver = DistributionSolver::new(
                terrain,
                constraints,
                demand_pts,
                source,
                base_params.design_flow, // demand_per_node — design §2.5
                material,
                req.project_name.clone(),
            );

            run_optimizer(
                solver,
                SolverType::Distribution,
                opt_config,
                base_params,
                &req,
                start,
            )
        }

        // ── WU-5: PumpStation dispatch ────────────────────────────────────
        //
        // PumpStationSolver takes Option<TerrainModel> — we pass Some(terrain) for
        // structural consistency even though the solver never reads it algorithmically.
        // Elevations are derived from terrain.elevation_at() (design §2.5, §9).
        // TerrainModel::elevation_at() is infallible (returns f64, never Err).
        // Pipe lengths are derived from Euclidean distance between source and outlet XY.
        //
        // Hardcoded v1 defaults (design §2.5 conservative defaults table):
        //   suction_diameter    = 0.20 m  — matches smoke test default diameter range
        //   discharge_diameter  = 0.20 m  — same (PUMP_DIAMETERS: 0.10–0.40 m)
        //   wet_well_factor     = 1.0     — Phase 6.5 smoke test: matches oracle (1.0)
        //   retention_minutes   = 5.0     — Phase 6.5 smoke test: matches oracle (5.0)
        //   num_pumps           = None    — let num_alternatives drive the range
        //   suction_d_idx       = None    — no diameter-index override
        //   discharge_d_idx     = None    — no diameter-index override
        "pump_station" => {
            let terrain = build_terrain_model(&req)?;
            let constraints = req.effective_constraints();
            let material = resolve_material(&req)?;

            let src_xy = xy(req.source.as_ref().unwrap()); // validated: Some
            let dst_xy = xy(req.outlet.as_ref().unwrap()); // validated: Some

            // Elevation lookup — infallible; falls back to nearest-neighbor if outside bounds.
            let suction_elevation = terrain.elevation_at(src_xy.0, src_xy.1);
            let discharge_elevation = terrain.elevation_at(dst_xy.0, dst_xy.1);

            // Pipe lengths: Euclidean distance between source and outlet XY (design §2.5).
            // Used for both suction and discharge sides as a conservative v1 approximation.
            let pipe_length = {
                let dx = dst_xy.0 - src_xy.0;
                let dy = dst_xy.1 - src_xy.1;
                (dx * dx + dy * dy).sqrt().max(1.0) // clamp to >= 1.0 m to avoid zero-length
            };

            let solver = PumpStationSolver::new(
                Some(terrain),
                constraints,
                suction_elevation,
                discharge_elevation,
                pipe_length, // suction_pipe_length (design §2.5: Euclidean distance)
                pipe_length, // discharge_pipe_length (same approximation)
                0.20,        // suction_diameter (m) — v1 default, PUMP_DIAMETERS range
                0.20,        // discharge_diameter (m) — v1 default, PUMP_DIAMETERS range
                material,
                1.0,  // wet_well_factor — Phase 6.5 smoke test value
                5.0,  // retention_minutes — Phase 6.5 smoke test value
                None, // num_pumps — let num_alternatives drive the range
                None, // suction_d_idx — no diameter-index override
                None, // discharge_d_idx — no diameter-index override
            );

            run_optimizer(
                solver,
                SolverType::PumpStation,
                opt_config,
                base_params,
                &req,
                start,
            )
        }

        // ── WU-5: Intake dispatch ─────────────────────────────────────────
        //
        // IntakeSolver takes Option<TerrainModel> — pass Some(terrain) for consistency.
        // source_elevation is derived from terrain.elevation_at(source_xy) (infallible).
        // pipe_elevation = source_elevation - 2.0 m (1–2 m below ground, design §5 placeholder).
        //
        // source_type defaults to "river" (CLI v1 constraint, design §2 table, §2.7 note).
        // Valid values: "river", "lake", "spring", "groundwater", "canal", "reservoir".
        // validate_request prevalidates; reaching here means "river" is always safe.
        //
        // Hardcoded v1 channel/screen constants (design §2.5 conservative defaults table,
        // Phase 6.5 smoke test values from solver_wiring_smoke.rs):
        //   channel_slope        = 0.001   — mild slope; IntakeGolden uses 0.005 but 0.001 is safer
        //   channel_width_factor = 1.2     — slight width margin over critical flow width
        //   channel_slope_factor = 1.0     — no slope correction
        //   weir_type            = 1       — rectangular weir (0 = v-notch per solver code)
        //   screen_velocity_factor= 1.5    — Phase 6.5 smoke uses f.solver_params.screen_velocity_factor (1.0)
        //   screen_velocity      = 0.15 m/s — Phase 6.5 smoke test hardcoded default
        //   bar_spacing          = 0.025 m  — Phase 6.5 smoke test hardcoded default
        //   bar_thickness        = 0.010 m  — Phase 6.5 smoke test hardcoded default
        //   channel_roughness    = 0.015    — Phase 6.5 smoke test hardcoded default (Manning n)
        "intake" => {
            let terrain = build_terrain_model(&req)?;
            let constraints = req.effective_constraints();
            let material = resolve_material(&req)?;

            let src_xy = xy(req.source.as_ref().unwrap()); // validated: Some

            // Elevation lookup — infallible; falls back to nearest-neighbor if outside bounds.
            let source_elevation = terrain.elevation_at(src_xy.0, src_xy.1);
            // pipe_elevation: 2 m below source surface — conservative v1 placeholder (design §5).
            let pipe_elevation = source_elevation - 2.0;

            let solver = IntakeSolver::new(
                Some(terrain),
                constraints,
                "river".to_string(), // source_type — CLI v1 default (design §2.7, VALID_SOURCE_TYPES)
                source_elevation,
                pipe_elevation, // 2 m below source surface (v1 placeholder, design §5)
                0.001,          // channel_slope — mild slope v1 default
                material,
                1.2, // channel_width_factor — slight width margin
                1.0, // channel_slope_factor — no slope correction
                1,   // weir_type — 1 = rectangular (design §5; IntakeGolden weir_type=0, but
                //              intake tests use weir_type from fixture via smoke; CLI uses 1
                //              as a conservative default matching design §5 skeleton)
                1.5,   // screen_velocity_factor — Phase 6.5 smoke: 1.0; CLI uses 1.5 (margin)
                0.15,  // screen_velocity (m/s) — Phase 6.5 smoke hardcoded default
                0.025, // bar_spacing (m) — Phase 6.5 smoke hardcoded default
                0.010, // bar_thickness (m) — Phase 6.5 smoke hardcoded default
                0.015, // channel_roughness (Manning n) — Phase 6.5 smoke hardcoded default
            );

            run_optimizer(
                solver,
                SolverType::Intake,
                opt_config,
                base_params,
                &req,
                start,
            )
        }

        // Unknown types are caught by validate_request above; this arm is
        // unreachable in normal operation.
        other => Err(CliError::InternalError(format!(
            "unknown project_type: {other}"
        ))),
    }
}

/// Validate a `DesignRequest` without running the optimizer.
///
/// 1. Calls `req.validate()` from hydro-types (range + cross-field checks).
/// 2. Performs per-`project_type` `Option<_>` presence checks per design §2.
///
/// The rules implemented here are the CLI pre-validation layer described in
/// design §2, rules 2–4. They surface errors as exit 1 (`ValidationError`)
/// rather than letting them reach the optimizer as exit 4 internal failures.
///
/// # Returns
/// - `Ok(())` — all checks pass.
/// - `Err(CliError::ValidationError(_))` — first failing check.
pub fn validate_request(req: &DesignRequest) -> Result<(), CliError> {
    // Rule 1: hydro-types range + cross-field validation.
    req.validate().map_err(CliError::from)?;

    // Rules 2–4: per-project_type required `Option<_>` presence checks.
    match req.project_type.as_str() {
        // Sewer requires outlet (discharge point) and at least one service point.
        "sewer" => {
            if req.outlet.is_none() {
                return Err(CliError::ValidationError(
                    HydroTypesError::MissingRequired { field: "outlet" },
                ));
            }
            match &req.service_points {
                None => {
                    return Err(CliError::ValidationError(
                        HydroTypesError::MissingRequired {
                            field: "service_points",
                        },
                    ))
                }
                Some(pts) if pts.is_empty() => {
                    return Err(CliError::ValidationError(
                        HydroTypesError::MissingRequired {
                            field: "service_points",
                        },
                    ))
                }
                _ => {}
            }
        }

        // WaterSupply requires source (supply node) and at least one demand point.
        "water_supply" => {
            if req.source.is_none() {
                return Err(CliError::ValidationError(
                    HydroTypesError::MissingRequired { field: "source" },
                ));
            }
            match &req.service_points {
                None => {
                    return Err(CliError::ValidationError(
                        HydroTypesError::MissingRequired {
                            field: "service_points",
                        },
                    ))
                }
                Some(pts) if pts.is_empty() => {
                    return Err(CliError::ValidationError(
                        HydroTypesError::MissingRequired {
                            field: "service_points",
                        },
                    ))
                }
                _ => {}
            }
        }

        // Conveyance requires source (origin) and outlet (destination).
        // `outlet` is reused as the destination field for ConveyanceSolver (design §2 table).
        "conveyance" => {
            if req.source.is_none() {
                return Err(CliError::ValidationError(
                    HydroTypesError::MissingRequired { field: "source" },
                ));
            }
            if req.outlet.is_none() {
                return Err(CliError::ValidationError(
                    HydroTypesError::MissingRequired { field: "outlet" },
                ));
            }
        }

        // Distribution requires source and >= 2 demand points.
        // Design §2 rule 3: < 2 points must surface as exit 1, not exit 4
        // (SolverError::InsufficientTerminals). Pre-validate in CLI.
        "distribution" => {
            if req.source.is_none() {
                return Err(CliError::ValidationError(
                    HydroTypesError::MissingRequired { field: "source" },
                ));
            }
            match &req.service_points {
                None => {
                    return Err(CliError::ValidationError(
                        HydroTypesError::MissingRequired {
                            field: "service_points",
                        },
                    ))
                }
                Some(pts) if pts.len() < 2 => {
                    return Err(CliError::ValidationError(
                        HydroTypesError::CrossFieldViolation {
                            message: "distribution requires >= 2 demand points".into(),
                        },
                    ))
                }
                _ => {}
            }
        }

        // PumpStation requires source (suction XY) and outlet (discharge XY).
        // Elevations and pipe lengths are derived from TerrainModel at dispatch time.
        "pump_station" => {
            if req.source.is_none() {
                return Err(CliError::ValidationError(
                    HydroTypesError::MissingRequired { field: "source" },
                ));
            }
            if req.outlet.is_none() {
                return Err(CliError::ValidationError(
                    HydroTypesError::MissingRequired { field: "outlet" },
                ));
            }
        }

        // Intake requires source (intake XY). source_type defaults to "river" in v1
        // (DesignRequest carries no source_type field). Design §2 table, §2.7 note.
        "intake" => {
            if req.source.is_none() {
                return Err(CliError::ValidationError(
                    HydroTypesError::MissingRequired { field: "source" },
                ));
            }
        }

        // ProjectTypeStr deserialization already rejects unknown project types,
        // so this arm is unreachable in practice. It is retained as a defensive guard.
        other => {
            return Err(CliError::ValidationError(
                HydroTypesError::CrossFieldViolation {
                    message: format!("unknown project_type: {other}"),
                },
            ))
        }
    }

    Ok(())
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    #[test]
    fn lib_skeleton_compiles() {
        // Verifies the crate compiles with the [lib] target present.
        // The test itself is a no-op; compilation success is the assertion.
    }
}
