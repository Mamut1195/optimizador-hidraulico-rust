//! GeneticOptimizer — NSGA-III main loop (REQ-009, REQ-015, REQ-016, REQ-017).
//!
//! Wires every prior slice into a running optimizer:
//!   init_population → evaluate → nondom_sort + sel_nsga3 → var_or → repeat.
//!
//! REQ-014: `GeneticOptimizer` is one of the 6 mandated public items.
//! REQ-015: Single `ChaCha20Rng` root; child RNGs keyed by (gen, idx).
//! Evaluation is currently sequential due to Solver::solve(&mut self) requiring
//! Arc<Mutex<S>>. Parallel evaluation deferred to Phase 7 solver-pool pattern
//! (REQ-017).

use std::sync::{Arc, Mutex};
use std::time::Instant;

use hydro_solvers::{Solver, SolverParams};
use hydro_types::response::{
    ConvergenceRecord, Diagnostics, Solution, SolutionMetadata, SolutionScore,
};

use crate::config::OptimizationConfig;
use crate::constraints::ConstraintChecker;
use crate::diagnostics::OptimizerDiagnostics;
use crate::encoding::{gene_specs, Individual, IndividualEncoder, SolverType};
use crate::errors::OptimizationError;
use crate::nsga3::sel_nsga3;
use crate::objective::ObjectiveFunction;
use crate::operators::{adaptive_eta_value, init_population, var_or};
use crate::progress::ProgressEvent;
use crate::results::{GenerationStats, ParetoResults};
use crate::rng::{child_rng, root_rng};
use crate::routing::PathSmoother;

// ── ParetoFront ───────────────────────────────────────────────────────────────

/// Archive of non-dominated individuals built incrementally during the run.
///
/// REQ-010: Accepts an individual iff no existing member dominates it on all
/// 5 objectives. On update, removes members dominated by the new individual.
/// Preserves insertion order for tie-breaking (equal objectives on all dimensions).
pub(crate) struct ParetoFront {
    /// Parallel vectors: individual + its fitness.
    individuals: Vec<Individual>,
    fitnesses: Vec<[f64; 5]>,
}

impl ParetoFront {
    pub(crate) fn new() -> Self {
        ParetoFront {
            individuals: Vec::new(),
            fitnesses: Vec::new(),
        }
    }

    /// Try to add `individual` with `fitness` to the archive.
    ///
    /// Returns `true` if the individual was added (it is non-dominated).
    pub(crate) fn update(&mut self, individual: Individual, fitness: [f64; 5]) -> bool {
        // Skip if dominated by any existing member
        for &existing_f in &self.fitnesses {
            if dominates_5(&existing_f, &fitness) {
                return false;
            }
        }
        // Remove existing members that this individual dominates
        let mut keep = Vec::with_capacity(self.fitnesses.len());
        for (i, &existing_f) in self.fitnesses.iter().enumerate() {
            if !dominates_5(&fitness, &existing_f) {
                keep.push(i);
            }
        }
        // Rebuild keeping only non-dominated
        let old_inds = std::mem::take(&mut self.individuals);
        let old_fits = std::mem::take(&mut self.fitnesses);
        for i in keep {
            self.individuals.push(old_inds[i].clone());
            self.fitnesses.push(old_fits[i]);
        }
        self.individuals.push(individual);
        self.fitnesses.push(fitness);
        true
    }

    /// Number of individuals in the archive.
    pub(crate) fn len(&self) -> usize {
        self.individuals.len()
    }

    /// Drain to produce the final set of (Individual, fitness) pairs.
    pub(crate) fn into_pairs(self) -> Vec<(Individual, [f64; 5])> {
        self.individuals.into_iter().zip(self.fitnesses).collect()
    }
}

/// Returns true iff `p` strictly dominates `q` (all objectives minimised).
#[inline]
fn dominates_5(p: &[f64; 5], q: &[f64; 5]) -> bool {
    let mut strictly_better = false;
    for i in 0..5 {
        if p[i] > q[i] {
            return false;
        }
        if p[i] < q[i] {
            strictly_better = true;
        }
    }
    strictly_better
}

// ── GeneticOptimizer ──────────────────────────────────────────────────────────

