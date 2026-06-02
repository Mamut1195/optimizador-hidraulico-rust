//! NSGA-III Niching — association and niche-count selection (PR-8d2, REQ-007).
//!
//! Implementation pending (GREEN phase). This file contains only the test suite.

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand_chacha::ChaCha20Rng;

    fn make_rng() -> ChaCha20Rng {
        ChaCha20Rng::seed_from_u64(42)
    }

    // ── RED tests for niching.rs (PR-8d2 REQ-007) ────────────────────────────

    /// Perpendicular distance from a point aligned with the reference line is 0.
    #[test]
    fn test_perp_distance_aligned_is_zero() {
        let p = [1.0_f64, 0.0, 0.0, 0.0, 0.0];
        let r = vec![1.0_f64, 0.0, 0.0, 0.0, 0.0];
        let d = perp_distance(&p, &r);
        assert!(d < 1e-10, "aligned point must have distance 0, got {d}");
    }

    /// Perpendicular distance from (1,0,0,0,0) to ref (0,1,0,0,0) is 1.
    #[test]
    fn test_perp_distance_orthogonal_refs() {
        let p = [1.0_f64, 0.0, 0.0, 0.0, 0.0];
        let r = vec![0.0_f64, 1.0, 0.0, 0.0, 0.0];
        let d = perp_distance(&p, &r);
        assert!(
            (d - 1.0).abs() < 1e-10,
            "perpendicular distance must be 1.0, got {d}"
        );
    }

    /// Point (1,1,0,0,0) aligned with diagonal ref (1,1,0,0,0) → distance = 0.
    #[test]
    fn test_perp_distance_diagonal_aligned() {
        let p = [1.0_f64, 1.0, 0.0, 0.0, 0.0];
        let r = vec![1.0_f64, 1.0, 0.0, 0.0, 0.0];
        let d = perp_distance(&p, &r);
        assert!(d < 1e-10, "aligned diagonal must have d=0, got {d}");
    }

    /// associate picks the nearest reference point for each individual.
    #[test]
    fn test_associate_picks_nearest_ref() {
        let normalized: &[[f64; 5]] = &[
            [0.9, 0.1, 0.0, 0.0, 0.0],
            [0.1, 0.9, 0.0, 0.0, 0.0],
        ];
        let refs: Vec<Vec<f64>> = vec![
            vec![1.0, 0.0, 0.0, 0.0, 0.0],
            vec![0.0, 1.0, 0.0, 0.0, 0.0],
        ];
        let assoc = associate(normalized, &refs);
        assert_eq!(assoc[0].0, 0, "individual 0 should associate to ref 0");
        assert_eq!(assoc[1].0, 1, "individual 1 should associate to ref 1");
    }

    /// associate with a single reference point maps all individuals to index 0.
    #[test]
    fn test_associate_single_ref_all_map_to_zero() {
        let normalized: &[[f64; 5]] = &[
            [1.0, 0.0, 0.0, 0.0, 0.0],
            [0.0, 0.0, 0.0, 0.0, 1.0],
            [0.5, 0.5, 0.0, 0.0, 0.0],
        ];
        let refs: Vec<Vec<f64>> = vec![vec![0.2, 0.2, 0.2, 0.2, 0.2]];
        let assoc = associate(normalized, &refs);
        for (i, &(ref_idx, _)) in assoc.iter().enumerate() {
            assert_eq!(ref_idx, 0, "individual {i} must map to ref 0");
        }
    }

    /// build_niche_counts correctly counts how many subset members map to each ref.
    #[test]
    fn test_build_niche_counts_basic() {
        let assoc: Vec<(usize, f64)> = vec![(0, 0.1), (0, 0.2), (1, 0.3)];
        let subset = vec![0, 1, 2];
        let counts = build_niche_counts(&subset, &assoc, 2);
        assert_eq!(counts[0], 2, "ref 0 should have niche count 2");
        assert_eq!(counts[1], 1, "ref 1 should have niche count 1");
    }

    /// build_niche_counts with an empty subset returns all-zero counts.
    #[test]
    fn test_build_niche_counts_empty_subset() {
        let assoc: Vec<(usize, f64)> = vec![(0, 0.1), (1, 0.2)];
        let counts = build_niche_counts(&[], &assoc, 2);
        assert_eq!(counts, vec![0, 0]);
    }

    /// REQ-007 Scenario: lower niche count candidate is preferred.
    #[test]
    fn test_select_from_partial_front_prefers_lowest_niche() {
        let assoc: Vec<(usize, f64)> = vec![
            (0, 0.5), // individual 0 → ref 0
            (1, 0.5), // individual 1 → ref 1
        ];
        let mut niche_count = vec![0_u32, 1_u32];
        let partial_front = vec![0, 1];
        let mut rng = make_rng();
        let selected =
            select_from_partial_front(&partial_front, &assoc, &mut niche_count, 1, &mut rng);
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0], 0, "individual 0 (ref niche=0) must be selected");
    }

    /// Tie-break by perpendicular distance: smaller distance wins.
    #[test]
    fn test_select_from_partial_front_tie_breaks_by_distance() {
        let assoc: Vec<(usize, f64)> = vec![
            (0, 0.8), // individual 0 → ref 0, d=0.8
            (0, 0.2), // individual 1 → ref 0, d=0.2
        ];
        let mut niche_count = vec![0_u32];
        let partial_front = vec![0, 1];
        let mut rng = make_rng();
        let selected =
            select_from_partial_front(&partial_front, &assoc, &mut niche_count, 1, &mut rng);
        assert_eq!(selected[0], 1, "individual 1 (smaller distance) must be selected");
    }

    /// Selecting all individuals returns all of them.
    #[test]
    fn test_select_from_partial_front_selects_all_if_needed_equals_len() {
        let assoc: Vec<(usize, f64)> = vec![(0, 0.1), (1, 0.2), (0, 0.3)];
        let mut niche_count = vec![0_u32, 0_u32];
        let partial_front = vec![0, 1, 2];
        let mut rng = make_rng();
        let selected =
            select_from_partial_front(&partial_front, &assoc, &mut niche_count, 3, &mut rng);
        assert_eq!(selected.len(), 3);
    }

    /// Niche count is incremented for the chosen reference after each selection.
    #[test]
    fn test_select_from_partial_front_niche_count_incremented() {
        let assoc: Vec<(usize, f64)> = vec![(0, 0.1), (0, 0.2)];
        let mut niche_count = vec![0_u32];
        let partial_front = vec![0, 1];
        let mut rng = make_rng();
        let _ = select_from_partial_front(&partial_front, &assoc, &mut niche_count, 2, &mut rng);
        assert_eq!(niche_count[0], 2);
    }
}
