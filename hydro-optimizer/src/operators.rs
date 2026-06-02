//! Genetic operators: SBX crossover, polynomial mutation, varOr, adaptive eta.
//!
//! Ports DEAP `tools.cxSimulatedBinaryBounded`, `tools.mutPolynomialBounded`,
//! and `algorithms.varOr` faithfully, including the adaptive-eta linear schedule.
//!
//! Design §5 / REQ-008, REQ-014, REQ-015:
//! - All functions take `&mut ChaCha20Rng` — no thread-local RNGs.
//! - Integer genes (`GeneType::Int`) are stored as raw floats in the chromosome.
//!   SBX and polynomial mutation operate on raw float values; `IndividualEncoder::decode`
//!   applies `round()` + clamp for integer genes (matches DEAP behaviour).
//! - All functions are `pub(crate)` (REQ-014).
//!
//! Note: `#[allow(dead_code)]` is kept on functions consumed by `optimizer.rs`
//! (PR-8f). This mirrors the pattern used in `nsga3/mod.rs` until the wiring
//! commit lands.

use rand::Rng;
use rand_chacha::ChaCha20Rng;

use crate::encoding::{GeneSpec, Individual};

// ── adaptive_eta_value ────────────────────────────────────────────────────────

/// Linear interpolation of eta from `eta_min` to `eta_max` over `max_gen` generations.
///
/// Mirrors `GeneticOptimizer._adaptive_eta_value()` in the Python oracle:
///
/// ```text
/// progress = generation / max(max_gen, 1)
/// eta      = eta_min + (eta_max - eta_min) * progress
/// ```
///
/// REQ-008: pure deterministic function, no RNG.
#[allow(dead_code)]
pub(crate) fn adaptive_eta_value(generation: u32, max_gen: u32, eta_min: f64, eta_max: f64) -> f64 {
    let progress = generation as f64 / max_gen.max(1) as f64;
    eta_min + (eta_max - eta_min) * progress
}

// ── sbx_crossover ─────────────────────────────────────────────────────────────

/// Simulated Binary Crossover (SBX) — bounded variant.
///
/// Ports `deap.tools.cxSimulatedBinaryBounded` (Deb & Agrawal, 1995).
/// Operates in-place, invalidating `fitness` on both offspring.
///
/// Integer genes: the raw float chromosome value is operated on by SBX (same
/// as DEAP — integers are stored as floats in the chromosome and decoded later).
#[allow(dead_code)]
pub(crate) fn sbx_crossover(
    parent1: &mut Individual,
    parent2: &mut Individual,
    specs: &[GeneSpec],
    eta: f64,
    rng: &mut ChaCha20Rng,
) {
    debug_assert_eq!(
        parent1.genes.len(),
        parent2.genes.len(),
        "sbx_crossover: chromosome length mismatch"
    );
    debug_assert_eq!(
        parent1.genes.len(),
        specs.len(),
        "sbx_crossover: genes/specs length mismatch"
    );

    for (i, spec) in specs.iter().enumerate() {
        let x1 = parent1.genes[i];
        let x2 = parent2.genes[i];
        let lb = spec.lower_bound;
        let ub = spec.upper_bound;

        // When parents are identical at this gene, skip (DEAP behaviour)
        if (x1 - x2).abs() < 1e-14 {
            continue;
        }

        // Ensure x_lo <= x_hi for the formula
        let (x_lo, x_hi) = if x1 <= x2 { (x1, x2) } else { (x2, x1) };
        let dx = x_hi - x_lo;

        let u: f64 = rng.gen();

        let beta1 = 1.0 + 2.0 * (x_lo - lb) / dx;
        let alpha1 = 2.0 - beta1.powf(-(eta + 1.0));
        let betaq1 = sbx_betaq(u, alpha1, eta);

        let beta2 = 1.0 + 2.0 * (ub - x_hi) / dx;
        let alpha2 = 2.0 - beta2.powf(-(eta + 1.0));
        let betaq2 = sbx_betaq(u, alpha2, eta);

        let c1 = (0.5 * ((x_lo + x_hi) - betaq1 * dx)).clamp(lb, ub);
        let c2 = (0.5 * ((x_lo + x_hi) + betaq2 * dx)).clamp(lb, ub);

        // Assign back preserving original ordering
        if x1 <= x2 {
            parent1.genes[i] = c1;
            parent2.genes[i] = c2;
        } else {
            parent1.genes[i] = c2;
            parent2.genes[i] = c1;
        }
    }

    // Invalidate cached fitness — DEAP semantics
    parent1.fitness = None;
    parent2.fitness = None;
}

