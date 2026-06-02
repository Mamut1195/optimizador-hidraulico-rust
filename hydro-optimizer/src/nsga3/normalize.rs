//! NSGA-III Normalization — extreme-point hyperplane intercept method.
//!
//! Implements the normalization procedure from Deb & Jain 2014 §4.1:
//! 1. Translate all objectives by the **ideal point** (per-objective minimum).
//! 2. Find **extreme points** via the Achievement Scalarizing Function (ASF).
//! 3. Solve 5×5 linear system for hyperplane **intercepts**.
//! 4. If the system is degenerate (|det| < 1e-12) or any intercept ≤ 0, fall back to
//!    per-objective maximum of the translated population.
//! 5. Divide translated objectives by intercepts → normalized objectives in [0, ∞).
//!
//! Design choice: hand-coded 5×5 Gaussian elimination; no external matrix crate.

/// Result of the normalization step.
///
/// - `normalized`: translated and intercept-divided fitness, one row per individual.
/// - `ideal`:      ideal point (per-objective minimum) used for translation.
/// - `intercepts`: the hyperplane intercepts (after fallback if needed).
pub(crate) struct NormalizeResult {
    pub normalized: Vec<[f64; 5]>,
    pub ideal: [f64; 5],
    pub intercepts: [f64; 5],
}

/// Normalize a set of 5-objective fitness vectors.
///
/// # Arguments
/// * `fitnesses` — one row per individual, 5 objectives, all to be minimized.
///
/// # Returns
/// A [`NormalizeResult`] with normalized objectives, ideal point, and intercepts.
///
/// # Panics
/// Does not panic for well-formed non-empty input. Empty input returns all-zero result.
pub(crate) fn normalize(fitnesses: &[[f64; 5]]) -> NormalizeResult {
    let n = fitnesses.len();
    if n == 0 {
        return NormalizeResult {
            normalized: Vec::new(),
            ideal: [0.0; 5],
            intercepts: [1.0; 5],
        };
    }

    // ── Step 1: Ideal point (per-objective minimum) ───────────────────────────
    let mut ideal = [f64::INFINITY; 5];
    for f in fitnesses {
        for j in 0..5 {
            if f[j] < ideal[j] {
                ideal[j] = f[j];
            }
        }
    }

    // ── Step 2: Translate by ideal ────────────────────────────────────────────
    let translated: Vec<[f64; 5]> = fitnesses
        .iter()
        .map(|f| {
            let mut t = [0.0_f64; 5];
            for j in 0..5 {
                t[j] = f[j] - ideal[j];
            }
            t
        })
        .collect();

    // ── Step 3: Extreme points via ASF ────────────────────────────────────────
    // ASF(f, w_i) = max_j(f_j / w_j) where w_j = 1 for j==i, else 1e-6.
    // Extreme point for axis i = individual that minimizes ASF with w = e_i (approx).
    let extreme_points = find_extreme_points(&translated);

    // ── Step 4: Solve 5×5 system for intercepts ───────────────────────────────
    //    A · x = 1  where A rows are the extreme points, 1 is the all-ones RHS.
    //    Intercepts a_j = 1 / x_j.
    let intercepts = compute_intercepts(&extreme_points, &translated);

    // ── Step 5: Divide by intercepts ─────────────────────────────────────────
    let normalized: Vec<[f64; 5]> = translated
        .iter()
        .map(|t| {
            let mut normed = [0.0_f64; 5];
            for j in 0..5 {
                normed[j] = if intercepts[j] > 1e-12 {
                    t[j] / intercepts[j]
                } else {
                    t[j]
                };
            }
            normed
        })
        .collect();

    NormalizeResult {
        normalized,
        ideal,
        intercepts,
    }
}

/// Find the extreme point for each objective axis via the ASF.
///
/// Returns a 5×5 matrix where row `i` is the extreme point for objective `i`.
fn find_extreme_points(translated: &[[f64; 5]]) -> [[f64; 5]; 5] {
    let mut extremes = [[0.0_f64; 5]; 5];
    for (axis, extreme) in extremes.iter_mut().enumerate() {
        let mut best_asf = f64::INFINITY;
        let mut best_point = [0.0_f64; 5];
        for t in translated {
            let asf = compute_asf(t, axis);
            if asf < best_asf {
                best_asf = asf;
                best_point = *t;
            }
        }
        *extreme = best_point;
    }
    extremes
}

