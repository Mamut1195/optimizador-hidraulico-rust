//! hydro-solvers — Solver trait and the six domain-specific solvers.
//!
//! Implements the `Solver` trait and concrete solvers:
//! - SewerSolver (PR-6): gravity sewer networks
//! - WaterSupplySolver, ConveyanceSolver (PR-7a): pressure systems
//! - DistributionSolver, PumpStationSolver, IntakeSolver (PR-7b): remaining solvers
//!
//! Each solver is an orchestration layer on top of hydro-hydraulics,
//! hydro-terrain, and hydro-norms. No direct I/O.

pub mod conveyance;
pub mod error;
pub mod sewer;
pub mod solver;
pub mod water_supply;

pub use conveyance::ConveyanceSolver;
pub use error::SolverError;
pub use sewer::SewerSolver;
pub use solver::{Solver, SolverParams};
pub use water_supply::WaterSupplySolver;

#[cfg(test)]
mod tests {
    #[test]
    fn workspace_member_compiles() {
        // The test passing confirms compilation succeeded.
    }
}
