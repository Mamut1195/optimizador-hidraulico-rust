//! ParetoResults — the top-level return type of `GeneticOptimizer::optimize`.
//!
//! Design §2 exact struct + accessors. REQ-011.
//!
//! # Public API (REQ-014)
//! `ParetoResults` is one of the 6 mandated public items. Everything internal
//! is `pub(crate)`.

use hydro_types::response::{ConvergenceRecord, Diagnostics, Solution};

use crate::config::OptimizationConfig;

// ── GenerationStats ───────────────────────────────────────────────────────────

/// Per-generation aggregate statistics (REQ-011, convergence_history).
#[derive(Debug, Clone)]
pub struct GenerationStats {
    /// 1-indexed generation number.
    pub generation: u32,
    /// Per-objective minimum fitness across the evaluated population.
    pub min_fitness: [f64; 5],
    /// Per-objective mean fitness across the evaluated population.
    pub avg_fitness: [f64; 5],
    /// Fraction of the population that is gene-level feasible.
    pub feasibility_rate: f64,
    /// Pareto front size after this generation.
    pub pareto_size: usize,
    /// Wall-clock seconds at end of this generation.
    pub elapsed_seconds: f64,
}

// ── ParetoResults ─────────────────────────────────────────────────────────────

/// Top-level result returned by `GeneticOptimizer::optimize`.
///
/// REQ-011 / Design §2.
pub struct ParetoResults {
    /// Pareto-optimal solutions sorted by `score.total_cost` ascending.
    pub solutions: Vec<Solution>,
    /// Per-generation convergence records (legacy format for `hydro_types`).
    pub convergence: Vec<ConvergenceRecord>,
    /// Optimizer diagnostics.
    pub diagnostics: Diagnostics,
    /// Total wall-clock time for the run.
    pub elapsed_seconds: f64,
    /// Snapshot of the config used (for reproducibility / audit).
    pub config_snapshot: OptimizationConfig,
}

impl ParetoResults {
    /// Return the solution with the lowest `total_cost`, or `None` if empty.
    pub fn best_by_cost(&self) -> Option<&Solution> {
        self.solutions.first()
    }