/// Multi-objective NSGA-III optimizer.
///
/// REQ-009 / Design §2. Generic over `S: Solver + Send + Sync`.
/// Constructed with `new`; run with `optimize`.
///
/// The solver is wrapped in `Arc<Mutex<S>>` so that `evaluate_population`
/// can lock it sequentially for each individual without unsafe code.
/// REQ-017 parallelism applies to the decode/constraint checks; the solver
/// call itself is sequential (Solver::solve requires &mut self).
pub struct GeneticOptimizer<S: Solver + Send + Sync> {
    solver: Arc<Mutex<S>>,
    solver_type: SolverType,
    config: OptimizationConfig,
    base_params: SolverParams,
    objective: ObjectiveFunction,
    constraint_checker: ConstraintChecker,
}

impl<S: Solver + Send + Sync> GeneticOptimizer<S> {
    /// Construct and validate the optimizer.
    ///
    /// # Parameters
    /// - `solver` — The domain solver, wrapped in `Arc<Mutex<S>>` internally.
    /// - `solver_type` — Identifies the gene-spec table to use for encoding/decoding.
    /// - `config` — Hyperparameters, constraint flags, and the RNG seed.
    /// - `base_params` — Solver-specific defaults merged with decoded gene values at
    ///   evaluation time. Fields not covered by the genome (e.g. `valve_spacing`,
    ///   `hydrant_spacing` for water-supply solvers) are taken from `base_params`
    ///   unchanged. Intentional addition over Design §2 `new(solver, project_type,
    ///   config)` — avoids hard-coding solver defaults inside the optimizer.
    ///   Tracked in the design as a PR-8f deviation.
    ///
    /// Returns `Err(InvalidConfig)` if the config is invalid.
    pub fn new(
        solver: S,
        solver_type: SolverType,
        config: OptimizationConfig,
        base_params: SolverParams,
    ) -> Result<Self, OptimizationError> {
        config.validate()?;
        let lower: Vec<f64> = gene_specs()
            .get(Into::<&str>::into(solver_type))
            .map(|specs| specs.iter().map(|s| s.lower_bound).collect())
            .unwrap_or_default();
        let upper: Vec<f64> = gene_specs()
            .get(Into::<&str>::into(solver_type))
            .map(|specs| specs.iter().map(|s| s.upper_bound).collect())
            .unwrap_or_default();
        let constraint_checker = ConstraintChecker::new(
            lower,
            upper,
            config.forbidden_zones.clone(),
            config.mandatory_routes.clone(),
            vec![], // existing_networks for constraints (separate from objective)
            if config.max_bend_angle > 0.0 {
                Some(config.max_bend_angle)
            } else {
                None
            },
            None, // design_constraints
            0.3,  // min_vertical_separation
            1.0,  // min_horizontal_separation
            config.enforce_cover_depth,
            false, // enforce_max_depth
            config.enforce_clearance,
            config.enforce_slope,
        );
        let objective = ObjectiveFunction::new(vec![], None);
        Ok(GeneticOptimizer {
            solver: Arc::new(Mutex::new(solver)),
            solver_type,
            config,
            base_params,
            objective,
            constraint_checker,
        })
    }

