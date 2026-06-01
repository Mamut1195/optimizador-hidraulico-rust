//! PR-8b verify-fix tests — addresses all WARNINGs from PR-8a verify report.
//!
//! RED → GREEN → REFACTOR per strict-tdd.md.
//!
//! Covers:
//! - WARNING-1: decode round-trip for all 6 solver types (not just sewer)
//! - WARNING-2: validate() rejection-path coverage (7+ paths)
//! - REQ-013 gap: Display tests for Solver, TimeBudgetExceeded, Internal variants
//! - SUGGESTION-1: integer-gene clamp test

use hydro_optimizer::{
    gene_specs, GeneType, Individual, IndividualEncoder, OptimizationConfig, OptimizationError,
    SolverType,
};
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;

// ── WARNING-1: decode round-trip for all 6 solver types ──────────────────────

/// REQ-001: encode→decode round-trip for water_supply.
#[test]
fn test_encode_decode_roundtrip_water_supply() {
    let mut rng = ChaCha20Rng::seed_from_u64(1);
    let ind = IndividualEncoder::random_individual(SolverType::WaterSupply, &mut rng);
    let decoded = IndividualEncoder::decode(&ind, SolverType::WaterSupply).unwrap();
    let all_specs = gene_specs();
    let specs = all_specs.get("water_supply").unwrap();
    assert_eq!(decoded.len(), specs.len());
    for spec in specs {
        let val = decoded.get(spec.name).expect("decoded must contain gene");
        assert!(
            *val >= spec.lower_bound && *val <= spec.upper_bound,
            "water_supply '{}' = {} out of bounds [{}, {}]",
            spec.name,
            val,
            spec.lower_bound,
            spec.upper_bound
        );
        if spec.dtype == GeneType::Int {
            assert_eq!(
                *val,
                val.round(),
                "water_supply integer gene '{}' = {} not integer",
                spec.name,
                val
            );
        }
    }
}

/// REQ-001: encode→decode round-trip for conveyance.
#[test]
fn test_encode_decode_roundtrip_conveyance() {
    let mut rng = ChaCha20Rng::seed_from_u64(2);
    let ind = IndividualEncoder::random_individual(SolverType::Conveyance, &mut rng);
    let decoded = IndividualEncoder::decode(&ind, SolverType::Conveyance).unwrap();
    let all_specs = gene_specs();
    let specs = all_specs.get("conveyance").unwrap();
    assert_eq!(decoded.len(), specs.len());
    for spec in specs {
        let val = decoded.get(spec.name).expect("decoded must contain gene");
        assert!(
            *val >= spec.lower_bound && *val <= spec.upper_bound,
            "conveyance '{}' = {} out of bounds [{}, {}]",
            spec.name,
            val,
            spec.lower_bound,
            spec.upper_bound
        );
        if spec.dtype == GeneType::Int {
            assert_eq!(
                *val,
                val.round(),
                "conveyance integer gene '{}' not integer",
                spec.name
            );
        }
    }
}

/// REQ-001: encode→decode round-trip for distribution.
#[test]
fn test_encode_decode_roundtrip_distribution() {
    let mut rng = ChaCha20Rng::seed_from_u64(3);
    let ind = IndividualEncoder::random_individual(SolverType::Distribution, &mut rng);
    let decoded = IndividualEncoder::decode(&ind, SolverType::Distribution).unwrap();
    let all_specs = gene_specs();
    let specs = all_specs.get("distribution").unwrap();
    assert_eq!(decoded.len(), specs.len());
    for spec in specs {
        let val = decoded.get(spec.name).expect("decoded must contain gene");
        assert!(
            *val >= spec.lower_bound && *val <= spec.upper_bound,
            "distribution '{}' = {} out of bounds [{}, {}]",
            spec.name,
            val,
            spec.lower_bound,
            spec.upper_bound
        );
        if spec.dtype == GeneType::Int {
            assert_eq!(
                *val,
                val.round(),
                "distribution integer gene '{}' not integer",
                spec.name
            );
        }
    }
}

/// REQ-001: encode→decode round-trip for pump_station.
#[test]
fn test_encode_decode_roundtrip_pump_station() {
    let mut rng = ChaCha20Rng::seed_from_u64(4);
    let ind = IndividualEncoder::random_individual(SolverType::PumpStation, &mut rng);
    let decoded = IndividualEncoder::decode(&ind, SolverType::PumpStation).unwrap();
    let all_specs = gene_specs();
    let specs = all_specs.get("pump_station").unwrap();
    assert_eq!(decoded.len(), specs.len());
    for spec in specs {
        let val = decoded.get(spec.name).expect("decoded must contain gene");
        assert!(
            *val >= spec.lower_bound && *val <= spec.upper_bound,
            "pump_station '{}' = {} out of bounds [{}, {}]",
            spec.name,
            val,
            spec.lower_bound,
            spec.upper_bound
        );
        if spec.dtype == GeneType::Int {
            assert_eq!(
                *val,
                val.round(),
                "pump_station integer gene '{}' not integer",
                spec.name
            );
        }
    }
}