/// Achievement Scalarizing Function for a translated fitness vector.
///
/// `w_j = 1` for `j == axis`, `1e-6` otherwise.
/// `ASF(f) = max_j(f_j / w_j)`.
#[inline]
fn compute_asf(f: &[f64; 5], axis: usize) -> f64 {
    f.iter()
        .enumerate()
        .map(|(j, &fj)| {
            let w = if j == axis { 1.0_f64 } else { 1e-6_f64 };
            fj / w
        })
        .fold(f64::NEG_INFINITY, f64::max)
}

/// Compute intercepts by solving the 5×5 linear system `A · x = 1`.
///
/// Falls back to per-objective maximum when the system is degenerate
/// (|det| < 1e-12) or any intercept is ≤ 0.
fn compute_intercepts(extreme_points: &[[f64; 5]; 5], translated: &[[f64; 5]]) -> [f64; 5] {
    // Build augmented matrix [A | b] where b = [1; 1; 1; 1; 1].
    let mut a = [[0.0_f64; 6]; 5];
    for i in 0..5 {
        for j in 0..5 {
            a[i][j] = extreme_points[i][j];
        }
        a[i][5] = 1.0;
    }

    // Gaussian elimination with partial pivoting.
    // Reason: genuine index-based algorithm; col/row indices are used simultaneously
    // for partial pivoting (a[row][col]) and back-substitution, not expressible as an iterator.
    #[allow(clippy::needless_range_loop)]
    for col in 0..5 {
        // Find pivot row (partial pivoting).
        let mut max_row = col;
        let mut max_val = a[col][col].abs();
        for row in (col + 1)..5 {
            if a[row][col].abs() > max_val {
                max_val = a[row][col].abs();
                max_row = row;
            }
        }
        a.swap(col, max_row);

        if a[col][col].abs() < 1e-12 {
            // Degenerate system — fall back to max-per-objective.
            return fallback_intercepts(translated);
        }

        // Eliminate below pivot row.
        for row in (col + 1)..5 {
            let factor = a[row][col] / a[col][col];
            // Copy pivot row to avoid simultaneous borrow of `a`.
            let pivot: [f64; 6] = a[col];
            for (elem, &piv) in a[row][col..].iter_mut().zip(pivot[col..].iter()) {
                *elem -= factor * piv;
            }
        }
    }

    // Back-substitution.
    let mut x = [0.0_f64; 5];
    for i in (0..5).rev() {
        x[i] = a[i][5];
        for j in (i + 1)..5 {
            x[i] -= a[i][j] * x[j];
        }
        x[i] /= a[i][i];
    }

    // Intercepts: a_j = 1 / x_j.
    let mut intercepts = [0.0_f64; 5];
    for j in 0..5 {
        if x[j] <= 0.0 || !x[j].is_finite() {
            return fallback_intercepts(translated);
        }
        intercepts[j] = 1.0 / x[j];
    }
    intercepts
}

/// Fallback: per-objective maximum of the translated population.
///
/// Used when the extreme-point linear system is degenerate.
fn fallback_intercepts(translated: &[[f64; 5]]) -> [f64; 5] {
    let mut max_vals = [f64::NEG_INFINITY; 5];
    for t in translated {
        for (max, &v) in max_vals.iter_mut().zip(t.iter()) {
            if v > *max {
                *max = v;
            }
        }
    }
    // Guard against zero intercepts (all-identical objectives on an axis).
    for max in &mut max_vals {
        if *max <= 0.0 {
            *max = 1.0;
        }
    }
    max_vals
}

// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;

    fn approx_eq(a: f64, b: f64, tol: f64) -> bool {
        (a - b).abs() < tol
    }

    // ── RED tests for normalize.rs (PR-8d2 REQ-007) ──────────────────────────

    /// Ideal point is per-objective minimum of the input.
    #[test]
    fn test_normalize_ideal_is_per_obj_minimum() {
        let fitnesses: &[[f64; 5]] = &[
            [3.0, 1.0, 4.0, 1.0, 5.0],
            [1.0, 5.0, 2.0, 3.0, 2.0],
            [2.0, 3.0, 1.0, 5.0, 1.0],
        ];
        let result = normalize(fitnesses);
        assert_eq!(result.ideal, [1.0, 1.0, 1.0, 1.0, 1.0]);
    }

    /// After normalization all translated values must be ≥ 0.
    #[test]
    fn test_normalize_translated_nonnegative() {
        let fitnesses: &[[f64; 5]] = &[
            [10.0, 5.0, 2.0, 8.0, 3.0],
            [2.0, 8.0, 5.0, 3.0, 7.0],
            [6.0, 3.0, 9.0, 1.0, 4.0],
        ];
        let result = normalize(fitnesses);
        for row in &result.normalized {
            for &v in row {
                assert!(v >= -1e-12, "normalized value {v} is negative");
            }
        }
    }

    /// For a population with clearly distinct extreme points intercepts must be positive.
    #[test]
    fn test_normalize_intercepts_positive_for_well_conditioned_input() {
        let fitnesses: &[[f64; 5]] = &[
            [5.0, 0.0, 0.0, 0.0, 0.0],
            [0.0, 4.0, 0.0, 0.0, 0.0],
            [0.0, 0.0, 3.0, 0.0, 0.0],
            [0.0, 0.0, 0.0, 2.0, 0.0],
            [0.0, 0.0, 0.0, 0.0, 6.0],
        ];
        let result = normalize(fitnesses);
        for (j, &ic) in result.intercepts.iter().enumerate() {
            assert!(ic > 0.0, "intercept[{j}] = {ic} must be positive");
        }
    }

    /// Known values: diagonal extreme points → intercepts equal the corner magnitudes.
    #[test]
    fn test_normalize_known_corner_values() {
        let a = 2.0_f64;
        let b = 3.0_f64;
        let c = 4.0_f64;
        let d = 5.0_f64;
        let e = 6.0_f64;
        let fitnesses: &[[f64; 5]] = &[
            [1.0 + a, 1.0, 1.0, 1.0, 1.0],
            [1.0, 1.0 + b, 1.0, 1.0, 1.0],
            [1.0, 1.0, 1.0 + c, 1.0, 1.0],
            [1.0, 1.0, 1.0, 1.0 + d, 1.0],
            [1.0, 1.0, 1.0, 1.0, 1.0 + e],
        ];
        let result = normalize(fitnesses);
        let tol = 1e-9;
        assert!(approx_eq(result.intercepts[0], a, tol), "intercept[0]");
        assert!(approx_eq(result.intercepts[1], b, tol), "intercept[1]");
        assert!(approx_eq(result.intercepts[2], c, tol), "intercept[2]");
        assert!(approx_eq(result.intercepts[3], d, tol), "intercept[3]");
        assert!(approx_eq(result.intercepts[4], e, tol), "intercept[4]");
    }

    /// Degenerate population (all identical) must NOT panic and use fallback.
    #[test]
    fn test_normalize_degenerate_all_identical_no_panic() {
        let fitnesses: &[[f64; 5]] = &[
            [2.0, 2.0, 2.0, 2.0, 2.0],
            [2.0, 2.0, 2.0, 2.0, 2.0],
            [2.0, 2.0, 2.0, 2.0, 2.0],
        ];
        let result = normalize(fitnesses);
        for &ic in &result.intercepts {
            assert!(ic > 0.0, "degenerate fallback intercept must be positive");
        }
    }

    /// Empty input returns empty normalized vector without panicking.
    #[test]
    fn test_normalize_empty_input() {
        let result = normalize(&[]);
        assert!(result.normalized.is_empty());
        assert_eq!(result.ideal, [0.0; 5]);
    }

    /// Normalized values at the corner individual equal 1.0 on its axis.
    #[test]
    fn test_normalize_corner_individual_maps_to_unit_value() {
        let fitnesses: &[[f64; 5]] = &[
            [1.0 + 5.0, 1.0, 1.0, 1.0, 1.0],
            [1.0, 1.0 + 4.0, 1.0, 1.0, 1.0],
            [1.0, 1.0, 1.0 + 3.0, 1.0, 1.0],
            [1.0, 1.0, 1.0, 1.0 + 2.0, 1.0],
            [1.0, 1.0, 1.0, 1.0, 1.0 + 6.0],
        ];
        let result = normalize(fitnesses);
        assert!(approx_eq(result.normalized[0][0], 1.0, 1e-9));
        assert!(approx_eq(result.normalized[0][1], 0.0, 1e-9));
    }
}
