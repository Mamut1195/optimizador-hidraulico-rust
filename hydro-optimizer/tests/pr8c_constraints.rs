//! PR-8c integration tests — ConstraintChecker (REQ-004, REQ-018).
//!
//! Strict TDD: tests written FIRST (RED), implementation follows (GREEN).
//!
//! # Spec formula (from REQ-004 and tasks #578)
//! penalty = 1e12 + violation_magnitude * 1e9
//!
//! NOTE: The explore §4 formula `(1e12 + dist) * 5` was found inconsistent
//! with the spec. The spec formula above is the resolution per tasks #578.
//!
//! # Oracle mapping (constraints.py)
//! - `is_feasible` → gene-level bounds check (fast pre-decode)
//! - `distance_to_feasible` → sum of per-gene bound violations
//! - `check_solution_feasibility` → post-decode geometric checks:
//!   forbidden zones, mandatory routes, bend angles, cover depth,
//!   existing clearance, segment slopes
//! - `check_route_feasibility` → route-level forbidden zone check
//! - `check_point_feasibility` → point-level forbidden zone check

#[allow(unused_imports)]
use hydro_optimizer::{
    ConstraintChecker, ConstraintReport, ConstraintViolation, ForbiddenZone, MandatoryRoute,
};
use hydro_types::enums::{NodeType, PipeMaterial};
use hydro_types::network::{NetworkNode, NetworkPipe, PipeId, PipeNetwork};
use hydro_types::response::{Solution, SolutionScore};

// ── helpers ───────────────────────────────────────────────────────────────────

fn make_solution(nodes: Vec<NetworkNode>, pipes: Vec<NetworkPipe>) -> Solution {
    Solution {
        rank: 1,
        network: PipeNetwork::new("test", nodes, pipes),
        score: SolutionScore::default(),
        metadata: Default::default(),
    }
}

fn pipe(id: &str, s: &str, e: &str, length: f64) -> NetworkPipe {
    NetworkPipe {
        id: PipeId::new(id),
        start_node_id: id_of(s),
        end_node_id: id_of(e),
        length,
        effective_length: length,
        diameter: 0.3,
        material: PipeMaterial::Pvc,
        roughness: 0.009,
        start_invert: 0.0,
        end_invert: 0.0,
        slope: 0.0,
        design_flow: 0.0,
        waypoints: vec![],
        metadata: Default::default(),
    }
}

fn node(id: &str, x: f64, y: f64, z: f64) -> NetworkNode {
    NetworkNode::new(id, x, y, z, NodeType::Manhole)
}

fn id_of(s: &str) -> hydro_types::network::NodeId {
    hydro_types::network::NodeId::new(s)
}

// ── COMMIT 1 — ConstraintReport + ConstraintViolation + spec penalty formula ──

/// REQ-004: A fully feasible candidate has no violations and penalty == 0.0.
#[test]
fn test_feasible_candidate_no_penalty() {
    let checker = ConstraintChecker::new_bounds_only(
        // 2 genes: [0.0..10.0, 0.0..5.0]
        vec![0.0, 0.0],
        vec![10.0, 5.0],
    );
    let individual = vec![5.0, 2.5]; // within bounds
    let report = checker.check_bounds(&individual);
    assert!(report.feasible, "expected feasible: true");
    assert!(report.violations.is_empty(), "expected no violations");
    assert_eq!(
        report.penalty, 0.0,
        "feasible candidate must have penalty 0.0"
    );
}

/// REQ-004: A candidate with one constraint violated by magnitude m
/// has penalty == 1e12 + m * 1e9 (spec formula, hard-coded tolerance).
#[test]
fn test_single_violation_spec_penalty_formula() {
    let checker = ConstraintChecker::new_bounds_only(vec![0.0, 0.0], vec![10.0, 5.0]);
    // gene[1] exceeds upper bound by 2.0 → violation_magnitude = 2.0
    let individual = vec![5.0, 7.0];
    let report = checker.check_bounds(&individual);
    assert!(!report.feasible, "expected feasible: false");
    assert_eq!(report.violations.len(), 1, "expected exactly 1 violation");

    let m = 2.0_f64;
    let expected_penalty = 1e12 + m * 1e9;
    assert!(
        (report.penalty - expected_penalty).abs() < 1.0,
        "penalty should be {expected_penalty} (spec formula 1e12 + m*1e9), got {}",
        report.penalty
    );
}

