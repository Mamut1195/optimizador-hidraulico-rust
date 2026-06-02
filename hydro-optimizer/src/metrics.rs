//! Quality metrics for multi-objective Pareto fronts (REQ-016 infrastructure).
//!
//! # Algorithms
//!
//! ## Hypervolume (HV)
//! Computes the dominated hypervolume for **2-D fronts** using a sorted sweep:
//! 1. Sort front points by objective-0 ascending.
//! 2. Walk right-to-left, accumulating the rectangular strips between consecutive
//!    x-values and the reference point's y-value minus the current point's y-value.
//!
//! For M != 2 this function returns `0.0`. A WFG or inclusion-exclusion
//! implementation for M > 2 is deferred to Phase 7. The parity test (REQ-016)
//! on the `sewer_basic` fixture (5 objectives) will drive that implementation
//! when the fixture lands.
//!
//! ## IGD+ (Inverted Generational Distance Plus)
//! Implements the Ishibuchi 2015 IGD+ formula:
//!   `IGD+(A, R) = (1/|R|) · Σ_{r∈R} min_{a∈A} d+(a, r)`
//! where `d+(a, r)_j = max(0, r_j - a_j)` for each objective j.
//! This is the Pareto-respecting version: only the component where the reference
//! point is better than the approximation point contributes to distance.

// Used by internal unit tests and by the REQ-016 parity test (pr8f_parity_skeleton.rs
// mirrors this logic locally until the public surface question is resolved in Phase 7).
#[allow(dead_code)]
/// Compute the hypervolume dominated by `front` relative to `reference_point`.
///
/// - For 2-D fronts: exact sorted-sweep algorithm.
/// - For M != 2: returns `0.0` (full HV for higher dimensions deferred, see module docs).
///
/// All objectives are assumed to be **minimized**. The reference point MUST
/// dominate (be strictly worse than) every front point on all objectives for
/// a meaningful result.
///
/// # Arguments
/// * `front`           – Slice of objective vectors (each has the same length M).
/// * `reference_point` – Reference point of length M.
pub(crate) fn hypervolume(front: &[Vec<f64>], reference_point: &[f64]) -> f64 {
    if front.is_empty() {
        return 0.0;
    }
    let m = reference_point.len();
    if m != 2 {
        // Full HV for M != 2 is deferred (WFG / inclusion-exclusion).
        // REQ-016 parity test will drive this implementation when the fixture lands.
        return 0.0;
    }

    // 2-D sorted sweep
    let mut pts: Vec<(f64, f64)> = front
        .iter()
        .filter_map(|v| {
            if v.len() >= 2 {
                Some((v[0], v[1]))
            } else {
                None
            }
        })
        .collect();

    if pts.is_empty() {
        return 0.0;
    }

    // Sort by x ascending
    pts.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    let ref_x = reference_point[0];
    let ref_y = reference_point[1];

    let mut hv = 0.0_f64;
    let mut prev_x = ref_x;

    // Walk from right to left (largest x first)
    for (x, y) in pts.iter().rev() {
        let width = prev_x - x;
        let height = ref_y - y;
        if width > 0.0 && height > 0.0 {
            hv += width * height;
        }
        prev_x = *x;
    }

    hv
}

// Same justification as hypervolume above.
#[allow(dead_code)]
/// Compute IGD+ (Inverted Generational Distance Plus) of approximation `front`
/// with respect to `reference_front`.
///
/// Uses the Ishibuchi 2015 formula:
///   `IGD+(A, R) = (1/|R|) · Σ_{r∈R} min_{a∈A} d+(a, r)`
///
/// where `d+(a, r) = sqrt(Σ_j max(0, r_j - a_j)²)`.
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
                        let diff = rj - aj; // positive when r is better (lower)
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

    /// Front is better (lower) than reference → d+(a,r) for each dim = max(0, r-a) > 0.
    #[test]
    fn test_igdplus_front_better_than_reference() {
        // a=(0,0) vs r=(1,1): d+([0,0],[1,1]) = sqrt((1-0)^2 + (1-0)^2) = sqrt(2)
        let front = vec![vec![0.0, 0.0]];
        let reference = vec![vec![1.0, 1.0]];
        let igd = igd_plus(&front, &reference);
        assert!(
            (igd - 2.0_f64.sqrt()).abs() < 1e-12,
            "expected sqrt(2), got {igd}"
        );
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
}