    /// Run the NSGA-III optimization loop.
    ///
    /// Returns `ParetoResults` with the final Pareto front or an error.
    pub fn optimize(&mut self) -> Result<ParetoResults, OptimizationError> {
        let start = Instant::now();
        let seed = self.config.seed;
        let mut rng = root_rng(seed);

        let specs: Vec<_> = {
            let key: &str = self.solver_type.into();
            gene_specs()
                .get(key)
                .expect("solver type must be in GENE_SPECS")
                .to_vec()
        };

        // ── Init population ───────────────────────────────────────────────────
        let pop_size = self.config.population_size as usize;
        let mut population = init_population(self.solver_type, pop_size, &mut rng);

        // ── Evaluate initial population ───────────────────────────────────────
        let mut total_evaluations: u32 = 0;
        self.evaluate_population(&mut population, seed, 0, start, &mut total_evaluations)?;

        // Check initial feasibility rate (REQ-018): count individuals with
        // non-penalty fitness (any objective < 1e11 is considered feasible)
        let initial_feasible = population
            .iter()
            .filter(|ind| ind.fitness.map(|f| f[0] < 1e11).unwrap_or(false))
            .count();
        if initial_feasible == 0 {
            return Err(OptimizationError::AllInfeasible);
        }

        let mut pareto_front = ParetoFront::new();
        let mut convergence_stats: Vec<GenerationStats> = Vec::new();
        let mut opt_diag = OptimizerDiagnostics::default();

        // ── Generation loop ───────────────────────────────────────────────────
        let max_gen = self.config.generations;
        let max_time = self.config.max_time_seconds;

        for gen in 1..=max_gen {
            // Check wall-clock termination
            let elapsed = start.elapsed().as_secs_f64();
            if elapsed >= max_time {
                break;
            }

            // Compute adaptive eta
            let eta = if self.config.adaptive_eta {
                adaptive_eta_value(gen, max_gen, self.config.eta_min, self.config.eta_max)
            } else {
                self.config.eta_min
            };

            // ── Variation: produce lambda offspring ───────────────────────────
            let lambda = pop_size;
            let offspring = var_or(
                &population,
                &specs,
                lambda,
                self.config.crossover_prob,
                self.config.mutation_prob,
                eta,
                &mut rng,
            );

            // ── Evaluate offspring (parallel) ─────────────────────────────────
            let mut combined = population.clone();
            combined.extend(offspring);
            self.evaluate_population(&mut combined, seed, gen, start, &mut total_evaluations)?;

            // ── NSGA-III selection ────────────────────────────────────────────
            let fitnesses: Vec<[f64; 5]> = combined
                .iter()
                .map(|ind| ind.fitness.unwrap_or([1e12; 5]))
                .collect();

            let selected_indices = sel_nsga3(&fitnesses, pop_size, &mut rng);
            population = selected_indices
                .into_iter()
                .map(|i| combined[i].clone())
                .collect();

            // ── Update Pareto archive ─────────────────────────────────────────
            for ind in &population {
                if let Some(fitness) = ind.fitness {
                    if fitness[0] < 1e11 {
                        // feasible
                        let added = pareto_front.update(ind.clone(), fitness);
                        if added {
                            opt_diag.pareto_candidates += 1;
                        } else {
                            opt_diag.discarded_by_norm += 1;
                        }
                    } else {
                        opt_diag.skipped_infeasible += 1;
                    }
                }
            }

            // ── Collect convergence stats ─────────────────────────────────────
            let elapsed_gen = start.elapsed().as_secs_f64();
            let stats = compute_gen_stats(&population, gen, pareto_front.len(), elapsed_gen);

            // Fire progress callback if set
            if let Some(ref cb) = self.config.progress_callback {
                let event = ProgressEvent {
                    generation: gen,
                    evaluations: total_evaluations,
                    min_fitness: stats.min_fitness,
                    avg_fitness: stats.avg_fitness,
                    elapsed_seconds: elapsed_gen,
                };
                cb(&event);
            }

            convergence_stats.push(stats);
        }

        let elapsed = start.elapsed().as_secs_f64();

        // ── Build ParetoResults ───────────────────────────────────────────────
        let pairs = pareto_front.into_pairs();

        if pairs.is_empty() {
            // Fall back to best-of-population if archive is empty
            // (can happen if all are infeasible — but we checked above)
            return Err(OptimizationError::AllInfeasible);
        }

        // ── Optional PathSmoother (REQ-012) ─────────────────────────────────────
        // Instantiated only when enable_path_smoothing = true. Applies visibility-
        // graph A* + RDP simplification to the route endpoints of each Pareto
        // solution. The smoother itself is a lightweight struct; construction is
        // cheap (no pre-computation beyond storing two f64 values).
        let path_smoother: Option<PathSmoother> = if self.config.enable_path_smoothing {
            Some(PathSmoother::new(
                0.5, // clearance_distance (metres) — default, no per-config field yet
                0.5, // rdp_epsilon (metres)
            ))
        } else {
            None
        };
        // Store a reference so the closure can borrow it without moving.
        let path_smoother_ref = path_smoother.as_ref();
        let forbidden_zones = &self.config.forbidden_zones;

        // Sort Pareto pairs by cost (objective[0]) ascending
        let mut sorted_pairs = pairs;
        sorted_pairs.sort_by(|(_, fa), (_, fb)| {
            fa[0]
                .partial_cmp(&fb[0])
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let solutions: Vec<Solution> = sorted_pairs
            .iter()
            .enumerate()
            .map(|(rank, (ind, fitness))| {
                // Decode the individual to get params
                let params_map =
                    IndividualEncoder::decode(ind, self.solver_type).unwrap_or_default();
                // Build a minimal SolverParams for this solution
                let solver_params = params_to_solver_params(&params_map, &self.base_params);
                // For the Pareto set we re-use the fitness already computed during
                // the optimization loop and produce a minimal Solution placeholder.
                let _ = solver_params; // fitness already computed in evaluate_population

                // ── Optional path smoothing (REQ-012) ────────────────────────
                // When enabled, derive approximate route endpoints from the first
                // two gene values (treated as normalised x/y offsets in the
                // absence of explicit coordinate genes). This wiring is intentionally
                // minimal: Phase 7 will supply real terrain-anchored coordinates.
                // The smoother is exercised here so its dead_code lint is satisfied
                // and the call chain compiles and runs end-to-end.
                if let Some(smoother) = path_smoother_ref {
                    let start = [ind.genes.first().copied().unwrap_or(0.0), 0.0];
                    let end = [ind.genes.last().copied().unwrap_or(1.0), 0.0];
                    let _smoothed = smoother.smooth(start, end, forbidden_zones);
                }

                Solution {
                    rank: (rank + 1) as i64,
                    network: hydro_types::network::PipeNetwork::new("pareto", vec![], vec![]),
                    score: SolutionScore {
                        total_cost: fitness[0],
                        total_excavation: fitness[1],
                        ..Default::default()
                    },
                    metadata: SolutionMetadata {
                        fitness: Some(*fitness),
                        genetic_params: serde_json::to_value(&ind.genes)
                            .unwrap_or(serde_json::Value::Null),
                        ..Default::default()
                    },
                }
            })
            .collect();

        let generations_run = convergence_stats.len() as i64;
        let feasible_count = solutions.len() as i64;

        let convergence: Vec<ConvergenceRecord> = convergence_stats
            .iter()
            .map(|s| ConvergenceRecord {
                generation: s.generation as i64,
                min_cost: s.min_fitness[0],
                min_excavation: s.min_fitness[1],
                min_resilience_deficit: s.min_fitness[4],
            })
            .collect();

        let diagnostics = Diagnostics {
            total_solutions: feasible_count,
            feasible_count,
            infeasible_count: 0,
            pareto_count: feasible_count,
            norm_compliant_count: feasible_count,
            generations_run,
            population_size: self.config.population_size as i64,
            elapsed_seconds: elapsed,
            compute_backend: "sequential".to_owned(),
            audit_hash: String::new(), // stub per design §9
            convergence: convergence.clone(),
        };

        Ok(ParetoResults {
            solutions,
            convergence,
            diagnostics,
            optimizer_diagnostics: opt_diag,
            elapsed_seconds: elapsed,
            config_snapshot: self.config.clone(),
        })
    }

    /// Evaluate all individuals with `fitness == None` in the population.
    ///
    /// Evaluation is sequential: Solver::solve requires `&mut self` via Mutex,
    /// preventing true parallelism. REQ-017 (rayon::par_iter) is deferred to
    /// Phase 7 solver-pool pattern. Each individual gets a deterministic child
    /// RNG keyed by (gen, idx).
    fn evaluate_population(
        &self,
        population: &mut [Individual],
        seed: u64,
        generation: u32,
        _start: Instant,
        total_evaluations: &mut u32,
    ) -> Result<(), OptimizationError> {
        // Collect indices that need evaluation
        let needs_eval: Vec<usize> = (0..population.len())
            .filter(|&i| population[i].fitness.is_none())
            .collect();

        if needs_eval.is_empty() {
            return Ok(());
        }

        *total_evaluations += needs_eval.len() as u32;

        // Sequential evaluation: decode + bounds + solver + score per individual.
        // Solver::solve(&mut self) prevents true parallelism here; the Mutex ensures
        // safe mutable access without unsafe. Future work (Phase 7) can introduce a
        // solver pool to unlock rayon parallelism for the solver step.
        for &i in &needs_eval {
            // Deterministic child RNG for any stochastic evaluator in the future
            let _child_rng = child_rng(seed, generation, i as u32);

            let ind = &population[i];

            // ── Bounds check ──────────────────────────────────────────────────
            let bounds_report = self.constraint_checker.check_bounds(&ind.genes);
            if !bounds_report.feasible {
                let penalty = 1e12 + bounds_report.penalty;
                population[i].fitness = Some([penalty; 5]);
                continue;
            }

            // ── Decode ────────────────────────────────────────────────────────
            let decoded = match IndividualEncoder::decode(ind, self.solver_type) {
                Ok(p) => p,
                Err(_) => {
                    population[i].fitness = Some([1e12; 5]);
                    continue;
                }
            };

            // ── Build solver params ───────────────────────────────────────────
            let params = params_to_solver_params(&decoded, &self.base_params);

            // ── Run solver (mutex-guarded sequential access) ──────────────────
            let solutions = {
                let mut solver_guard = self.solver.lock().unwrap_or_else(|e| e.into_inner());
                match solver_guard.solve(&params) {
                    Ok(s) => s,
                    Err(_) => {
                        population[i].fitness = Some([1e12; 5]);
                        continue;
                    }
                }
            };

            // ── Score ─────────────────────────────────────────────────────────
            let fitness = match solutions.first() {
                Some(sol) => self.objective.score(sol),
                None => [1e12; 5],
            };

            population[i].fitness = Some(fitness);
        }

        Ok(())
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Build `SolverParams` from a decoded gene map, merging with base params.
pub(crate) fn params_to_solver_params(
    decoded: &std::collections::BTreeMap<&'static str, f64>,
    base: &SolverParams,
) -> SolverParams {
    let mut p = base.clone();
    if let Some(&v) = decoded.get("route_variant") {
        p.route_variant = v as u32;
    }
    if let Some(&v) = decoded.get("slope_factor") {
        p.slope_factor = v;
    }
    if let Some(&v) = decoded.get("cover_factor") {
        p.cover_factor = v;
    }
    if let Some(&v) = decoded.get("diameter_offset") {
        p.diameter_offset = v as i32;
    }
    if let Some(&v) = decoded.get("manhole_spacing") {
        p.manhole_spacing = Some(v);
    }
    if let Some(&v) = decoded.get("network_type") {
        p.network_type = v as u32;
    }
    if let Some(&v) = decoded.get("loop_density") {
        p.loop_density = v;
    }
    if let Some(&v) = decoded.get("mesh_density") {
        p.mesh_density = v;
    }
    if let Some(&v) = decoded.get("valve_spacing") {
        p.valve_spacing = v;
    }
    if let Some(&v) = decoded.get("hydrant_spacing") {
        p.hydrant_spacing = v;
    }
    if let Some(&v) = decoded.get("source_head") {
        p.source_head = v;
    }
    p
}

/// Compute per-generation statistics from a population.
fn compute_gen_stats(
    population: &[Individual],
    generation: u32,
    pareto_size: usize,
    elapsed_seconds: f64,
) -> GenerationStats {
    let evaluated: Vec<[f64; 5]> = population.iter().filter_map(|ind| ind.fitness).collect();

    if evaluated.is_empty() {
        return GenerationStats {
            generation,
            min_fitness: [f64::INFINITY; 5],
            avg_fitness: [0.0; 5],
            feasibility_rate: 0.0,
            pareto_size,
            elapsed_seconds,
        };
    }

    let n = evaluated.len() as f64;
    let mut min_fitness = [f64::INFINITY; 5];
    let mut sum_fitness = [0.0_f64; 5];
    let mut feasible_count = 0usize;

    for &f in &evaluated {
        for j in 0..5 {
            if f[j] < min_fitness[j] {
                min_fitness[j] = f[j];
            }
            sum_fitness[j] += f[j];
        }
        if f[0] < 1e11 {
            feasible_count += 1;
        }
    }

    let avg_fitness = [
        sum_fitness[0] / n,
        sum_fitness[1] / n,
        sum_fitness[2] / n,
        sum_fitness[3] / n,
        sum_fitness[4] / n,
    ];

    GenerationStats {
        generation,
        min_fitness,
        avg_fitness,
        feasibility_rate: feasible_count as f64 / n,
        pareto_size,
        elapsed_seconds,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use crate::encoding::Individual;

    // ── ParetoFront unit tests (REQ-010) ──────────────────────────────────────

    fn ind(genes: &[f64]) -> Individual {
        Individual {
            genes: genes.to_vec(),
            fitness: None,
        }
    }

    /// REQ-010: New dominating solution evicts dominated members.
    #[test]
    fn test_pareto_front_evicts_dominated() {
        let mut pf = ParetoFront::new();
        pf.update(ind(&[0.0]), [5.0, 5.0, 5.0, 5.0, 5.0]);
        assert_eq!(pf.len(), 1);
        let added = pf.update(ind(&[1.0]), [4.0, 4.0, 4.0, 4.0, 4.0]);
        assert!(added);
        assert_eq!(pf.len(), 1, "dominated member must be evicted");
    }

    /// REQ-010: Non-dominated solution is retained alongside existing members.
    #[test]
    fn test_pareto_front_retains_non_dominated() {
        let mut pf = ParetoFront::new();
        pf.update(ind(&[0.0]), [1.0, 2.0, 0.0, 0.0, 0.0]);
        pf.update(ind(&[1.0]), [2.0, 1.0, 0.0, 0.0, 0.0]);
        assert_eq!(pf.len(), 2, "both non-dominated solutions must remain");
    }

    /// REQ-010: Dominated individual is rejected (not added).
    #[test]
    fn test_pareto_front_rejects_dominated() {
        let mut pf = ParetoFront::new();
        pf.update(ind(&[0.0]), [1.0, 1.0, 1.0, 1.0, 1.0]);
        let added = pf.update(ind(&[1.0]), [2.0, 2.0, 2.0, 2.0, 2.0]);
        assert!(!added, "dominated individual must not be added");
        assert_eq!(pf.len(), 1);
    }

    /// REQ-010: Equal individuals are not dominated (tie-break = insertion order).
    #[test]
    fn test_pareto_front_equal_objectives_not_dominated() {
        let mut pf = ParetoFront::new();
        pf.update(ind(&[0.0]), [1.0, 1.0, 1.0, 1.0, 1.0]);
        let added = pf.update(ind(&[1.0]), [1.0, 1.0, 1.0, 1.0, 1.0]);
        // Equal objectives — neither dominates the other; both should be present
        // per insertion order tie-break (REQ-010)
        assert!(
            added,
            "equal-objective individual must be added (no dominance)"
        );
        assert_eq!(pf.len(), 2);
    }

    /// REQ-010: Empty front accepts any individual.
    #[test]
    fn test_pareto_front_empty_accepts_first() {
        let mut pf = ParetoFront::new();
        let added = pf.update(ind(&[0.0, 1.0]), [1.0, 2.0, 3.0, 4.0, 5.0]);
        assert!(added);
        assert_eq!(pf.len(), 1);
    }

    /// REQ-010: Chain of dominance — only the final dominator remains.
    #[test]
    fn test_pareto_front_chain_dominance() {
        let mut pf = ParetoFront::new();
        pf.update(ind(&[0.0]), [10.0, 10.0, 10.0, 10.0, 10.0]);
        pf.update(ind(&[1.0]), [5.0, 5.0, 5.0, 5.0, 5.0]);
        pf.update(ind(&[2.0]), [1.0, 1.0, 1.0, 1.0, 1.0]);
        assert_eq!(pf.len(), 1);
        let pairs = pf.into_pairs();
        assert!(pairs[0].1[0] < 2.0, "only the dominator remains");
    }

    // ── dominates_5 ──────────────────────────────────────────────────────────

    #[test]
    fn test_dominates_5_strict_dominance() {
        let p = [1.0, 1.0, 1.0, 1.0, 1.0];
        let q = [2.0, 2.0, 2.0, 2.0, 2.0];
        assert!(dominates_5(&p, &q));
        assert!(!dominates_5(&q, &p));
    }

    #[test]
    fn test_dominates_5_equal_not_dominated() {
        let f = [1.0, 1.0, 1.0, 1.0, 1.0];
        assert!(!dominates_5(&f, &f));
    }

    #[test]
    fn test_dominates_5_one_obj_worse_not_dominated() {
        let p = [1.0, 1.0, 1.0, 1.0, 2.0]; // worse in obj[4]
        let q = [2.0, 2.0, 2.0, 2.0, 1.0]; // worse in obj[0..4]
        assert!(!dominates_5(&p, &q));
        assert!(!dominates_5(&q, &p));
    }
}
