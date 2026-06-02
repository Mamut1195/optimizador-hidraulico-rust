//! Genetic operators: SBX crossover, polynomial mutation, varOr, adaptive eta.
//!
//! Ports DEAP `tools.cxSimulatedBinaryBounded`, `tools.mutPolynomialBounded`,
//! and `algorithms.varOr` faithfully, including the adaptive-eta linear schedule.
//!
//! Design §5 / REQ-008, REQ-014, REQ-015:
//! - All functions take `&mut ChaCha20Rng` — no thread-local RNGs.
//! - Integer genes (`GeneType::Int`) pass through SBX/mutation unchanged and are
//!   rounded + clamped in the decode step (matches DEAP behaviour where int genes
//!   still receive float chromosomes, then `encoder.decode()` casts them).
//! - All functions are `pub(crate)` (REQ-014).

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use rand::SeedableRng;
    use rand_chacha::ChaCha20Rng;

    use super::*;
    use crate::encoding::{gene_specs, GeneType, Individual, SolverType};
    use crate::rng::root_rng;

    fn sewer_specs() -> &'static [crate::encoding::GeneSpec] {
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
        assert!(
            (result - 5.0).abs() < 1e-12,
            "expected 5.0, got {result}"
        );
    }

    #[test]
    fn test_adaptive_eta_at_gen_max_equals_eta_max() {
        let result = adaptive_eta_value(100, 100, 5.0, 50.0);
        assert!(
            (result - 50.0).abs() < 1e-12,
            "expected 50.0, got {result}"
        );
    }

    #[test]
    fn test_adaptive_eta_at_midpoint_equals_midpoint_value() {
        let result = adaptive_eta_value(50, 100, 5.0, 50.0);
        assert!(
            (result - 27.5).abs() < 1e-12,
            "expected 27.5, got {result}"
        );
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
                p1.genes[i], spec.lower_bound, spec.upper_bound
            );
            assert!(
                p2.genes[i] >= spec.lower_bound && p2.genes[i] <= spec.upper_bound,
                "child2 gene[{i}]={} out of [{}, {}]",
                p2.genes[i], spec.lower_bound, spec.upper_bound
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
                    ind.genes[i], spec.lower_bound, spec.upper_bound
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
            .map(|i| make_individual(vec![i as f64 * 0.5, 0.8, 1.1, 1.0, 70.0 + i as f64 * 5.0]))
            .collect();
        let mut rng = root_rng(7);
        let offspring = var_or(&pop, specs, 30, 0.7, 0.2, 15.0, &mut rng);
        for (j, ind) in offspring.iter().enumerate() {
            for (i, spec) in specs.iter().enumerate() {
                assert!(
                    ind.genes[i] >= spec.lower_bound && ind.genes[i] <= spec.upper_bound,
                    "offspring[{j}].gene[{i}]={} out of bounds"
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
            assert_eq!(ind.genes, genes, "pure reproduction must produce exact clones");
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
                    ind.genes[i], spec.lower_bound, spec.upper_bound
                );
            }
        }
    }

    #[test]
    fn test_init_population_fitness_is_none() {
        let mut rng = root_rng(1);
        let pop = init_population(SolverType::WaterSupply, 10, &mut rng);
        for ind in &pop {
            assert!(ind.fitness.is_none(), "freshly init'd individual must have no fitness");
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
                    "int-gene[{i}] out of bounds after SBX"
                );
            }
        }
    }
}