/// REQ-004: Multiple violations — penalty sums per-violation per spec.
/// Spec says: penalty = sum over violations of (1e12 + magnitude * 1e9).
#[test]
fn test_multiple_violations_penalty_sums() {
    let checker = ConstraintChecker::new_bounds_only(vec![0.0, 0.0, 0.0], vec![5.0, 5.0, 5.0]);
    // gene[0]: below lb by 1.0, gene[2]: above ub by 3.0
    let individual = vec![-1.0, 2.5, 8.0];
    let report = checker.check_bounds(&individual);
    assert!(!report.feasible);
    assert_eq!(report.violations.len(), 2, "expected 2 violations");

    // Per-violation penalty summation
    let total: f64 = report
        .violations
        .iter()
        .map(|v| 1e12 + v.magnitude * 1e9)
        .sum();
    assert!(
        (report.penalty - total).abs() < 1.0,
        "penalty should equal sum of per-violation (1e12 + m*1e9): expected {total}, got {}",
        report.penalty
    );
}

/// Constraint violation carries the magnitude field correctly.
#[test]
fn test_violation_magnitude_is_correct() {
    let checker = ConstraintChecker::new_bounds_only(vec![0.0], vec![10.0]);
    // gene exceeds upper by exactly 3.5
    let individual = vec![13.5];
    let report = checker.check_bounds(&individual);
    assert_eq!(report.violations.len(), 1);
    assert!(
        (report.violations[0].magnitude - 3.5).abs() < 1e-10,
        "magnitude should be 3.5, got {}",
        report.violations[0].magnitude
    );
}

// ── COMMIT 2 — Per-constraint checks ─────────────────────────────────────────

// ── 2a: gene-level bounds ─────────────────────────────────────────────────────

/// Oracle: `is_feasible` with bounds — gene below lower bound is infeasible.
#[test]
fn test_bounds_below_lower_bound_infeasible() {
    let checker = ConstraintChecker::new_bounds_only(vec![1.0], vec![5.0]);
    let report = checker.check_bounds(&[0.0]); // 0.0 < 1.0
    assert!(!report.feasible);
    assert_eq!(report.violations.len(), 1);
    assert!(report.violations[0].magnitude > 0.0);
}

/// Oracle: `is_feasible` — gene above upper bound is infeasible.
#[test]
fn test_bounds_above_upper_bound_infeasible() {
    let checker = ConstraintChecker::new_bounds_only(vec![0.0], vec![3.0]);
    let report = checker.check_bounds(&[5.0]); // 5.0 > 3.0
    assert!(!report.feasible);
}

/// Oracle: `is_feasible` — gene exactly at bounds is feasible (0.001 tolerance).
#[test]
fn test_bounds_exact_boundary_feasible() {
    let checker = ConstraintChecker::new_bounds_only(vec![0.0], vec![3.0]);
    let report = checker.check_bounds(&[3.0]);
    assert!(report.feasible, "exact upper bound should be feasible");
    let report2 = checker.check_bounds(&[0.0]);
    assert!(report2.feasible, "exact lower bound should be feasible");
}

/// Oracle: `distance_to_feasible` — no bounds → distance = 0.0.
#[test]
fn test_bounds_no_bounds_always_feasible() {
    let checker = ConstraintChecker::new_no_bounds();
    let report = checker.check_bounds(&[999.0, -999.0]);
    assert!(report.feasible);
    assert!(report.violations.is_empty());
    assert_eq!(report.penalty, 0.0);
}

// ── 2b: forbidden zones ───────────────────────────────────────────────────────

/// Oracle line 189: a pipe route intersecting a forbidden polygon is infeasible.
#[test]
fn test_forbidden_zone_pipe_intersects_is_infeasible() {
    // Forbidden square: (0,0)→(10,10)
    let zone = ForbiddenZone {
        vertices: vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]],
        label: None,
    };
    let checker = ConstraintChecker::new_geometric(vec![zone], vec![], None);

    // Pipe from (-5, 5) to (15, 5) — crosses the forbidden square
    let route = vec![(-5.0_f64, 5.0_f64), (15.0, 5.0)];
    let report = checker.check_route(&route);
    assert!(
        !report.feasible,
        "route crossing forbidden zone should be infeasible"
    );
    assert!(report
        .violations
        .iter()
        .any(|v| v.kind == ConstraintViolationKind::ForbiddenZone));
    assert!(report.penalty > 0.0);
}

