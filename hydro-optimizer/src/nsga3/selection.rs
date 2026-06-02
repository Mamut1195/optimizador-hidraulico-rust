//! NSGA-III Environmental Selection — top-level wrapper.
//!
//! Implements `select_environmental` which combines:
//! 1. Fast nondominated sort (PR-8d1).
//! 2. Fill population front-by-front until the "splitting" front.
//! 3. Normalize objectives (this slice — `normalize`).
//! 4. Associate + niche-count selection from the partial front (this slice — `niching`).
//!
//! This mirrors DEAP's `tools.selNSGA3` algorithm for 5 minimization objectives.

use rand_chacha::ChaCha20Rng;

use crate::nsga3::niching::{associate, build_niche_counts, select_from_partial_front};
use crate::nsga3::nondom_sort::fast_nondominated_sort;
use crate::nsga3::normalize::normalize;
use crate::nsga3::reference_points::uniform_reference_points;

/// Select `n` individuals from a population using NSGA-III environmental selection.
///
/// # Arguments
/// * `fitnesses`  — fitness matrix, one `[f64; 5]` row per individual.
/// * `n`          — target selection size (≤ `fitnesses.len()`).
/// * `ref_points` — reference point vectors from the Das-Dennis lattice.
/// * `rng`        — RNG for tie-breaking in niching.
///
/// # Returns
/// A `Vec<usize>` of length `min(n, fitnesses.len())` containing selected indices.
pub(crate) fn select_environmental(
    fitnesses: &[[f64; 5]],
    n: usize,
    ref_points: &[Vec<f64>],
    rng: &mut ChaCha20Rng,
) -> Vec<usize> {
    let pop_size = fitnesses.len();

    if pop_size == 0 || n == 0 {
        return Vec::new();
    }
    if n >= pop_size {
        return (0..pop_size).collect();
    }

    // ── Step 1: Fast nondominated sort ────────────────────────────────────────
    let fronts = fast_nondominated_sort(fitnesses);

    // ── Step 2: Fill front-by-front until splitting front ─────────────────────
    let mut selected: Vec<usize> = Vec::with_capacity(n);
    let mut partial_front: Vec<usize> = Vec::new();

    for front in &fronts {
        if selected.len() + front.len() <= n {
            // Entire front fits.
            selected.extend_from_slice(front);
        } else {
            // This is the splitting (partial) front.
            partial_front = front.clone();
            break;
        }
        if selected.len() == n {
            return selected;
        }
    }

    // Already have exactly n after full fronts (no partial needed).
    if selected.len() == n {
        return selected;
    }

    let needed = n - selected.len();

    // ── Step 3: Normalize ALL objectives (definite + partial) ─────────────────
    // Gather all candidate indices for normalization context.
    let all_candidates: Vec<usize> = selected
        .iter()
        .copied()
        .chain(partial_front.iter().copied())
        .collect();

    let candidate_fitnesses: Vec<[f64; 5]> = all_candidates.iter().map(|&i| fitnesses[i]).collect();

    let norm_result = normalize(&candidate_fitnesses);

    // ── Step 4: Associate ALL candidates to reference points ──────────────────
    // Build association table indexed by ORIGINAL population index.
    // Unused entries default to (0, f64::INFINITY).
    let mut assoc: Vec<(usize, f64)> = vec![(0, f64::INFINITY); pop_size];
    let candidate_assoc = associate(&norm_result.normalized, ref_points);
    for (pos, &orig_idx) in all_candidates.iter().enumerate() {
        assoc[orig_idx] = candidate_assoc[pos];
    }

    // ── Step 5: Build niche counts from definite set ──────────────────────────
    let mut niche_count = build_niche_counts(&selected, &assoc, ref_points.len());

    // ── Step 6: Select from partial front via niching ─────────────────────────
    let extra = select_from_partial_front(&partial_front, &assoc, &mut niche_count, needed, rng);

    selected.extend(extra);
    selected
}

/// Convenience wrapper using the standard 5-objective, p=4 reference points.
///
/// Generates 70 Das-Dennis reference points and delegates to `select_environmental`.
pub(crate) fn sel_nsga3(fitnesses: &[[f64; 5]], n: usize, rng: &mut ChaCha20Rng) -> Vec<usize> {
    let ref_points = uniform_reference_points(5, 4);
    select_environmental(fitnesses, n, &ref_points, rng)
}

// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    fn make_rng() -> ChaCha20Rng {
        ChaCha20Rng::seed_from_u64(0)
    }

    fn synthetic_fitnesses(n: usize) -> Vec<[f64; 5]> {
        (0..n)
            .map(|i| {
                let t = i as f64 / n.max(1) as f64;
                [
                    1.0 + t,
                    1.0 + (1.0 - t),
                    1.0 + 0.5 * t,
                    1.0 + 0.5 * (1.0 - t),
                    1.0 + 0.3 * t,
                ]
            })
            .collect()
    }

    // ── RED tests for selection.rs (PR-8d2 REQ-007) ──────────────────────────

    /// REQ-007 Scenario: selection returns exactly n individuals.
    #[test]
    fn test_select_environmental_returns_n() {
        let fitnesses = synthetic_fitnesses(20);
        let ref_points = uniform_reference_points(5, 4);
        let mut rng = make_rng();
        let selected = select_environmental(&fitnesses, 10, &ref_points, &mut rng);
        assert_eq!(selected.len(), 10, "must return exactly 10 individuals");
    }

    /// Larger population with n=100.
    #[test]
    fn test_select_environmental_returns_n_large() {
        let fitnesses = synthetic_fitnesses(200);
        let ref_points = uniform_reference_points(5, 4);
        let mut rng = make_rng();
        let selected = select_environmental(&fitnesses, 100, &ref_points, &mut rng);
        assert_eq!(selected.len(), 100, "must return exactly 100 from 200");
    }

    /// When pop_size ≤ n all individuals are returned.
    #[test]
    fn test_select_environmental_all_returned_when_pop_le_n() {
        let fitnesses = synthetic_fitnesses(5);
        let ref_points = uniform_reference_points(5, 4);
        let mut rng = make_rng();
        let selected = select_environmental(&fitnesses, 10, &ref_points, &mut rng);
        assert_eq!(selected.len(), 5, "all 5 returned when n > pop");
    }

    /// Returned indices are within valid range.
    #[test]
    fn test_select_environmental_valid_indices() {
        let n_pop = 50;
        let fitnesses = synthetic_fitnesses(n_pop);
        let ref_points = uniform_reference_points(5, 4);
        let mut rng = make_rng();
        let selected = select_environmental(&fitnesses, 25, &ref_points, &mut rng);
        for &idx in &selected {
            assert!(idx < n_pop, "index {idx} out of range");
        }
    }

    /// No duplicate indices.
    #[test]
    fn test_select_environmental_no_duplicates() {
        let fitnesses = synthetic_fitnesses(40);
        let ref_points = uniform_reference_points(5, 4);
        let mut rng = make_rng();
        let selected = select_environmental(&fitnesses, 20, &ref_points, &mut rng);
        let unique: std::collections::BTreeSet<usize> = selected.iter().copied().collect();
        assert_eq!(unique.len(), selected.len(), "no duplicates allowed");
    }

    /// sel_nsga3 convenience wrapper returns correct count.
    #[test]
    fn test_sel_nsga3_returns_correct_count() {
        let fitnesses = synthetic_fitnesses(30);
        let mut rng = make_rng();
        let selected = sel_nsga3(&fitnesses, 15, &mut rng);
        assert_eq!(selected.len(), 15);
    }

    /// Empty population returns empty selection.
    #[test]
    fn test_select_environmental_empty_population() {
        let ref_points = uniform_reference_points(5, 4);
        let mut rng = make_rng();
        let selected = select_environmental(&[], 10, &ref_points, &mut rng);
        assert!(selected.is_empty());
    }

    /// n=0 returns empty selection.
    #[test]
    fn test_select_environmental_n_zero() {
        let fitnesses = synthetic_fitnesses(10);
        let ref_points = uniform_reference_points(5, 4);
        let mut rng = make_rng();
        let selected = select_environmental(&fitnesses, 0, &ref_points, &mut rng);
        assert!(selected.is_empty());
    }

    /// REQ-007 Scenario: lower-niche-count candidate is preferred over higher.
    #[test]
    fn test_select_environmental_lower_niche_preferred() {
        let fitnesses: &[[f64; 5]] = &[
            [1.0, 2.0, 0.0, 0.0, 0.0], // 0 — front 0
            [2.0, 1.0, 0.0, 0.0, 0.0], // 1 — front 0
            [3.0, 3.0, 0.0, 0.0, 0.0], // 2 — front 1
            [4.0, 4.0, 0.0, 0.0, 0.0], // 3 — front 1
        ];
        let ref_points = uniform_reference_points(5, 4);
        let mut rng = make_rng();
        let selected = select_environmental(fitnesses, 3, &ref_points, &mut rng);
        assert_eq!(selected.len(), 3, "must select exactly 3");
        let set: std::collections::BTreeSet<usize> = selected.iter().copied().collect();
        assert!(set.contains(&0), "front-0 individual 0 must be selected");
        assert!(set.contains(&1), "front-0 individual 1 must be selected");
    }

    /// Determinism: same seed produces same selection.
    #[test]
    fn test_select_environmental_deterministic() {
        let fitnesses = synthetic_fitnesses(30);
        let ref_points = uniform_reference_points(5, 4);

        let mut rng1 = ChaCha20Rng::seed_from_u64(123);
        let sel1 = select_environmental(&fitnesses, 15, &ref_points, &mut rng1);

        let mut rng2 = ChaCha20Rng::seed_from_u64(123);
        let sel2 = select_environmental(&fitnesses, 15, &ref_points, &mut rng2);

        assert_eq!(
            sel1, sel2,
            "identical seeds must produce identical selection"
        );
    }
}
