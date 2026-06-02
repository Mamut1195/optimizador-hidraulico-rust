//! NSGA-III Normalization — extreme-point hyperplane intercept method (PR-8d2, REQ-007).
//!
//! Implementation pending (GREEN phase). This file contains only the test suite.

#[cfg(test)]
mod tests {
    // Functions called here do not exist yet → compile error (RED).
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