/// A pipe route that does NOT intersect any forbidden zone is feasible.
#[test]
fn test_forbidden_zone_pipe_clear_is_feasible() {
    let zone = ForbiddenZone {
        vertices: vec![[0.0, 0.0], [5.0, 0.0], [5.0, 5.0], [0.0, 5.0]],
        label: None,
    };
    let checker = ConstraintChecker::new_geometric(vec![zone], vec![], None);

    // Pipe from (10,0) to (20,0) — entirely outside the square
    let route = vec![(10.0_f64, 0.0_f64), (20.0, 0.0)];
    let report = checker.check_route(&route);
    assert!(report.feasible);
    assert!(report.violations.is_empty());
    assert_eq!(report.penalty, 0.0);
}

/// No forbidden zones → any route is feasible.
#[test]
fn test_no_forbidden_zones_always_feasible() {
    let checker = ConstraintChecker::new_geometric(vec![], vec![], None);
    let route = vec![(0.0_f64, 0.0_f64), (100.0, 100.0)];
    let report = checker.check_route(&route);
    assert!(report.feasible);
}

// ── 2c: mandatory routes ──────────────────────────────────────────────────────

/// Oracle line 196: mandatory route satisfied when solution passes through it.
#[test]
fn test_mandatory_route_satisfied_is_feasible() {
    let mandatory = MandatoryRoute {
        waypoints: vec![[0.0, 0.0], [10.0, 0.0]],
        corridor_width: 2.0,
        label: None,
    };
    let checker = ConstraintChecker::new_geometric(vec![], vec![mandatory], None);

    // Solution has a pipe from (0,0) to (10,0) — exactly on the mandatory route
    let solution = make_solution(
        vec![node("n1", 0.0, 0.0, 5.0), node("n2", 10.0, 0.0, 5.0)],
        vec![pipe("p1", "n1", "n2", 10.0)],
    );
    let report = checker.check_solution(&solution);
    assert!(
        report.feasible,
        "solution traversing mandatory route should be feasible"
    );
}

/// Oracle line 204: mandatory route NOT traversed → infeasible.
#[test]
fn test_mandatory_route_missing_is_infeasible() {
    let mandatory = MandatoryRoute {
        waypoints: vec![[100.0, 100.0], [200.0, 100.0]],
        corridor_width: 2.0,
        label: None,
    };
    let checker = ConstraintChecker::new_geometric(vec![], vec![mandatory], None);

    // Solution pipe is far from the mandatory route
    let solution = make_solution(
        vec![node("n1", 0.0, 0.0, 5.0), node("n2", 10.0, 0.0, 5.0)],
        vec![pipe("p1", "n1", "n2", 10.0)],
    );
    let report = checker.check_solution(&solution);
    assert!(
        !report.feasible,
        "solution missing mandatory route should be infeasible"
    );
    assert!(report
        .violations
        .iter()
        .any(|v| v.kind == ConstraintViolationKind::MandatoryRoute));
}

// ── 2d: bend angle ────────────────────────────────────────────────────────────

/// Oracle line 276: bend angle below max_bend_angle is a violation.
/// Setup: three nodes forming a 90° bend; max_bend_angle = 45° → the
/// 90° deflection does NOT violate (90 >= 45). Use max_bend_angle = 120°
/// to force a violation at 90°.
#[test]
fn test_bend_angle_violation_detected() {
    // n1→n2 is horizontal (east), n2→n3 is vertical (north): 90° at n2
    // With max_bend_angle = 120°, 90° < 120° → violation
    let checker = ConstraintChecker::new_geometric(vec![], vec![], Some(120.0));

    let solution = make_solution(
        vec![
            node("n1", 0.0, 0.0, 5.0),
            node("n2", 10.0, 0.0, 5.0),
            node("n3", 10.0, 10.0, 5.0),
        ],
        vec![pipe("p1", "n1", "n2", 10.0), pipe("p2", "n2", "n3", 10.0)],
    );
    let report = checker.check_solution(&solution);
    assert!(
        !report.feasible,
        "90° bend with max_bend_angle=120° should be infeasible"
    );
    assert!(report
        .violations
        .iter()
        .any(|v| v.kind == ConstraintViolationKind::BendAngle));
}

