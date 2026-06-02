//! hydro-cli library — public API for the JSON-in / JSON-out engine shell.
//!
//! # Public surface (REQ-001, REQ-010)
//!
//! - [`CliError`] — typed error enum with [`CliError::exit_code`] (REQ-003)
//! - [`run`] — full optimization dispatch stub (implemented in WU-4/5)
//! - [`validate_request`] — structural + solver-specific validation stub (WU-2)
//!
//! All items are re-exported at the crate root so integration tests can call
//! them directly without spawning a subprocess (REQ-010).

use hydro_optimizer::{OptimizationConfig, OptimizationError};
use hydro_solvers::{SolverError, SolverParams};
use hydro_terrain::TerrainModel;
use hydro_types::{error::HydroTypesError, request::DesignRequest, response::DesignResult};

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

// ── Public API stubs ────────────────────────────────────────────────────────
//
// Bodies are `todo!()` for WU-1. Signatures are locked per REQ-001.
// Implementation grows across WU-2 (validate_request) through WU-8 (audit_hash).

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
/// # Implementation note (WU-1)
/// Body is a stub (`todo!()`). Dispatch, terrain build, config mapping, and
/// audit-hash computation are added in WU-3 through WU-8.
pub fn run(_req: DesignRequest, _seed_override: Option<u64>) -> Result<DesignResult, CliError> {
    todo!("run() is implemented in WU-4 (sewer dispatch) through WU-5 (remaining solvers)")
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