    /// Return the solution with the lowest excavation volume, or `None` if empty.
    pub fn best_by_excavation(&self) -> Option<&Solution> {
        self.solutions.iter().min_by(|a, b| {
            a.score
                .total_excavation
                .partial_cmp(&b.score.total_excavation)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    }

    /// Return the solution with the lowest resilience deficit (best resilience),
    /// or `None` if empty.
    ///
    /// Uses `metadata.fitness[4]` (resilience_deficit objective) if available;
    /// falls back to 0.0.
    pub fn best_by_resilience(&self) -> Option<&Solution> {
        self.solutions.iter().min_by(|a, b| {
            let ra = a
                .metadata
                .fitness
                .map(|f| f[4])
                .unwrap_or(f64::INFINITY);
            let rb = b
                .metadata
                .fitness
                .map(|f| f[4])
                .unwrap_or(f64::INFINITY);
            ra.partial_cmp(&rb)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    }

    /// Return the solution minimising the weighted sum of normalised objectives.
    ///
    /// `weights` = `[w_cost, w_excavation, w_pumping, w_interference, w_resilience]`.
    /// All objectives are normalized to `[0, 1]` by dividing by the maximum value
    /// across the Pareto set. Returns `None` if empty.
    pub fn best_balanced(&self, weights: &[f64; 5]) -> Option<&Solution> {
        if self.solutions.is_empty() {
            return None;
        }
        // Compute per-objective max for normalisation
        let mut obj_max = [0.0_f64; 5];
        for sol in &self.solutions {
            if let Some(f) = sol.metadata.fitness {
                for j in 0..5 {
                    if f[j] > obj_max[j] {
                        obj_max[j] = f[j];
                    }
                }
            }
        }

        self.solutions.iter().min_by(|a, b| {
            let score_a = weighted_score(a, weights, &obj_max);
            let score_b = weighted_score(b, weights, &obj_max);
            score_a
                .partial_cmp(&score_b)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    }

    /// Return the 5-objective fitness matrix (one row per solution).
    ///
    /// Rows where `metadata.fitness` is absent are filled with `[f64::INFINITY; 5]`.
    pub fn comparison_table(&self) -> Vec<[f64; 5]> {
        self.solutions
            .iter()
            .map(|s| s.metadata.fitness.unwrap_or([f64::INFINITY; 5]))
            .collect()
    }

    /// One-paragraph human-readable summary.
    pub fn summary(&self) -> String {
        let n = self.solutions.len();
        let best_cost = self
            .best_by_cost()
            .map(|s| format!("${:.0}", s.score.total_cost))
            .unwrap_or_else(|| "N/A".to_owned());
        format!(
            "Pareto front: {n} solutions. Best by cost: {best_cost}. \
             Elapsed: {:.1}s. Generations: {}.",
            self.elapsed_seconds, self.diagnostics.generations_run
        )
    }
}

fn weighted_score(sol: &Solution, weights: &[f64; 5], obj_max: &[f64; 5]) -> f64 {
    let f = match sol.metadata.fitness {
        Some(x) => x,
        None => return f64::INFINITY,
    };
    let mut s = 0.0;
    for j in 0..5 {
        let norm = if obj_max[j] > 1e-14 {
            f[j] / obj_max[j]
        } else {
            0.0
        };
        s += weights[j] * norm;
    }
    s
}

// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use hydro_types::network::PipeNetwork;
    use hydro_types::response::{SolutionMetadata, SolutionScore};

    fn make_sol(cost: f64, fitness: [f64; 5]) -> Solution {
        Solution {
            rank: 1,
            network: PipeNetwork::new("t", vec![], vec![]),
            score: SolutionScore {
                total_cost: cost,
                ..Default::default()
            },
            metadata: SolutionMetadata {
                fitness: Some(fitness),
                ..Default::default()
            },
        }
    }

    fn make_results(solutions: Vec<Solution>) -> ParetoResults {
        ParetoResults {
            solutions,
            convergence: vec![],
            diagnostics: Diagnostics::default(),
            elapsed_seconds: 1.0,
            config_snapshot: OptimizationConfig::default(),
        }
    }

    #[test]
    fn test_best_by_cost_returns_first() {
        let r = make_results(vec![
            make_sol(300.0, [300.0, 1.0, 1.0, 0.0, 0.1]),
            make_sol(500.0, [500.0, 0.5, 0.5, 0.0, 0.05]),
            make_sol(800.0, [800.0, 2.0, 2.0, 0.0, 0.2]),
        ]);
        let best = r.best_by_cost().unwrap();
        assert!((best.score.total_cost - 300.0).abs() < 1e-9);
    }

    #[test]
    fn test_best_by_cost_returns_none_when_empty() {
        let r = make_results(vec![]);
        assert!(r.best_by_cost().is_none());
    }

    #[test]
    fn test_best_balanced_uses_weights() {
        let r = make_results(vec![
            make_sol(100.0, [100.0, 5.0, 1.0, 0.0, 0.1]),
            make_sol(500.0, [500.0, 1.0, 1.0, 0.0, 0.0]),
        ]);
        // Weight only excavation (obj[1])
        let best = r.best_balanced(&[0.0, 1.0, 0.0, 0.0, 0.0]).unwrap();
        assert!((best.score.total_cost - 500.0).abs() < 1e-9);
    }

    #[test]
    fn test_comparison_table_length_matches_solutions() {
        let r = make_results(vec![
            make_sol(100.0, [100.0, 1.0, 1.0, 0.0, 0.1]),
            make_sol(200.0, [200.0, 2.0, 2.0, 0.0, 0.2]),
        ]);
        let t = r.comparison_table();
        assert_eq!(t.len(), 2);
    }
}
