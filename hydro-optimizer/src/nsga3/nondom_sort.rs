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
pub fn fast_nondominated_sort(fitnesses: &[[f64; 5]]) -> Vec<Vec<usize>> {
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
}
