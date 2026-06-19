//! Quality metrics for multi-objective Pareto fronts (REQ-016 infrastructure).
//!
//! # Algorithms
//!
//! ## Hypervolume (HV)
//! Computes exact dominated hypervolume for arbitrary-dimensional minimization
//! fronts by recursively slicing the union of point-to-reference hyperrectangles.
//!
//! ## IGD+ (Inverted Generational Distance Plus)
//! Implements the Ishibuchi 2015 IGD+ formula:
//!   `IGD+(A, R) = (1/|R|) · Σ_{r∈R} min_{a∈A} d+(a, r)`
//! where `d+(a, r)_j = max(0, a_j - r_j)` for each objective j.
//! This is the Pareto-respecting version: only components where the approximation
//! point is worse than the reference point contribute to distance.

// Used by internal unit tests and the committed Python-oracle metric fixture.
#[allow(dead_code)]
/// Compute the hypervolume dominated by `front` relative to `reference_point`.
///
/// Supports arbitrary objective counts. Points outside the reference box,
/// non-finite points, and points with the wrong dimensionality are ignored.
///
/// All objectives are assumed to be **minimized**. The reference point MUST
/// dominate (be strictly worse than) every front point on all objectives for
/// a meaningful result.
///
/// # Arguments
/// * `front`           – Slice of objective vectors (each has the same length M).
/// * `reference_point` – Reference point of length M.
pub(crate) fn hypervolume(front: &[Vec<f64>], reference_point: &[f64]) -> f64 {
    let dimensions = reference_point.len();
    if front.is_empty() || dimensions == 0 || reference_point.iter().any(|value| !value.is_finite())
    {
        return 0.0;
    }

    let points: Vec<Vec<f64>> = front
        .iter()
        .filter(|point| {
            point.len() == dimensions
                && point.iter().all(|value| value.is_finite())
                && point
                    .iter()
                    .zip(reference_point)
                    .all(|(value, reference)| value < reference)
        })
        .cloned()
        .collect();

    hypervolume_slices(&points, reference_point)
}

fn hypervolume_slices(points: &[Vec<f64>], reference_point: &[f64]) -> f64 {
    if points.is_empty() {
        return 0.0;
    }

    if reference_point.len() == 1 {
        let minimum = points
            .iter()
            .map(|point| point[0])
            .fold(reference_point[0], f64::min);
        return (reference_point[0] - minimum).max(0.0);
    }

    let axis = reference_point.len() - 1;
    let mut levels: Vec<f64> = points.iter().map(|point| point[axis]).collect();
    levels.sort_by(|a, b| a.total_cmp(b));
    levels.dedup_by(|a, b| a.total_cmp(b).is_eq());
    levels.push(reference_point[axis]);

    let mut volume = 0.0;
    for bounds in levels.windows(2) {
        let lower = bounds[0];
        let upper = bounds[1];
        if upper <= lower {
            continue;
        }

        let active: Vec<Vec<f64>> = points
            .iter()
            .filter(|point| point[axis] <= lower)
            .map(|point| point[..axis].to_vec())
            .collect();
        let slice = hypervolume_slices(&active, &reference_point[..axis]);
        volume += slice * (upper - lower);
    }

    volume
}

// Same justification as hypervolume above.
#[allow(dead_code)]
/// Compute IGD+ (Inverted Generational Distance Plus) of approximation `front`
/// with respect to `reference_front`.
///
/// Uses the Ishibuchi 2015 formula:
///   `IGD+(A, R) = (1/|R|) · Σ_{r∈R} min_{a∈A} d+(a, r)`
///
/// where `d+(a, r) = sqrt(Σ_j max(0, a_j - r_j)²)`.
///
/// All objectives are minimized. Returns `f64::INFINITY` if either front is empty.
///
/// # Arguments
/// * `front`           – The approximation front (what we're evaluating).
/// * `reference_front` – The reference (oracle) Pareto front.
pub(crate) fn igd_plus(front: &[Vec<f64>], reference_front: &[Vec<f64>]) -> f64 {
    if front.is_empty() || reference_front.is_empty() {
        return f64::INFINITY;
    }

    let mut sum = 0.0_f64;
    for r in reference_front {
        let min_dist = front
            .iter()
            .map(|a| {
                // d+(a, r): only dimensions where r is better than a contribute
                let sq_sum: f64 = r
                    .iter()
                    .zip(a.iter())
                    .map(|(&rj, &aj)| {
                        let diff = aj - rj; // positive when approximation is worse (higher)
                        if diff > 0.0 {
                            diff * diff
                        } else {
                            0.0
                        }
                    })
                    .sum();
                sq_sum.sqrt()
            })
            .fold(f64::INFINITY, f64::min);
        sum += min_dist;
    }

    sum / reference_front.len() as f64
}

