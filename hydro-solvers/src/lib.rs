//! hydro-solvers — Solver trait and the six domain-specific solvers.
//!
//! Implements the `Solver` trait and six concrete solvers:
//! SewerSolver, WaterSupplySolver, ConveyanceSolver, DistributionSolver,
//! PumpStationSolver, IntakeSolver.
//!
//! Each solver is an orchestration layer on top of hydro-hydraulics, hydro-terrain,
//! and hydro-norms. No direct I/O.

#[cfg(test)]
mod tests {
    #[test]
    fn workspace_member_compiles() {
        // The test passing confirms compilation succeeded.
    }
}