/// REQ-001: encode→decode round-trip for intake.
#[test]
fn test_encode_decode_roundtrip_intake() {
    let mut rng = ChaCha20Rng::seed_from_u64(5);
    let ind = IndividualEncoder::random_individual(SolverType::Intake, &mut rng);
    let decoded = IndividualEncoder::decode(&ind, SolverType::Intake).unwrap();
    let all_specs = gene_specs();
    let specs = all_specs.get("intake").unwrap();
    assert_eq!(decoded.len(), specs.len());
    for spec in specs {
        let val = decoded.get(spec.name).expect("decoded must contain gene");
        assert!(
            *val >= spec.lower_bound && *val <= spec.upper_bound,
            "intake '{}' = {} out of bounds [{}, {}]",
            spec.name,
            val,
            spec.lower_bound,
            spec.upper_bound
        );
        if spec.dtype == GeneType::Int {
            assert_eq!(
                *val,
                val.round(),
                "intake integer gene '{}' not integer",
                spec.name
            );
        }
    }
}

// ── WARNING-2: validate() rejection paths ────────────────────────────────────

/// REQ-002: validate() rejects generations=0.
#[test]
fn test_config_rejects_generations_zero() {
    let cfg = OptimizationConfig {
        generations: 0,
        ..Default::default()
    };
    match cfg.validate() {
        Err(OptimizationError::InvalidConfig(msg)) => {
            assert!(
                msg.contains("generations"),
                "error should mention 'generations', got: {msg}"
            );
        }
        other => panic!("expected Err(InvalidConfig), got {other:?}"),
    }
}

/// REQ-002: validate() rejects crossover_prob < 0.
#[test]
fn test_config_rejects_crossover_below_zero() {
    let cfg = OptimizationConfig {
        crossover_prob: -0.1,
        ..Default::default()
    };
    match cfg.validate() {
        Err(OptimizationError::InvalidConfig(msg)) => {
            assert!(
                msg.contains("crossover"),
                "error should mention 'crossover', got: {msg}"
            );
        }
        other => panic!("expected Err(InvalidConfig), got {other:?}"),
    }
}

/// REQ-002: validate() rejects crossover_prob > 1.
#[test]
fn test_config_rejects_crossover_above_one() {
    let cfg = OptimizationConfig {
        crossover_prob: 1.5,
        ..Default::default()
    };
    match cfg.validate() {
        Err(OptimizationError::InvalidConfig(msg)) => {
            assert!(
                msg.contains("crossover"),
                "error should mention 'crossover', got: {msg}"
            );
        }
        other => panic!("expected Err(InvalidConfig), got {other:?}"),
    }
}

/// REQ-002: validate() rejects mutation_prob < 0.
#[test]
fn test_config_rejects_mutation_below_zero() {
    let cfg = OptimizationConfig {
        mutation_prob: -0.05,
        ..Default::default()
    };
    match cfg.validate() {
        Err(OptimizationError::InvalidConfig(msg)) => {
            assert!(
                msg.contains("mutation"),
                "error should mention 'mutation', got: {msg}"
            );
        }
        other => panic!("expected Err(InvalidConfig), got {other:?}"),
    }
}

/// REQ-002: validate() rejects mutation_prob > 1.
#[test]
fn test_config_rejects_mutation_above_one() {
    let cfg = OptimizationConfig {
        mutation_prob: 1.1,
        ..Default::default()
    };
    match cfg.validate() {
        Err(OptimizationError::InvalidConfig(msg)) => {
            assert!(
                msg.contains("mutation"),
                "error should mention 'mutation', got: {msg}"
            );
        }
        other => panic!("expected Err(InvalidConfig), got {other:?}"),
    }
}

/// REQ-002: validate() rejects max_time_seconds <= 0.
#[test]
fn test_config_rejects_max_time_nonpositive() {
    let cfg = OptimizationConfig {
        max_time_seconds: 0.0,
        ..Default::default()
    };
    match cfg.validate() {
        Err(OptimizationError::InvalidConfig(msg)) => {
            assert!(
                msg.contains("max_time"),
                "error should mention 'max_time', got: {msg}"
            );
        }
        other => panic!("expected Err(InvalidConfig), got {other:?}"),
    }
}