// ── Tests ─────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::{hypervolume, igd_plus};
    use serde::Deserialize;

    #[derive(Deserialize)]
    struct QualityMetricsFixture {
        approximation_front: Vec<Vec<f64>>,
        reference_front: Vec<Vec<f64>>,
        reference_point: Vec<f64>,
        oracle_hypervolume: f64,
        oracle_igd_plus: f64,
        absolute_tolerance: f64,
    }

    // ── Hypervolume tests ─────────────────────────────────────────────────────

    /// 2-D front: single point — HV = rectangle from point to reference.
    #[test]
    fn test_hv_single_point_2d() {
        let front = vec![vec![1.0, 1.0]];
        let ref_pt = vec![3.0, 3.0];
        // Area = (3-1) * (3-1) = 4.0
        let hv = hypervolume(&front, &ref_pt);
        assert!((hv - 4.0).abs() < 1e-12, "expected 4.0, got {hv}");
    }

    /// 2-D front: two non-dominated points.
    /// Points: (1, 3) and (3, 1), reference: (4, 4)
    /// Strip for (3,1) rightmost: x-range [3,4] width=1, height=4-1=3 → 3
    /// Strip for (1,3): x-range [1,3] width=2, height=4-3=1 → 2
    /// Total = 5
    #[test]
    fn test_hv_two_points_2d() {
        let front = vec![vec![1.0, 3.0], vec![3.0, 1.0]];
        let ref_pt = vec![4.0, 4.0];
        let hv = hypervolume(&front, &ref_pt);
        assert!((hv - 5.0).abs() < 1e-12, "expected 5.0, got {hv}");
    }

    #[test]
    fn test_hv_single_point_5d() {
        let front = vec![vec![1.0; 5]];
        let reference = vec![3.0; 5];

        let hv = hypervolume(&front, &reference);

        assert!((hv - 32.0).abs() < 1e-12, "expected 32.0, got {hv}");
    }

    #[test]
    fn test_hv_two_points_3d_union() {
        let front = vec![vec![1.0, 1.0, 2.0], vec![2.0, 1.0, 1.0]];
        let reference = vec![3.0, 3.0, 3.0];

        let hv = hypervolume(&front, &reference);

        assert!((hv - 6.0).abs() < 1e-12, "expected 6.0, got {hv}");
    }

    /// Empty front → HV = 0.
    #[test]
    fn test_hv_empty_front() {
        let hv = hypervolume(&[], &[2.0, 2.0]);
        assert_eq!(hv, 0.0);
    }

    // ── IGD+ tests ────────────────────────────────────────────────────────────

    /// Perfect match: front equals reference → IGD+ = 0.
    #[test]
    fn test_igdplus_perfect_match() {
        let front = vec![vec![1.0, 2.0], vec![2.0, 1.0]];
        let reference = vec![vec![1.0, 2.0], vec![2.0, 1.0]];
        let igd = igd_plus(&front, &reference);
        assert!(
            igd.abs() < 1e-12,
            "perfect match should give IGD+ ~0, got {igd}"
        );
    }

    /// Reference is better (lower) than approximation, so every dimension contributes.
    #[test]
    fn test_igdplus_reference_better_than_front() {
        let front = vec![vec![1.0, 1.0]];
        let reference = vec![vec![0.0, 0.0]];
        let igd = igd_plus(&front, &reference);
        assert!(
            (igd - 2.0_f64.sqrt()).abs() < 1e-12,
            "expected sqrt(2), got {igd}"
        );
    }

    #[test]
    fn test_igdplus_front_better_than_reference_is_zero() {
        let front = vec![vec![0.0, 0.0]];
        let reference = vec![vec![1.0, 1.0]];
        let igd = igd_plus(&front, &reference);

        assert!(igd.abs() < 1e-12, "expected zero, got {igd}");
    }

    /// Empty front → IGD+ = infinity.
    #[test]
    fn test_igdplus_empty_front() {
        let igd = igd_plus(&[], &[vec![1.0, 1.0]]);
        assert!(igd.is_infinite());
    }

    /// Empty reference → IGD+ = infinity.
    #[test]
    fn test_igdplus_empty_reference() {
        let igd = igd_plus(&[vec![1.0, 1.0]], &[]);
        assert!(igd.is_infinite());
    }

    #[test]
    fn test_five_objective_metrics_match_python_oracle() {
        let fixture: QualityMetricsFixture = serde_json::from_str(include_str!(
            "../../tests/oracle/fixtures/optimizer_quality_metrics_golden.json"
        ))
        .expect("optimizer quality metric fixture must be valid JSON");

        let hypervolume = hypervolume(&fixture.approximation_front, &fixture.reference_point);
        let igd_plus = igd_plus(&fixture.approximation_front, &fixture.reference_front);

        assert!(
            (hypervolume - fixture.oracle_hypervolume).abs() <= fixture.absolute_tolerance,
            "hypervolume parity failed: rust={hypervolume}, oracle={}",
            fixture.oracle_hypervolume
        );
        assert!(
            (igd_plus - fixture.oracle_igd_plus).abs() <= fixture.absolute_tolerance,
            "IGD+ parity failed: rust={igd_plus}, oracle={}",
            fixture.oracle_igd_plus
        );
    }
}