/// Compute the SBX spread factor betaq from a uniform sample `u` and `alpha`.
///
/// Implements DEAP `cxSimulatedBinaryBounded` exact formula.
#[inline]
fn sbx_betaq(u: f64, alpha: f64, eta: f64) -> f64 {
    if u <= 1.0 / alpha {
        (u * alpha).powf(1.0 / (eta + 1.0))
    } else {
        (1.0 / (2.0 - u * alpha)).powf(1.0 / (eta + 1.0))
    }
}

// ── polynomial_mutation ───────────────────────────────────────────────────────

/// Polynomial mutation — bounded variant.
///
/// Ports `deap.tools.mutPolynomialBounded` (Deb, 2001).
/// Each gene is mutated independently with probability `indpb = 1 / num_genes`.
/// Operates in-place, invalidating `fitness` when at least one gene is changed.
#[allow(dead_code)]
pub(crate) fn polynomial_mutation(
    individual: &mut Individual,
    specs: &[GeneSpec],
    eta: f64,
    rng: &mut ChaCha20Rng,
) {
    debug_assert_eq!(
        individual.genes.len(),
        specs.len(),
        "polynomial_mutation: genes/specs length mismatch"
    );

    let indpb = 1.0 / specs.len().max(1) as f64;
    let mut mutated = false;

    for (i, spec) in specs.iter().enumerate() {
        if rng.gen::<f64>() > indpb {
            continue;
        }
        mutated = true;

        let x = individual.genes[i];
        let lb = spec.lower_bound;
        let ub = spec.upper_bound;
        let dx = ub - lb;

        if dx < 1e-14 {
            continue; // degenerate gene — leave unchanged
        }

        let u: f64 = rng.gen();
        let delta = poly_mutation_delta(u, x, lb, ub, eta, dx);
        individual.genes[i] = (x + delta * dx).clamp(lb, ub);
    }

    if mutated {
        individual.fitness = None;
    }
}

/// Compute normalised polynomial mutation delta (scaled by range `dx`).
///
/// Implements DEAP `mutPolynomialBounded` inner formula exactly.
/// The returned value is `delta_q` (normalised); caller multiplies by `dx`.
#[inline]
fn poly_mutation_delta(u: f64, x: f64, lb: f64, ub: f64, eta: f64, dx: f64) -> f64 {
    if u < 0.5 {
        let delta_l = (x - lb) / dx;
        let base = 2.0 * u + (1.0 - 2.0 * u) * (1.0 - delta_l).powf(eta + 1.0);
        base.powf(1.0 / (eta + 1.0)) - 1.0
    } else {
        let delta_r = (ub - x) / dx;
        let base = 2.0 * (1.0 - u) + 2.0 * (u - 0.5) * (1.0 - delta_r).powf(eta + 1.0);
        1.0 - base.powf(1.0 / (eta + 1.0))
    }
}

// ── var_or ────────────────────────────────────────────────────────────────────

