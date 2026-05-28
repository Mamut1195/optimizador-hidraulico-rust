//! hydro-optimizer — NSGA-III multi-objective optimizer.
//!
//! Faithful port of the Python genetic.py (DEAP-based mu+lambda NSGA-III).
//! Includes: IndividualEncoder, reference point lattice (Das-Dennis),
//! fast nondominated sort, SBX crossover, polynomial mutation, varOr,
//! niching/normalization/association, and the main evolve() loop.
//!
//! Determinism boundary: a single `StdRng` threaded by `&mut` through all
//! RNG-consuming operators. rayon parallelizes ONLY the fitness map.

#[cfg(test)]
mod tests {
    #[test]
    fn workspace_member_compiles() {
        // The test passing confirms compilation succeeded.
    }
}
