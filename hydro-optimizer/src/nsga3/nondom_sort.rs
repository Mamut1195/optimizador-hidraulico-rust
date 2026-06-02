//! Fast nondominated sort — Deb 2002 (NSGA-II paper Algorithm 1).
//!
//! Complexity: O(M · N²) where M = number of objectives, N = population size.
//! Design choice: faithful Deb 2002 implementation for DEAP parity (§4.1).
//!
//! Returns a `Vec<Vec<usize>>` where `fronts[0]` contains the Pareto-front indices
//! (rank 1), `fronts[1]` rank 2, etc.  The concatenation of all fronts equals
//! the full index range `0..fitnesses.len()`.

/// Determine whether individual `p` dominates individual `q` under minimisation.
///
/// `p` dominates `q` iff:
///   - `p` is no worse than `q` in all objectives, AND
///   - `p` is strictly better in at least one objective.
#[inline]
fn dominates(p: &[f64; 5], q: &[f64; 5]) -> bool {
    let mut strictly_better = false;
    for i in 0..5 {
        if p[i] > q[i] {
            return false; // p is worse in at least one objective
        }
        if p[i] < q[i] {
            strictly_better = true;
        }
    }
    strictly_better
}

/// Fast nondominated sort (Deb 2002).
///
/// # Arguments
/// * `fitnesses` — slice of 5-objective fitness vectors.
///
/// # Returns
/// A vector of fronts; `fronts[i]` is the list of individual indices at rank `i+1`.
/// Equal-objective individuals are not mutually dominating (tie-break is insertion order).
pub(crate) fn fast_nondominated_sort(fitnesses: &[[f64; 5]]) -> Vec<Vec<usize>> {
    let n = fitnesses.len();
    if n == 0 {
        return Vec::new();
    }

    // domination_count[p] = number of individuals that dominate p
    let mut domination_count: Vec<usize> = vec![0; n];
    // dominated_set[p] = indices dominated by p
    let mut dominated_set: Vec<Vec<usize>> = vec![Vec::new(); n];

    let mut front0: Vec<usize> = Vec::new();

    for p in 0..n {
        for q in 0..n {
            if p == q {
                continue;
            }
            if dominates(&fitnesses[p], &fitnesses[q]) {
                dominated_set[p].push(q);
            } else if dominates(&fitnesses[q], &fitnesses[p]) {
                domination_count[p] += 1;
            }
        }
        if domination_count[p] == 0 {
            front0.push(p);
        }
    }

    let mut fronts: Vec<Vec<usize>> = Vec::new();
    let mut current_front = front0;

    while !current_front.is_empty() {
        let mut next_front: Vec<usize> = Vec::new();
        for &p in &current_front {
            for &q in &dominated_set[p] {
                domination_count[q] -= 1;
                if domination_count[q] == 0 {
                    next_front.push(q);
                }
            }
        }
        fronts.push(current_front);
        current_front = next_front;
    }

    fronts
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn dominates_strict() {
        let a = [1.0_f64, 1.0, 1.0, 1.0, 1.0];
        let b = [2.0_f64, 2.0, 2.0, 2.0, 2.0];
        assert!(dominates(&a, &b));
        assert!(!dominates(&b, &a));
    }

    #[test]
    fn dominates_equal_is_not_domination() {
        let a = [1.0_f64; 5];
        assert!(!dominates(&a, &a));
    }

    #[test]
    fn dominates_partial_tradeoff() {
        let a = [1.0_f64, 2.0, 0.0, 0.0, 0.0];
        let b = [2.0_f64, 1.0, 0.0, 0.0, 0.0];
        assert!(!dominates(&a, &b));
        assert!(!dominates(&b, &a));
    }

    // ── REQ-005: fast_nondominated_sort — moved from tests/pr8d1_nsga3_sort_refpoints.rs ──

    /// Known 3-point, 5-objective population (2 active objectives only).
    ///
    /// A = (1, 2, 0, 0, 0): nondominated
    /// B = (2, 1, 0, 0, 0): nondominated (A does not dominate B or vice-versa)
    /// C = (3, 3, 0, 0, 0): dominated by both A and B
    ///
    /// Expected: front[0] = {0, 1}, front[1] = {2}
    #[test]
    fn test_nondom_sort_known_3point_2obj() {
        let fitnesses: &[[f64; 5]] = &[
            [1.0, 2.0, 0.0, 0.0, 0.0], // A — index 0
            [2.0, 1.0, 0.0, 0.0, 0.0], // B — index 1
            [3.0, 3.0, 0.0, 0.0, 0.0], // C — index 2
        ];
        let fronts = fast_nondominated_sort(fitnesses);
        assert_eq!(fronts.len(), 2, "expected exactly 2 fronts");

        let front0: std::collections::BTreeSet<usize> = fronts[0].iter().copied().collect();
        let front1: std::collections::BTreeSet<usize> = fronts[1].iter().copied().collect();

        assert!(front0.contains(&0), "A must be in front 0");
        assert!(front0.contains(&1), "B must be in front 0");
        assert_eq!(
            front0.len(),
            2,
            "front 0 must contain exactly 2 individuals"
        );
        assert!(front1.contains(&2), "C must be in front 1");
        assert_eq!(front1.len(), 1, "front 1 must contain exactly 1 individual");
    }

    /// REQ-005 Scenario: All-identical objectives → every individual gets rank 1.
    #[test]
    fn test_nondom_sort_all_identical_objectives() {
        let fitnesses: &[[f64; 5]] = &[
            [1.0, 1.0, 1.0, 1.0, 1.0],
            [1.0, 1.0, 1.0, 1.0, 1.0],
            [1.0, 1.0, 1.0, 1.0, 1.0],
            [1.0, 1.0, 1.0, 1.0, 1.0],
        ];
        let fronts = fast_nondominated_sort(fitnesses);
        assert_eq!(fronts.len(), 1, "all identical → single front");
        assert_eq!(fronts[0].len(), 4, "all 4 individuals in front 0");
    }

    /// Strictly dominated chain: A=(1,1,1,1,1) dominates B=(2,2,2,2,2) dominates C=(3,3,3,3,3).
    /// Expected: 3 separate fronts, one individual each.
    #[test]
    fn test_nondom_sort_strictly_dominated_chain() {
        let fitnesses: &[[f64; 5]] = &[
            [1.0, 1.0, 1.0, 1.0, 1.0], // best — index 0
            [2.0, 2.0, 2.0, 2.0, 2.0], // middle — index 1
            [3.0, 3.0, 3.0, 3.0, 3.0], // worst — index 2
        ];
        let fronts = fast_nondominated_sort(fitnesses);
        assert_eq!(fronts.len(), 3, "strict chain → 3 fronts");
        assert_eq!(fronts[0], vec![0]);
        assert_eq!(fronts[1], vec![1]);
        assert_eq!(fronts[2], vec![2]);
    }

    /// Single individual always gets rank 1 (front 0).
    #[test]
    fn test_nondom_sort_single_individual() {
        let fitnesses: &[[f64; 5]] = &[[5.0, 3.0, 1.0, 2.0, 4.0]];
        let fronts = fast_nondominated_sort(fitnesses);
        assert_eq!(fronts.len(), 1);
        assert_eq!(fronts[0], vec![0]);
    }

    /// Empty population returns empty fronts (no panic).
    #[test]
    fn test_nondom_sort_empty_population() {
        let fitnesses: &[[f64; 5]] = &[];
        let fronts = fast_nondominated_sort(fitnesses);
        assert!(fronts.is_empty());
    }

    /// All individuals across fronts account for every input index (no index dropped).
    #[test]
    fn test_nondom_sort_all_indices_covered() {
        let fitnesses: &[[f64; 5]] = &[
            [1.0, 5.0, 0.0, 0.0, 0.0],
            [3.0, 3.0, 0.0, 0.0, 0.0],
            [5.0, 1.0, 0.0, 0.0, 0.0],
            [4.0, 4.0, 0.0, 0.0, 0.0],
            [2.0, 6.0, 0.0, 0.0, 0.0],
        ];
        let fronts = fast_nondominated_sort(fitnesses);
        let mut all_indices: Vec<usize> = fronts.iter().flatten().copied().collect();
        all_indices.sort_unstable();
        assert_eq!(
            all_indices,
            vec![0, 1, 2, 3, 4],
            "all 5 indices must appear"
        );
    }
}