/// Oracle line 279: straight pipes produce no bend angle violation.
#[test]
fn test_bend_angle_straight_pipe_feasible() {
    // n1→n2→n3 all on same horizontal line: 180° at n2 → no violation
    let checker = ConstraintChecker::new_geometric(vec![], vec![], Some(45.0));

    let solution = make_solution(
        vec![
            node("n1", 0.0, 0.0, 5.0),
            node("n2", 10.0, 0.0, 5.0),
            node("n3", 20.0, 0.0, 5.0),
        ],
        vec![pipe("p1", "n1", "n2", 10.0), pipe("p2", "n2", "n3", 10.0)],
    );
    let report = checker.check_solution(&solution);
    assert!(
        report.feasible,
        "straight pipes should produce no bend angle violation"
    );
}

// ── 2e: cover depth ───────────────────────────────────────────────────────────

/// Oracle line 315: pipe cover below min_cover → violation (when enforce_cover_depth=true).
#[test]
fn test_cover_depth_below_min_is_infeasible() {
    // min_cover = 1.5 m; pipe cover = z - invert = 5.0 - 4.5 = 0.5 m < 1.5
    let checker = ConstraintChecker::new_with_cover(1.5, false, 10.0);

    let mut p = pipe("p1", "n1", "n2", 10.0);
    p.start_invert = 4.5; // cover = 5.0 - 4.5 = 0.5 < 1.5
    p.end_invert = 4.5;
    let solution = make_solution(
        vec![
            NetworkNode {
                z: 5.0,
                ..node("n1", 0.0, 0.0, 5.0)
            },
            NetworkNode {
                z: 5.0,
                ..node("n2", 10.0, 0.0, 5.0)
            },
        ],
        vec![p],
    );
    let report = checker.check_solution(&solution);
    assert!(
        !report.feasible,
        "cover 0.5 m < min 1.5 m should be infeasible"
    );
    assert!(report
        .violations
        .iter()
        .any(|v| v.kind == ConstraintViolationKind::CoverDepth));
}

/// Oracle line 315: pipe cover above min_cover → feasible.
#[test]
fn test_cover_depth_above_min_is_feasible() {
    let checker = ConstraintChecker::new_with_cover(1.5, false, 10.0);

    let mut p = pipe("p1", "n1", "n2", 10.0);
    p.start_invert = 2.0; // cover = 5.0 - 2.0 = 3.0 > 1.5
    p.end_invert = 2.0;
    let solution = make_solution(
        vec![
            NetworkNode {
                z: 5.0,
                ..node("n1", 0.0, 0.0, 5.0)
            },
            NetworkNode {
                z: 5.0,
                ..node("n2", 10.0, 0.0, 5.0)
            },
        ],
        vec![p],
    );
    let report = checker.check_solution(&solution);
    assert!(
        report.feasible,
        "cover 3.0 m >= min 1.5 m should be feasible"
    );
}

// ── 2f: existing clearance ────────────────────────────────────────────────────

/// Oracle line 395: crossing with insufficient vertical separation → violation.
#[test]
fn test_existing_clearance_crossing_insufficient_vertical_infeasible() {
    use hydro_optimizer::ExistingNetworkEntry;

    // Existing network: horizontal pipe at depth 1.5 m, from (-5,5) to (15,5)
    // New pipe: from (5,-5) to (5,15) — crosses the existing at (5,5)
    // Proposed pipe depth = z - invert = 5.0 - 3.5 = 1.5 m
    // Vertical sep = |1.5 - 1.5| = 0.0 < 0.3 m → violation
    let existing = ExistingNetworkEntry {
        coords: vec![(-5.0, 5.0), (15.0, 5.0)],
        depth: 1.5,
        setback: None,
    };
    let checker = ConstraintChecker::new_with_clearance(vec![existing], 0.3, 1.0, true);

    let mut p = pipe("p1", "n1", "n2", 20.0);
    p.start_invert = 3.5; // cover = 5.0 - 3.5 = 1.5 m depth
    p.end_invert = 3.5;
    let solution = make_solution(
        vec![
            NetworkNode {
                z: 5.0,
                ..node("n1", 5.0, -5.0, 5.0)
            },
            NetworkNode {
                z: 5.0,
                ..node("n2", 5.0, 15.0, 5.0)
            },
        ],
        vec![p],
    );
    let report = checker.check_solution(&solution);
    assert!(
        !report.feasible,
        "crossing with 0.0 m vertical sep < 0.3 m should be infeasible"
    );
    assert!(report
        .violations
        .iter()
        .any(|v| v.kind == ConstraintViolationKind::ExistingClearance));
}