/// μ+λ offspring production operator.
///
/// Ports `deap.algorithms.varOr` exactly:
/// - For each offspring slot:
///   - With probability `cxpb`: pick two parents, clone, apply SBX, add both (may
///     overshoot by 1 if lambda is odd — DEAP pads to exactly `lambda_`).
///   - With probability `mutpb`: clone one parent, apply polynomial mutation.
///   - Otherwise: clone one parent unchanged (reproduction).
///
/// Produces exactly `lambda_` offspring.
/// Invalidated individuals have `fitness = None`.
///
/// # Panics (debug)
/// Panics if `cxpb + mutpb > 1.0` or `population` is empty.
#[allow(dead_code)]
pub(crate) fn var_or(
    population: &[Individual],
    specs: &[GeneSpec],
    lambda_: usize,
    cxpb: f64,
    mutpb: f64,
    eta: f64,
    rng: &mut ChaCha20Rng,
) -> Vec<Individual> {
    debug_assert!(
        cxpb + mutpb <= 1.0 + 1e-9,
        "var_or: cxpb + mutpb must be <= 1.0, got {}",
        cxpb + mutpb
    );
    debug_assert!(
        !population.is_empty(),
        "var_or: population must not be empty"
    );

    let mut offspring: Vec<Individual> = Vec::with_capacity(lambda_);
    let n = population.len();

    while offspring.len() < lambda_ {
        let u: f64 = rng.gen();

        if u < cxpb {
            // Crossover
            let i1 = rng.gen_range(0..n);
            let i2 = rng.gen_range(0..n);
            let mut child1 = population[i1].clone();
            let mut child2 = population[i2].clone();
            sbx_crossover(&mut child1, &mut child2, specs, eta, rng);
            offspring.push(child1);
            if offspring.len() < lambda_ {
                offspring.push(child2);
            }
        } else if u < cxpb + mutpb {
            // Mutation
            let i = rng.gen_range(0..n);
            let mut child = population[i].clone();
            polynomial_mutation(&mut child, specs, eta, rng);
            offspring.push(child);
        } else {
            // Reproduction — exact clone
            let i = rng.gen_range(0..n);
            offspring.push(population[i].clone());
        }
    }

    offspring
}

// ── init_population ───────────────────────────────────────────────────────────

