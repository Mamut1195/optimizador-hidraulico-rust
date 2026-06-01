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
pub fn uniform_reference_points(nobj: usize, p: usize) -> Vec<Vec<f64>> {
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
}
