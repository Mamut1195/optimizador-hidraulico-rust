//! Solver trait and SolverParams — the common interface for all hydraulic solvers.
//!
//! Mirrors Python `BaseSolver` ABC from `hydro_engine/solvers/base.py`.
//! Every concrete solver (Sewer, WaterSupply, …) implements this trait.

use hydro_types::{PipeNetwork, Solution, SolutionScore};

use crate::error::SolverError;

// ── SolverParams ──────────────────────────────────────────────────────────────

/// Flat parameter struct carrying all optional genetic and design parameters.
///
/// Mirrors the kwargs accepted by `SewerSolver.solve()` and other solver
/// solve() methods in the Python oracle. All fields are optional with defaults
/// matching the Python oracle defaults.
#[derive(Debug, Clone)]
pub struct SolverParams {
    // ── Topology / routing ────────────────────────────────────────────────
    /// Topology variant (0 = optimal Steiner, 1+ = perturbed weights).
    pub route_variant: u32,

    // ── Genetic / design parameters ───────────────────────────────────────
    /// Multiplier for natural terrain slope (0.5-2.0). Default: 1.0.
    pub slope_factor: f64,
    /// Multiplier for minimum cover depth (1.0-1.5). Default: 1.0.
    pub cover_factor: f64,
    /// Steps above minimum required diameter (0-3). Default: 0.
    pub diameter_offset: i32,
    /// Max distance between manholes (m). None = keep all nodes. Default: None.
    pub manhole_spacing: Option<f64>,

    // ── Network parameters ────────────────────────────────────────────────
    /// Network type (1 = branched, 2 = looped). Default: 1.
    pub network_type: u32,
    /// Loop density for distribution networks (0.0-1.0). Default: 0.5.
    pub loop_density: f64,
    /// Mesh density for distribution networks (0.0-1.0). Default: 0.5.
    pub mesh_density: f64,
    /// Valve spacing in meters. Default: 500.0.
    pub valve_spacing: f64,
    /// Fire hydrant spacing in meters. Default: 200.0.
    pub hydrant_spacing: f64,

    // ── Hydraulic parameters ──────────────────────────────────────────────
    /// Source head (pressure head at source, mca). Default: 30.0.
    pub source_head: f64,
    /// Design flow (m³/s). Default: 0.002.
    pub design_flow: f64,

    // ── Solver options ────────────────────────────────────────────────────
    /// Number of alternative designs to generate. Default: 3.
    pub num_alternatives: u32,

    // ── Grid ─────────────────────────────────────────────────────────────
    /// Grid resolution for the terrain graph (m). Default: 20.0.
    pub grid_resolution: f64,
}

impl Default for SolverParams {
    fn default() -> Self {
        SolverParams {
            route_variant: 0,
            slope_factor: 1.0,
            cover_factor: 1.0,
            diameter_offset: 0,
            manhole_spacing: None,
            network_type: 1,
            loop_density: 0.5,
            mesh_density: 0.5,
            valve_spacing: 500.0,
            hydrant_spacing: 200.0,
            source_head: 30.0,
            design_flow: 0.002,
            num_alternatives: 3,
            grid_resolution: 20.0,
        }
    }
}

// ── Solver trait ──────────────────────────────────────────────────────────────

/// Common interface for all hydraulic solvers.
///
/// Mirrors `BaseSolver` ABC from `hydro_engine/solvers/base.py`:
/// - `solve()` generates ranked design alternatives.
/// - `evaluate()` scores a completed network.
///
/// Each concrete solver (SewerSolver, WaterSupplySolver, …) implements this
/// trait with project-type-specific build/route/dimension/score logic.
pub trait Solver {
    /// Generate one or more ranked design alternatives.
    ///
    /// The `params` struct carries all genetic and design parameters.
    /// Returns a Vec of Solutions sorted by total_cost ascending (best first).
    fn solve(&mut self, params: &SolverParams) -> Result<Vec<Solution>, SolverError>;

    /// Evaluate a pre-built network and return its score.
    ///
    /// Used both internally (inside solve) and in the oracle fixture tests
    /// (evaluate-only sub-fixture: reconstruct network → evaluate → assert parity).
    fn evaluate(&self, network: &PipeNetwork) -> Result<SolutionScore, SolverError>;
}