/// Oracle line 406: parallel pipes with sufficient separation → feasible.
#[test]
fn test_existing_clearance_parallel_sufficient_horizontal_feasible() {
    use hydro_optimizer::ExistingNetworkEntry;

    // Existing: from (0,0) to (10,0) at depth 1.5 m
    // Proposed: from (0,5) to (10,5) — parallel, horizontal sep = 5.0 > 1.0 m
    let existing = ExistingNetworkEntry {
        coords: vec![(0.0, 0.0), (10.0, 0.0)],
        depth: 1.5,
        setback: None,
    };
    let checker = ConstraintChecker::new_with_clearance(vec![existing], 0.3, 1.0, true);

    let solution = make_solution(
        vec![node("n1", 0.0, 5.0, 5.0), node("n2", 10.0, 5.0, 5.0)],
        vec![pipe("p1", "n1", "n2", 10.0)],
    );
    let report = checker.check_solution(&solution);
    assert!(
        report.feasible,
        "parallel pipe at 5.0 m sep >= 1.0 m should be feasible"
    );
}

// ── 2g: slope bounds ─────────────────────────────────────────────────────────

/// Oracle line 362: downhill slope below min_slope → violation (when enforce_slopes=true).
#[test]
fn test_slope_below_min_is_infeasible() {
    // min_slope = 0.005, pipe slope = 0.001 < 0.005
    let checker = ConstraintChecker::new_with_slopes(0.005, 0.1, true);

    let mut p = pipe("p1", "n1", "n2", 100.0);
    p.start_invert = 5.0;
    p.end_invert = 4.9; // slope = (5.0 - 4.9) / 100 = 0.001 < 0.005
    p.slope = 0.001;
    let solution = make_solution(
        vec![node("n1", 0.0, 0.0, 6.0), node("n2", 100.0, 0.0, 5.9)],
        vec![p],
    );
    let report = checker.check_solution(&solution);
    assert!(
        !report.feasible,
        "slope 0.001 < min 0.005 should be infeasible"
    );
    assert!(report
        .violations
        .iter()
        .any(|v| v.kind == ConstraintViolationKind::Slope));
}

/// Oracle line 364: pipe slope above max_slope → violation.
#[test]
fn test_slope_above_max_is_infeasible() {
    let checker = ConstraintChecker::new_with_slopes(0.001, 0.05, true);

    let mut p = pipe("p1", "n1", "n2", 100.0);
    p.start_invert = 5.0;
    p.end_invert = -5.0; // slope = (5.0 - (-5.0)) / 100 = 0.1 > 0.05
    p.slope = 0.1;
    let solution = make_solution(
        vec![node("n1", 0.0, 0.0, 6.0), node("n2", 100.0, 0.0, -4.0)],
        vec![p],
    );
    let report = checker.check_solution(&solution);
    assert!(
        !report.feasible,
        "slope 0.1 > max 0.05 should be infeasible"
    );
}

/// Oracle line 361: adverse (negative/uphill) slope is NOT rejected by slope check.
#[test]
fn test_adverse_slope_not_rejected_by_slope_check() {
    let checker = ConstraintChecker::new_with_slopes(0.005, 0.1, true);

    let mut p = pipe("p1", "n1", "n2", 100.0);
    p.start_invert = 3.0;
    p.end_invert = 4.0; // adverse: end_invert > start_invert
    p.slope = -0.01; // negative slope = uphill
    let solution = make_solution(
        vec![node("n1", 0.0, 0.0, 5.0), node("n2", 100.0, 0.0, 5.0)],
        vec![p],
    );
    let report = checker.check_solution(&solution);
    // Adverse slopes are handled by pumping objective, not slope constraint
    assert!(
        report.feasible,
        "adverse slope should not be rejected by slope constraint"
    );
}

