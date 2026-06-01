//! Das-Dennis uniform reference points on the unit simplex.
//!
//! Generates the simplex lattice for NSGA-III (Deb & Jain 2014 §3).
//! For `nobj` objectives and `p` divisions, produces C(nobj+p-1, p) reference points,
//! each a normalised direction vector summing to 1.0.
//!
//! Design choice: `Vec<Vec<f64>>` rows (dynamic `nobj`) for public API;
//! the PR-8d1 scope covers only reference-point generation.
//! The NSGA-III normalisation + niching (PR-8d2) consumes these at runtime.

/// Generate uniform reference points on the unit simplex (Das-Dennis lattice).
///
/// # Arguments
/// * `nobj` — number of objectives (≥ 1).
/// * `p`    — number of divisions along each axis (≥ 1).
///
/// # Returns
/// A vector of `C(nobj + p - 1, p)` reference-direction vectors.
/// Each vector has `nobj` components, all ≥ 0.0, summing to 1.0.
///
/// # Panics
/// Does not panic for any sane `nobj ≥ 1, p ≥ 1` combination.
pub(crate) fn uniform_reference_points(nobj: usize, p: usize) -> Vec<Vec<f64>> {
    debug_assert!(p >= 1, "uniform_reference_points: divisions p must be >= 1");
    let mut result: Vec<Vec<f64>> = Vec::new();
    let mut current: Vec<usize> = vec![0; nobj];
    gen_refs(nobj, p, 0, p, &mut current, &mut result);
    result
}

/// Recursive enumeration of compositions of `remaining` into `nobj - depth` non-negative parts.
fn gen_refs(
    nobj: usize,
    p: usize,
    depth: usize,
    remaining: usize,
    current: &mut Vec<usize>,
    result: &mut Vec<Vec<f64>>,
) {
    if depth == nobj - 1 {
        // Last dimension: forced to `remaining`
        current[depth] = remaining;
        let point: Vec<f64> = current.iter().map(|&v| v as f64 / p as f64).collect();
        result.push(point);
        return;
    }
    for i in 0..=remaining {
        current[depth] = i;
        gen_refs(nobj, p, depth + 1, remaining - i, current, result);
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    /// C(n+p-1, p) — closed-form binomial coefficient for small values.
    fn binomial(n: usize, k: usize) -> usize {
        if k > n {
            return 0;
        }
        let k = k.min(n - k);
        let mut result = 1_usize;
        for i in 0..k {
            result = result * (n - i) / (i + 1);
        }
        result
    }

    #[test]
    fn count_5obj_p4() {
        // C(8,4) = 70
        let pts = uniform_reference_points(5, 4);
        assert_eq!(pts.len(), binomial(8, 4));
    }

    #[test]
    fn sum_to_one() {
        let pts = uniform_reference_points(5, 4);
        for pt in &pts {
            let s: f64 = pt.iter().sum();
            assert!((s - 1.0).abs() < 1e-12, "sum={s}");
        }
    }

    // ── REQ-006: uniform_reference_points — moved from tests/pr8d1_nsga3_sort_refpoints.rs ──

    /// REQ-006 Scenario: nobj=5, p=4 → C(8,4) = 70 reference points.
    #[test]
    fn test_ref_points_count_5obj_p4() {
        let pts = uniform_reference_points(5, 4);
        assert_eq!(
            pts.len(),
            70,
            "nobj=5 p=4 must produce 70 reference points (C(8,4))"
        );
    }

    /// REQ-006 Scenario: each reference point sums to 1.0 (tolerance 1e-9).
    #[test]
    fn test_ref_points_each_sums_to_one_5obj_p4() {
        let pts = uniform_reference_points(5, 4);
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
        let pts = uniform_reference_points(5, 4);
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
        let pts = uniform_reference_points(nobj, 4);
        for (i, pt) in pts.iter().enumerate() {
            assert_eq!(
                pt.len(),
                nobj,
                "point[{i}] has {} coords, expected {nobj}",
                pt.len()
            );
        }
    }

    /// 2-objective, p=3: C(4,3) = 4 points.
    #[test]
    fn test_ref_points_count_2obj_p3() {
        let pts = uniform_reference_points(2, 3);
        // Compositions of 3 into 2 non-neg parts: (0,3),(1,2),(2,1),(3,0) = 4
        assert_eq!(pts.len(), 4, "nobj=2 p=3 must produce 4 reference points");
    }

    /// 3-objective, p=2: C(4,2) = 6 points.
    #[test]
    fn test_ref_points_count_3obj_p2() {
        let pts = uniform_reference_points(3, 2);
        assert_eq!(pts.len(), 6, "nobj=3 p=2 must produce 6 reference points");
    }

    /// p=1 with 5 objectives: corner points only, C(5,1)=5 points.
    #[test]
    fn test_ref_points_count_5obj_p1() {
        let pts = uniform_reference_points(5, 1);
        assert_eq!(
            pts.len(),
            5,
            "nobj=5 p=1 must produce 5 corner reference points"
        );
    }
}
