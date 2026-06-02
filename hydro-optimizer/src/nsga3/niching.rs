//! NSGA-III Niching — association and niche-count selection.
//!
//! Implements the niching procedure from Deb & Jain 2014 §4.2:
//! 1. **Associate** each individual to the nearest reference point
//!    (minimum perpendicular distance from the individual to the reference line).
//! 2. Build **niche counts** over the "definite" set (fronts fully included in selection).
//! 3. **Select** individuals from the partial (splitting) front by preferring the reference
//!    with the lowest niche count; tie-break by minimum perpendicular distance,
//!    then by smallest index (deterministic).
//!
//! Design choice: `rng` is injected but used only when two candidates have
//! identical `(niche_count, distance)` — mirrors DEAP's tie-break behavior.

use rand::RngCore;

/// Perpendicular distance from point `p` to the reference line through the origin
/// defined by direction vector `r`.
///
/// `d_perp = sqrt(||p||² − (p·r)² / ||r||²)`
/// (reference vectors sum to 1 but are not necessarily unit length; we normalize).
pub(crate) fn perp_distance(p: &[f64; 5], r: &[f64]) -> f64 {
    debug_assert_eq!(r.len(), 5, "reference vector must have 5 components");

    // dot(p, r)
    let dot_pr: f64 = p.iter().zip(r).map(|(a, b)| a * b).sum();
    // ||r||²
    let r_sq: f64 = r.iter().map(|x| x * x).sum();

    if r_sq < 1e-30 {
        // Degenerate reference — distance = ||p||
        return p.iter().map(|x| x * x).sum::<f64>().sqrt();
    }

    // ||p||²
    let p_sq: f64 = p.iter().map(|x| x * x).sum();
    // d² = ||p||² − (p·r)²/||r||²
    let d_sq = (p_sq - dot_pr * dot_pr / r_sq).max(0.0);
    d_sq.sqrt()
}

/// Associate each normalized individual to its nearest reference point.
///
/// # Arguments
/// * `normalized` — normalized fitness vectors, one per individual.
/// * `refs`       — reference point vectors (rows from Das-Dennis lattice).
///
/// # Returns
/// A vector of `(ref_idx, distance)` pairs, one per individual.
pub(crate) fn associate(normalized: &[[f64; 5]], refs: &[Vec<f64>]) -> Vec<(usize, f64)> {
    normalized
        .iter()
        .map(|p| {
            let mut best_ref = 0_usize;
            let mut best_dist = f64::INFINITY;
            for (r_idx, r) in refs.iter().enumerate() {
                let d = perp_distance(p, r.as_slice());
                if d < best_dist {
                    best_dist = d;
                    best_ref = r_idx;
                }
            }
            (best_ref, best_dist)
        })
        .collect()
}

/// Build niche counts for a subset of individuals (the "definite" set).
///
/// # Arguments
/// * `subset`      — indices of individuals in the definite set.
/// * `assoc`       — associations for ALL individuals (indexed by individual index).
/// * `num_refs`    — total number of reference points.
///
/// # Returns
/// A `Vec<u32>` of length `num_refs` with niche counts.
pub(crate) fn build_niche_counts(
    subset: &[usize],
    assoc: &[(usize, f64)],
    num_refs: usize,
) -> Vec<u32> {
    let mut counts = vec![0_u32; num_refs];
    for &idx in subset {
        let (ref_idx, _) = assoc[idx];
        counts[ref_idx] += 1;
    }
    counts
}

/// Select `needed` individuals from `partial_front` to fill the next-generation population.
///
/// Uses niche-count-based selection:
/// - Find the reference point `j*` with minimum niche count (among refs associated to partial_front).
/// - Among partial-front individuals associated with `j*`, pick the one with minimum distance
///   (tie on distance: pick the one with the smallest index — deterministic).
/// - Increment niche count for `j*` and repeat until `needed` individuals are chosen.
///
/// `rng` is reserved for future stochastic tie-breaking (mirrors DEAP) but is not consumed
/// in the current deterministic path.
///
/// # Arguments
/// * `partial_front` — indices of individuals in the splitting front.
/// * `assoc`         — full association table (indexed by individual index).
/// * `niche_count`   — mutable niche counts (updated as selections are made).
/// * `needed`        — how many individuals to select.
/// * `_rng`          — RNG for stochastic tie-breaking (reserved).
///
/// # Returns
/// Indices of the selected individuals (length == `needed`).
pub(crate) fn select_from_partial_front(
    partial_front: &[usize],
    assoc: &[(usize, f64)],
    niche_count: &mut [u32],
    needed: usize,
    _rng: &mut dyn RngCore,
) -> Vec<usize> {
    let mut remaining: Vec<usize> = partial_front.to_vec();
    let mut selected: Vec<usize> = Vec::with_capacity(needed);

    while selected.len() < needed && !remaining.is_empty() {
        // Find the minimum niche count among references associated to remaining individuals.
        let min_nc = remaining
            .iter()
            .map(|&idx| niche_count[assoc[idx].0])
            .min()
            .unwrap_or(0);

        // Collect candidates associated with a ref point that has the minimum niche count.
        let candidates: Vec<usize> = remaining
            .iter()
            .copied()
            .filter(|&idx| niche_count[assoc[idx].0] == min_nc)
            .collect();

        // Among candidates, pick the one with the smallest perpendicular distance.
        // Tie-break by smallest individual index (deterministic).
        let chosen = candidates
            .iter()
            .copied()
            .min_by(|&a, &b| {
                let da = assoc[a].1;
                let db = assoc[b].1;
                da.partial_cmp(&db)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a.cmp(&b))
            })
            .expect("candidates must be non-empty");

        // Update niche count for the chosen reference point.
        let chosen_ref = assoc[chosen].0;
        niche_count[chosen_ref] += 1;

        // Remove chosen from remaining.
        remaining.retain(|&idx| idx != chosen);
        selected.push(chosen);
    }

    selected
}

// ─────────────────────────────────────────────────────────────────────────────
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
        let normalized: &[[f64; 5]] = &[[0.9, 0.1, 0.0, 0.0, 0.0], [0.1, 0.9, 0.0, 0.0, 0.0]];
        let refs: Vec<Vec<f64>> =
            vec![vec![1.0, 0.0, 0.0, 0.0, 0.0], vec![0.0, 1.0, 0.0, 0.0, 0.0]];
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
        assert_eq!(
            selected[0], 0,
            "individual 0 (ref niche=0) must be selected"
        );
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
        assert_eq!(
            selected[0], 1,
            "individual 1 (smaller distance) must be selected"
        );
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