// ── COMMIT 3 — ConstraintChecker::check + REQ-018 feasibility rate ───────────

/// REQ-004: Aggregate ConstraintChecker check integrates bounds + geometric.
/// Feasible individual with no geometric issues → feasible report.
#[test]
fn test_aggregate_check_feasible_candidate() {
    let checker =
        ConstraintChecker::new_full(vec![0.0, 0.0], vec![10.0, 5.0], vec![], vec![], None);
    let genes = vec![5.0, 2.5];
    let solution = make_solution(
        vec![node("n1", 0.0, 0.0, 5.0), node("n2", 10.0, 0.0, 5.0)],
        vec![pipe("p1", "n1", "n2", 10.0)],
    );
    let report = checker.check_full(&genes, &solution);
    assert!(report.feasible);
    assert_eq!(report.penalty, 0.0);
}

/// REQ-004: Bounds-infeasible individual → penalty without calling geometric check.
#[test]
fn test_aggregate_check_bounds_infeasible_skips_geometric() {
    let checker = ConstraintChecker::new_full(vec![0.0], vec![10.0], vec![], vec![], None);
    let genes = vec![15.0]; // out of bounds by 5.0
    let solution = make_solution(
        vec![node("n1", 0.0, 0.0, 5.0), node("n2", 10.0, 0.0, 5.0)],
        vec![pipe("p1", "n1", "n2", 10.0)],
    );
    let report = checker.check_full(&genes, &solution);
    assert!(!report.feasible);
    // Expected penalty: 1e12 + 5.0 * 1e9
    let expected = 1e12 + 5.0 * 1e9;
    assert!(
        (report.penalty - expected).abs() < 1.0,
        "penalty should be {expected}, got {}",
        report.penalty
    );
}

/// REQ-018: On a synthetic 50-individual fixture, feasibility rate ≥ 10%.
///
/// sewer gene specs:
///   route_variant:   [0.0, 9.0] Int
///   slope_factor:    [0.5, 2.0] Float
///   cover_factor:    [1.0, 1.5] Float
///   diameter_offset: [0.0, 3.0] Int
///   manhole_spacing: [60.0, 120.0] Float
///
/// With uniform random sampling within bounds, 100% of random individuals
/// are within bounds — only genes outside this range would fail.
/// We manually construct 50 individuals: 45 within bounds + 5 out-of-bounds.
/// Expected feasible rate = 45/50 = 90% ≥ 10%.
#[test]
fn test_req018_feasibility_rate_at_least_10_percent() {
    // sewer bounds
    let lower = vec![0.0, 0.5, 1.0, 0.0, 60.0];
    let upper = vec![9.0, 2.0, 1.5, 3.0, 120.0];
    let checker = ConstraintChecker::new_bounds_only(lower.clone(), upper.clone());

    // 45 feasible (midpoints) + 5 infeasible (out of bounds)
    let mut individuals: Vec<Vec<f64>> = Vec::new();
    for _ in 0..45 {
        // Midpoint of each gene — guaranteed feasible
        let mid: Vec<f64> = lower
            .iter()
            .zip(upper.iter())
            .map(|(lo, hi)| (lo + hi) / 2.0)
            .collect();
        individuals.push(mid);
    }
    for _ in 0..5 {
        // gene[0] out of bounds: -1.0 < 0.0
        individuals.push(vec![-1.0, 1.0, 1.2, 1.0, 90.0]);
    }
    assert_eq!(individuals.len(), 50);

    let feasible_count = individuals
        .iter()
        .filter(|ind| checker.check_bounds(ind).feasible)
        .count();

    let feasible_rate = feasible_count as f64 / 50.0;
    assert!(
        feasible_rate >= 0.10,
        "REQ-018: feasibility rate {:.1}% < 10% floor",
        feasible_rate * 100.0
    );
}

// Use explicit import for ConstraintViolationKind in the tests above
use hydro_optimizer::ConstraintViolationKind;
