//! T-5.1.a — MockSolver implements the Solver trait.
//!
//! RED test: fails until SolverError, SolverParams, and the Solver trait exist.

use hydro_solvers::{Solver, SolverError, SolverParams};
use hydro_types::{DesignConstraints, PipeNetwork, Solution, SolutionScore};

// ── MockSolver ────────────────────────────────────────────────────────────────

struct MockSolver {
    constraints: DesignConstraints,
}

impl MockSolver {
    fn new() -> Self {
        MockSolver {
            constraints: DesignConstraints::default(),
        }
    }
}

impl Solver for MockSolver {
    fn solve(&mut self, _params: &SolverParams) -> Result<Vec<Solution>, SolverError> {
        Ok(vec![])
    }

    fn evaluate(&self, _network: &PipeNetwork) -> Result<SolutionScore, SolverError> {
        Ok(SolutionScore::default())
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[test]
fn mock_solver_implements_trait() {
    let mut solver = MockSolver::new();
    let params = SolverParams::default();
    let solutions = solver
        .solve(&params)
        .expect("solve must not error on MockSolver");
    assert!(
        solutions.is_empty(),
        "MockSolver returns empty solutions by design"
    );
    // Constraints field is accessible (ensures the struct is usable)
    assert!((solver.constraints.min_slope - 0.005).abs() < 1e-12);
}

#[test]
fn mock_solver_evaluate_returns_default_score() {
    let solver = MockSolver::new();
    let nodes = vec![];
    let pipes = vec![];
    let net = PipeNetwork::new("test", nodes, pipes);
    let score = solver
        .evaluate(&net)
        .expect("evaluate must not error on MockSolver");
    assert_eq!(score.total_cost, 0.0);
    assert_eq!(score.norm_violations, 0);
}

#[test]
fn solver_error_variants_are_constructible() {
    let _e1 = SolverError::NoOutlet;
    let _e2 = SolverError::NoSource;
    let _e3 = SolverError::InsufficientTerminals;
    let _e4 = SolverError::NoRoute;
    let _e5 = SolverError::NoFeasibleSolution;
    let _e6 = SolverError::SteinerFailed("test".to_string());
    let _e7 = SolverError::BuildFailed("test".to_string());
}

#[test]
fn solver_params_default_is_sane() {
    let p = SolverParams::default();
    assert_eq!(p.route_variant, 0);
    assert!((p.slope_factor - 1.0).abs() < 1e-12);
    assert!((p.cover_factor - 1.0).abs() < 1e-12);
    assert_eq!(p.diameter_offset, 0);
    assert!(p.manhole_spacing.is_none());
}
