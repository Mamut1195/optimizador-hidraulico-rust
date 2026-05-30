//! SewerSolver — placeholder stub; full implementation follows in T-5.2.
//!
//! This file will be replaced by the full SewerSolver implementation.
//! Exists only to satisfy the `pub use sewer::SewerSolver;` re-export in lib.rs
//! during T-5.1 scaffolding.

use hydro_types::{DesignConstraints, PipeNetwork, Solution, SolutionScore};
use hydro_terrain::TerrainModel;

use crate::error::SolverError;
use crate::solver::{Solver, SolverParams};

/// Stub for T-5.1 scaffolding. Full implementation in T-5.2.
pub struct SewerSolver {
    pub terrain: TerrainModel,
    pub constraints: DesignConstraints,
}

impl SewerSolver {
    pub fn new(terrain: TerrainModel, constraints: DesignConstraints) -> Self {
        SewerSolver { terrain, constraints }
    }
}

impl Solver for SewerSolver {
    fn solve(&mut self, _params: &SolverParams) -> Result<Vec<Solution>, SolverError> {
        Err(SolverError::BuildFailed("SewerSolver not yet implemented".to_string()))
    }

    fn evaluate(&self, _network: &PipeNetwork) -> Result<SolutionScore, SolverError> {
        Err(SolverError::BuildFailed("SewerSolver not yet implemented".to_string()))
    }
}
