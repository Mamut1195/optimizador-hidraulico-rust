//! NSGA-III Environmental Selection — top-level wrapper (PR-8d2, REQ-007).
//!
//! Implementation pending (GREEN phase). This file contains only the test suite.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nsga3::reference_points::uniform_reference_points;
    use rand::SeedableRng;
    use rand_chacha::ChaCha20Rng;

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

        assert_eq!(sel1, sel2, "identical seeds must produce identical selection");
    }
}