/// Generate an initial population of `pop_size` random individuals.
///
/// Each individual is created via `IndividualEncoder::random_individual`,
/// with genes uniformly sampled within their respective bounds.
/// `fitness` is `None` on all returned individuals.
///
/// Mirrors `toolbox.population(n=self.config.population_size)` in the oracle.
#[allow(dead_code)]
pub(crate) fn init_population(
    solver_type: crate::encoding::SolverType,
    pop_size: usize,
    rng: &mut ChaCha20Rng,
) -> Vec<Individual> {
    (0..pop_size)
        .map(|_| crate::encoding::IndividualEncoder::random_individual(solver_type, rng))
        .collect()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encoding::{gene_specs, GeneType, Individual, SolverType};
    use crate::rng::root_rng;

    fn sewer_specs() -> &'static [GeneSpec] {
        gene_specs().get("sewer").unwrap().as_slice()
    }

    fn make_individual(genes: Vec<f64>) -> Individual {
        Individual {
            genes,
            fitness: Some([1.0; 5]),
        }
    }

    // ── adaptive_eta_value ────────────────────────────────────────────────────

    #[test]
    fn test_adaptive_eta_at_gen_0_equals_eta_min() {
        let result = adaptive_eta_value(0, 100, 5.0, 50.0);
        assert!((result - 5.0).abs() < 1e-12, "expected 5.0, got {result}");
    }

    #[test]
    fn test_adaptive_eta_at_gen_max_equals_eta_max() {
        let result = adaptive_eta_value(100, 100, 5.0, 50.0);
        assert!((result - 50.0).abs() < 1e-12, "expected 50.0, got {result}");
    }

    #[test]
    fn test_adaptive_eta_at_midpoint_equals_midpoint_value() {
        let result = adaptive_eta_value(50, 100, 5.0, 50.0);
        assert!((result - 27.5).abs() < 1e-12, "expected 27.5, got {result}");
    }

    #[test]
    fn test_adaptive_eta_max_gen_zero_does_not_panic() {
        let result = adaptive_eta_value(0, 0, 5.0, 50.0);
        assert!(result.is_finite(), "should not be NaN or inf");
    }

    #[test]
    fn test_adaptive_eta_monotone_increasing() {
        let etas: Vec<f64> = (0..=10)
            .map(|g| adaptive_eta_value(g, 10, 5.0, 50.0))
            .collect();
        for w in etas.windows(2) {
            assert!(w[1] >= w[0], "eta should be non-decreasing");
        }
    }

    // ── SBX crossover ─────────────────────────────────────────────────────────

    #[test]
    fn test_sbx_offspring_within_bounds() {
        let specs = sewer_specs();
        let mut p1 = make_individual(vec![0.3, 0.6, 1.1, 1.0, 70.0]);
        let mut p2 = make_individual(vec![5.0, 1.8, 1.4, 2.0, 110.0]);
        let mut rng = root_rng(42);

        sbx_crossover(&mut p1, &mut p2, specs, 15.0, &mut rng);

        for (i, spec) in specs.iter().enumerate() {
            assert!(
                p1.genes[i] >= spec.lower_bound && p1.genes[i] <= spec.upper_bound,
                "child1 gene[{i}]={} out of [{}, {}]",
                p1.genes[i],
                spec.lower_bound,
                spec.upper_bound
            );
            assert!(
                p2.genes[i] >= spec.lower_bound && p2.genes[i] <= spec.upper_bound,
                "child2 gene[{i}]={} out of [{}, {}]",
                p2.genes[i],
                spec.lower_bound,
                spec.upper_bound
            );
        }
    }

    #[test]
    fn test_sbx_invalidates_fitness() {
        let specs = sewer_specs();
        let mut p1 = make_individual(vec![0.3, 0.6, 1.1, 1.0, 70.0]);
        let mut p2 = make_individual(vec![5.0, 1.8, 1.4, 2.0, 110.0]);
        let mut rng = root_rng(1);
        sbx_crossover(&mut p1, &mut p2, specs, 15.0, &mut rng);
        assert!(p1.fitness.is_none(), "child1 fitness should be invalidated");
        assert!(p2.fitness.is_none(), "child2 fitness should be invalidated");
    }

    #[test]
    fn test_sbx_identical_parents_unchanged() {
        let specs = sewer_specs();
        let genes = vec![2.0, 1.0, 1.2, 1.0, 80.0];
        let mut p1 = make_individual(genes.clone());
        let mut p2 = make_individual(genes.clone());
        let mut rng = root_rng(7);
        sbx_crossover(&mut p1, &mut p2, specs, 15.0, &mut rng);
        assert_eq!(p1.genes, genes);
        assert_eq!(p2.genes, genes);
    }

    #[test]
    fn test_sbx_deterministic_with_same_seed() {
        let specs = sewer_specs();
        let run = |seed: u64| {
            let mut p1 = make_individual(vec![0.5, 0.8, 1.2, 1.0, 75.0]);
            let mut p2 = make_individual(vec![4.0, 1.5, 1.4, 2.0, 100.0]);
            let mut rng = root_rng(seed);
            sbx_crossover(&mut p1, &mut p2, specs, 15.0, &mut rng);
            (p1.genes.clone(), p2.genes.clone())
        };
        assert_eq!(run(99), run(99), "SBX must be deterministic");
        assert_ne!(run(99), run(100), "different seeds should differ");
    }

    // ── Polynomial mutation ───────────────────────────────────────────────────

    #[test]
    fn test_poly_mutation_offspring_within_bounds() {
        let specs = sewer_specs();
        let mut rng = root_rng(42);
        for _ in 0..100 {
            let mut ind = make_individual(vec![2.0, 1.0, 1.2, 1.0, 80.0]);
            polynomial_mutation(&mut ind, specs, 20.0, &mut rng);
            for (i, spec) in specs.iter().enumerate() {
                assert!(
                    ind.genes[i] >= spec.lower_bound && ind.genes[i] <= spec.upper_bound,
                    "gene[{i}]={} out of [{}, {}]",
                    ind.genes[i],
                    spec.lower_bound,
                    spec.upper_bound
                );
            }
        }
    }

    #[test]
    fn test_poly_mutation_deterministic_with_same_seed() {
        let specs = sewer_specs();
        let run = |seed: u64| {
            let mut ind = make_individual(vec![2.0, 1.0, 1.2, 1.0, 80.0]);
            let mut rng = root_rng(seed);
            polynomial_mutation(&mut ind, specs, 20.0, &mut rng);
            ind.genes.clone()
        };
        assert_eq!(run(55), run(55));
        assert_ne!(run(55), run(56));
    }

    #[test]
    fn test_poly_mutation_invalidates_fitness_when_mutated() {
        let specs = sewer_specs();
        let mut any_mutated = false;
        for seed in 0..200u64 {
            let mut ind = make_individual(vec![2.0, 1.0, 1.2, 1.0, 80.0]);
            let original = ind.genes.clone();
            let mut rng = root_rng(seed);
            polynomial_mutation(&mut ind, specs, 20.0, &mut rng);
            if ind.genes != original {
                assert!(ind.fitness.is_none(), "fitness must be None after mutation");
                any_mutated = true;
                break;
            }
        }
        assert!(any_mutated, "expected at least one mutation in 200 seeds");
    }

    // ── varOr ─────────────────────────────────────────────────────────────────

    #[test]
    fn test_var_or_returns_exactly_lambda_offspring() {
        let specs = sewer_specs();
        let pop: Vec<Individual> = (0..10)
            .map(|_| make_individual(vec![2.0, 1.0, 1.2, 1.0, 80.0]))
            .collect();
        let mut rng = root_rng(42);
        let offspring = var_or(&pop, specs, 20, 0.7, 0.2, 15.0, &mut rng);
        assert_eq!(offspring.len(), 20);
    }

    #[test]
    fn test_var_or_offspring_genes_within_bounds() {
        let specs = sewer_specs();
        let pop: Vec<Individual> = (0..10)
            .map(|i| {
                make_individual(vec![
                    (i as f64) * 0.5,
                    0.8,
                    1.1,
                    1.0,
                    70.0 + (i as f64) * 5.0,
                ])
            })
            .collect();
        let mut rng = root_rng(7);
        let offspring = var_or(&pop, specs, 30, 0.7, 0.2, 15.0, &mut rng);
        for (j, ind) in offspring.iter().enumerate() {
            for (i, spec) in specs.iter().enumerate() {
                assert!(
                    ind.genes[i] >= spec.lower_bound && ind.genes[i] <= spec.upper_bound,
                    "offspring[{j}].gene[{i}]={} out of [{}, {}]",
                    ind.genes[i],
                    spec.lower_bound,
                    spec.upper_bound
                );
            }
        }
    }

    #[test]
    fn test_var_or_deterministic_same_seed() {
        let specs = sewer_specs();
        let pop: Vec<Individual> = (0..10)
            .map(|_| make_individual(vec![2.0, 1.0, 1.2, 1.0, 80.0]))
            .collect();
        let run = |seed: u64| {
            let mut rng = root_rng(seed);
            let off = var_or(&pop, specs, 20, 0.7, 0.2, 15.0, &mut rng);
            off.iter().map(|ind| ind.genes.clone()).collect::<Vec<_>>()
        };
        assert_eq!(run(42), run(42));
        assert_ne!(run(42), run(43));
    }

    #[test]
    fn test_var_or_lambda_1() {
        let specs = sewer_specs();
        let pop = vec![make_individual(vec![2.0, 1.0, 1.2, 1.0, 80.0])];
        let mut rng = root_rng(0);
        let offspring = var_or(&pop, specs, 1, 0.7, 0.2, 15.0, &mut rng);
        assert_eq!(offspring.len(), 1);
    }

    #[test]
    fn test_var_or_pure_reproduction_cxpb_0_mutpb_0() {
        let specs = sewer_specs();
        let genes = vec![2.0, 1.0, 1.2, 1.0, 80.0];
        let pop = vec![make_individual(genes.clone())];
        let mut rng = root_rng(0);
        let offspring = var_or(&pop, specs, 5, 0.0, 0.0, 15.0, &mut rng);
        for ind in &offspring {
            assert_eq!(
                ind.genes, genes,
                "pure reproduction must produce exact clones"
            );
        }
    }

    // ── init_population ───────────────────────────────────────────────────────

    #[test]
    fn test_init_population_returns_correct_size() {
        let mut rng = root_rng(42);
        let pop = init_population(SolverType::Sewer, 50, &mut rng);
        assert_eq!(pop.len(), 50);
    }

    #[test]
    fn test_init_population_genes_within_bounds() {
        let specs = sewer_specs();
        let mut rng = root_rng(42);
        let pop = init_population(SolverType::Sewer, 100, &mut rng);
        for (j, ind) in pop.iter().enumerate() {
            assert_eq!(ind.genes.len(), specs.len());
            for (i, spec) in specs.iter().enumerate() {
                assert!(
                    ind.genes[i] >= spec.lower_bound && ind.genes[i] <= spec.upper_bound,
                    "ind[{j}].gene[{i}]={} out of [{}, {}]",
                    ind.genes[i],
                    spec.lower_bound,
                    spec.upper_bound
                );
            }
        }
    }

    #[test]
    fn test_init_population_fitness_is_none() {
        let mut rng = root_rng(1);
        let pop = init_population(SolverType::WaterSupply, 10, &mut rng);
        for ind in &pop {
            assert!(
                ind.fitness.is_none(),
                "freshly init'd individual must have no fitness"
            );
        }
    }

    #[test]
    fn test_init_population_deterministic() {
        let run = |seed: u64| {
            let mut rng = root_rng(seed);
            let pop = init_population(SolverType::Sewer, 20, &mut rng);
            pop.iter().map(|ind| ind.genes.clone()).collect::<Vec<_>>()
        };
        assert_eq!(run(7), run(7));
        assert_ne!(run(7), run(8));
    }

    #[test]
    fn test_init_population_all_solver_types() {
        let types = [
            SolverType::Sewer,
            SolverType::WaterSupply,
            SolverType::Conveyance,
            SolverType::Distribution,
            SolverType::PumpStation,
            SolverType::Intake,
        ];
        for st in types {
            let mut rng = root_rng(42);
            let pop = init_population(st, 5, &mut rng);
            assert_eq!(pop.len(), 5, "must return 5 for {st:?}");
        }
    }

    // ── GeneType integer-gene contract ────────────────────────────────────────

    #[test]
    fn test_sewer_has_integer_genes() {
        let specs = sewer_specs();
        let int_count = specs.iter().filter(|s| s.dtype == GeneType::Int).count();
        assert!(int_count >= 2, "sewer should have at least 2 integer genes");
    }

    #[test]
    fn test_sbx_bounds_respected_for_integer_gene_positions() {
        let specs = sewer_specs();
        let mut rng = root_rng(999);
        for _ in 0..50 {
            let mut p1 = make_individual(vec![0.0, 0.5, 1.0, 0.0, 60.0]);
            let mut p2 = make_individual(vec![9.0, 2.0, 1.5, 3.0, 120.0]);
            sbx_crossover(&mut p1, &mut p2, specs, 10.0, &mut rng);
            for (i, spec) in specs.iter().enumerate() {
                assert!(
                    p1.genes[i] >= spec.lower_bound && p1.genes[i] <= spec.upper_bound,
                    "int-gene[{i}]={} out of [{}, {}] after SBX",
                    p1.genes[i],
                    spec.lower_bound,
                    spec.upper_bound
                );
            }
        }
    }
}
