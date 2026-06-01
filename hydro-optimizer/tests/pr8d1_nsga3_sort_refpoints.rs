//! PR-8d1 RED tests — NSGA-III fast nondominated sort + Das-Dennis reference points
//!
//! REQ-005: fast_nondominated_sort assigns Pareto ranks correctly.
//! REQ-006: uniform_reference_points generates the simplex lattice for Das-Dennis.
//!
//! All tests MUST fail (compile error) before the nsga3 module exists.

use hydro_optimizer::nsga3::{nondom_sort, reference_points};

// ── REQ-005: fast nondominated sort ──────────────────────────────────────────

/// Known 3-point, 5-objective population (2 active objectives only).
///
/// A = (1, 2, 0, 0, 0): nondominated
/// B = (2, 1, 0, 0, 0): nondominated (A does not dominate B or vice-versa)
/// C = (3, 3, 0, 0, 0): dominated by both A and B
///
/// Expected: front[0] = {0, 1}, front[1] = {2}
#[test]
fn test_nondom_sort_known_3point_2obj() {
    let fitnesses: &[[f64; 5]] = &[
        [1.0, 2.0, 0.0, 0.0, 0.0], // A — index 0
        [2.0, 1.0, 0.0, 0.0, 0.0], // B — index 1
        [3.0, 3.0, 0.0, 0.0, 0.0], // C — index 2
    ];
    let fronts = nondom_sort::fast_nondominated_sort(fitnesses);
    assert_eq!(fronts.len(), 2, "expected exactly 2 fronts");

    let front0: std::collections::BTreeSet<usize> = fronts[0].iter().copied().collect();
    let front1: std::collections::BTreeSet<usize> = fronts[1].iter().copied().collect();

    assert!(front0.contains(&0), "A must be in front 0");
    assert!(front0.contains(&1), "B must be in front 0");
    assert_eq!(front0.len(), 2, "front 0 must contain exactly 2 individuals");
    assert!(front1.contains(&2), "C must be in front 1");
    assert_eq!(front1.len(), 1, "front 1 must contain exactly 1 individual");
}

/// REQ-005 Scenario: All-identical objectives → every individual gets rank 1.
#[test]
fn test_nondom_sort_all_identical_objectives() {
    let fitnesses: &[[f64; 5]] = &[
        [1.0, 1.0, 1.0, 1.0, 1.0],
        [1.0, 1.0, 1.0, 1.0, 1.0],
        [1.0, 1.0, 1.0, 1.0, 1.0],
        [1.0, 1.0, 1.0, 1.0, 1.0],
    ];
    let fronts = nondom_sort::fast_nondominated_sort(fitnesses);
    assert_eq!(fronts.len(), 1, "all identical → single front");
    assert_eq!(fronts[0].len(), 4, "all 4 individuals in front 0");
}

/// Strictly dominated chain: A=(1,1,1,1,1) dominates B=(2,2,2,2,2) dominates C=(3,3,3,3,3).
/// Expected: 3 separate fronts, one individual each.
#[test]
fn test_nondom_sort_strictly_dominated_chain() {
    let fitnesses: &[[f64; 5]] = &[
        [1.0, 1.0, 1.0, 1.0, 1.0], // best — index 0
        [2.0, 2.0, 2.0, 2.0, 2.0], // middle — index 1
        [3.0, 3.0, 3.0, 3.0, 3.0], // worst — index 2
    ];
    let fronts = nondom_sort::fast_nondominated_sort(fitnesses);
    assert_eq!(fronts.len(), 3, "strict chain → 3 fronts");
    assert_eq!(fronts[0], vec![0]);
    assert_eq!(fronts[1], vec![1]);
    assert_eq!(fronts[2], vec![2]);
}

/// Single individual always gets rank 1 (front 0).
#[test]
fn test_nondom_sort_single_individual() {
    let fitnesses: &[[f64; 5]] = &[[5.0, 3.0, 1.0, 2.0, 4.0]];
    let fronts = nondom_sort::fast_nondominated_sort(fitnesses);
    assert_eq!(fronts.len(), 1);
    assert_eq!(fronts[0], vec![0]);
}