/// REQ-002: validate() rejects eta_min <= 0.
#[test]
fn test_config_rejects_eta_min_nonpositive() {
    let cfg = OptimizationConfig {
        eta_min: 0.0,
        ..Default::default()
    };
    match cfg.validate() {
        Err(OptimizationError::InvalidConfig(msg)) => {
            assert!(
                msg.contains("eta"),
                "error should mention 'eta', got: {msg}"
            );
        }
        other => panic!("expected Err(InvalidConfig), got {other:?}"),
    }
}

/// REQ-002: validate() rejects eta_min > eta_max.
#[test]
fn test_config_rejects_eta_min_greater_than_eta_max() {
    let cfg = OptimizationConfig {
        eta_min: 50.0,
        eta_max: 5.0,
        ..Default::default()
    };
    match cfg.validate() {
        Err(OptimizationError::InvalidConfig(msg)) => {
            assert!(
                msg.contains("eta"),
                "error should mention 'eta', got: {msg}"
            );
        }
        other => panic!("expected Err(InvalidConfig), got {other:?}"),
    }
}

/// REQ-002: validate() rejects negative weight.
#[test]
fn test_config_rejects_negative_weight() {
    let cfg = OptimizationConfig {
        weight_cost: -1.0,
        ..Default::default()
    };
    match cfg.validate() {
        Err(OptimizationError::InvalidConfig(msg)) => {
            assert!(
                msg.contains("weight"),
                "error should mention 'weight', got: {msg}"
            );
        }
        other => panic!("expected Err(InvalidConfig), got {other:?}"),
    }
}

// ── REQ-013: Display tests for untested error variants ────────────────────────

/// REQ-013: Solver variant Display contains expected context.
///
/// Note: Solver(#[from] SolverError) cannot be constructed directly here without
/// a SolverError instance; we verify via the thiserror-generated format string
/// indirectly by checking variant constructability and Display on the closest
/// available path. Since hydro_solvers::SolverError is accessible via the
/// hydro_solvers crate, we import it here.
#[test]
fn test_error_solver_variant_display() {
    use hydro_solvers::SolverError;
    let inner = SolverError::BuildFailed("test".to_owned());
    let e = OptimizationError::from(inner);
    let msg = format!("{e}");
    assert!(
        msg.contains("solver") || msg.contains("error"),
        "Solver variant display should mention 'solver' or 'error', got: {msg}"
    );
}

/// REQ-013: TimeBudgetExceeded variant Display contains expected context.
#[test]
fn test_error_time_budget_exceeded_display() {
    let e = OptimizationError::TimeBudgetExceeded(12.5);
    let msg = format!("{e}");
    assert!(
        msg.contains("budget") || msg.contains("12.5") || msg.contains("wall"),
        "TimeBudgetExceeded display should contain budget/time info, got: {msg}"
    );
}

/// REQ-013: Internal variant Display contains expected context.
#[test]
fn test_error_internal_variant_display() {
    let e = OptimizationError::Internal("niche count overflow".to_owned());
    let msg = format!("{e}");
    assert!(
        msg.contains("internal") || msg.contains("invariant"),
        "Internal display should mention 'internal' or 'invariant', got: {msg}"
    );
}

// ── SUGGESTION-1: integer-gene clamp tests ────────────────────────────────────

/// REQ-001: negative route_variant value clamps to lower_bound then rounds to 0.
///
/// Oracle semantics: clamp(-0.5, 0, 9) → 0.0 → round → 0
#[test]
fn test_decode_integer_gene_clamps_below_lower_bound() {
    // route_variant is genes[0] for sewer (int, [0, 9])
    // value -0.5 should clamp to 0.0 then remain 0 (already integer)
    let ind = Individual {
        genes: vec![-0.5, 1.0, 1.0, 0.0, 60.0],
        fitness: None,
    };
    let decoded = IndividualEncoder::decode(&ind, SolverType::Sewer).unwrap();
    let route_variant = decoded.get("route_variant").unwrap();
    assert_eq!(
        *route_variant, 0.0,
        "route_variant -0.5 should clamp to 0.0, got {route_variant}"
    );
}

/// REQ-001: value 9.7 for route_variant (int [0,9]) clamps to 9, rounds to 9.
#[test]
fn test_decode_integer_gene_clamps_above_upper_bound() {
    // value 9.7 should clamp to 9.0 (upper bound) then round to 9
    let ind = Individual {
        genes: vec![9.7, 1.0, 1.0, 0.0, 60.0],
        fitness: None,
    };
    let decoded = IndividualEncoder::decode(&ind, SolverType::Sewer).unwrap();
    let route_variant = decoded.get("route_variant").unwrap();
    assert_eq!(
        *route_variant, 9.0,
        "route_variant 9.7 should clamp to 9.0, got {route_variant}"
    );
}