/// Empty population returns empty fronts (no panic).
#[test]
fn test_nondom_sort_empty_population() {
    let fitnesses: &[[f64; 5]] = &[];
    let fronts = nondom_sort::fast_nondominated_sort(fitnesses);
    assert!(fronts.is_empty());
}

/// All individuals across fronts account for every input index (no index dropped).
#[test]
fn test_nondom_sort_all_indices_covered() {
    let fitnesses: &[[f64; 5]] = &[
        [1.0, 5.0, 0.0, 0.0, 0.0],
        [3.0, 3.0, 0.0, 0.0, 0.0],
        [5.0, 1.0, 0.0, 0.0, 0.0],
        [4.0, 4.0, 0.0, 0.0, 0.0],
        [2.0, 6.0, 0.0, 0.0, 0.0],
    ];
    let fronts = nondom_sort::fast_nondominated_sort(fitnesses);
    let mut all_indices: Vec<usize> = fronts.iter().flatten().copied().collect();
    all_indices.sort_unstable();
    assert_eq!(all_indices, vec![0, 1, 2, 3, 4], "all 5 indices must appear");
}

// ── REQ-006: Das-Dennis reference points ─────────────────────────────────────

/// REQ-006 Scenario: nobj=5, p=4 → C(8,4) = 70 reference points.
#[test]
fn test_ref_points_count_5obj_p4() {
    let pts = reference_points::uniform_reference_points(5, 4);
    assert_eq!(pts.len(), 70, "nobj=5 p=4 must produce 70 reference points (C(8,4))");
}

/// REQ-006 Scenario: each reference point sums to 1.0 (tolerance 1e-9).
#[test]
fn test_ref_points_each_sums_to_one_5obj_p4() {
    let pts = reference_points::uniform_reference_points(5, 4);
    for (i, pt) in pts.iter().enumerate() {
        let s: f64 = pt.iter().sum();
        assert!(
            (s - 1.0_f64).abs() < 1e-9,
            "point[{i}] sum={s} deviates from 1.0 by more than 1e-9"
        );
    }
}

/// All coordinates are non-negative.
#[test]
fn test_ref_points_all_nonnegative() {
    let pts = reference_points::uniform_reference_points(5, 4);
    for (i, pt) in pts.iter().enumerate() {
        for (j, &v) in pt.iter().enumerate() {
            assert!(
                v >= 0.0,
                "point[{i}][{j}]={v} is negative — reference points must be in the unit simplex"
            );
        }
    }
}

/// Each reference point has exactly `nobj` coordinates.
#[test]
fn test_ref_points_correct_dimensionality() {
    let nobj = 5_usize;
    let pts = reference_points::uniform_reference_points(nobj, 4);
    for (i, pt) in pts.iter().enumerate() {
        assert_eq!(pt.len(), nobj, "point[{i}] has {} coords, expected {nobj}", pt.len());
    }
}

/// 2-objective, p=3: C(4,3) = 4 points.
#[test]
fn test_ref_points_count_2obj_p3() {
    let pts = reference_points::uniform_reference_points(2, 3);
    // Compositions of 3 into 2 non-neg parts: (0,3),(1,2),(2,1),(3,0) = 4
    assert_eq!(pts.len(), 4, "nobj=2 p=3 must produce 4 reference points");
}

/// 3-objective, p=2: C(4,2) = 6 points.
#[test]
fn test_ref_points_count_3obj_p2() {
    let pts = reference_points::uniform_reference_points(3, 2);
    assert_eq!(pts.len(), 6, "nobj=3 p=2 must produce 6 reference points");
}

/// p=1 with 5 objectives: corner points only, C(5,1)=5 points.
#[test]
fn test_ref_points_count_5obj_p1() {
    let pts = reference_points::uniform_reference_points(5, 1);
    assert_eq!(pts.len(), 5, "nobj=5 p=1 must produce 5 corner reference points");
}
