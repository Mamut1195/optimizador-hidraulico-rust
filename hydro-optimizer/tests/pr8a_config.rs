//! PR-8a config tests (REQ-002, REQ-013 scenarios).
//!
//! RED → GREEN → REFACTOR per strict-tdd.md.

use hydro_optimizer::config::OptimizationConfig;
use hydro_optimizer::errors::OptimizationError;
use hydro_optimizer::rng::{child_rng, root_rng};

// ── OptimizationConfig ────────────────────────────────────────────────────────

/// REQ-002 Scenario: Default config is valid — fields match oracle defaults.
#[test]
fn test_config_default_is_valid() {
    let cfg = OptimizationConfig::default();
    assert_eq!(cfg.population_size, 100);
    assert_eq!(cfg.generations, 50);
    assert!((cfg.crossover_prob - 0.9).abs() < 1e-12);
    assert!((cfg.mutation_prob - 0.1).abs() < 1e-12);
    assert!((cfg.max_time_seconds - 300.0).abs() < 1e-12);
    assert_eq!(cfg.seed, 42);
}

/// REQ-002: Serde round-trip of OptimizationConfig.
#[test]
fn test_config_serde_roundtrip() {
    let cfg = OptimizationConfig::default();
    let json = serde_json::to_string(&cfg).expect("serialize must succeed");
    let back: OptimizationConfig = serde_json::from_str(&json).expect("deserialize must succeed");
    assert_eq!(back.population_size, cfg.population_size);
    assert_eq!(back.generations, cfg.generations);
    assert_eq!(back.seed, cfg.seed);
    assert!((back.crossover_prob - cfg.crossover_prob).abs() < 1e-12);
    assert!((back.mutation_prob - cfg.mutation_prob).abs() < 1e-12);
}

/// REQ-002 Scenario: Invalid population size is rejected by validator.
#[test]
fn test_config_invalid_population_rejected() {
    let cfg = OptimizationConfig {
        population_size: 0,
        ..Default::default()
    };
    let result = cfg.validate();
    match result {
        Err(OptimizationError::InvalidConfig(_)) => {} // expected
        other => panic!("expected Err(InvalidConfig) for population_size=0, got {other:?}"),
    }
}

// ── RNG helpers ───────────────────────────────────────────────────────────────

/// Design §5: root_rng with same seed produces same sequence.
#[test]
fn test_rng_root_produces_reproducible_sequence() {
    use rand::RngCore;
    let mut rng1 = root_rng(42);
    let mut rng2 = root_rng(42);
    let v1 = rng1.next_u64();
    let v2 = rng2.next_u64();
    assert_eq!(v1, v2, "same seed must produce same first value");
    // Second value also must match
    assert_eq!(rng1.next_u64(), rng2.next_u64());
}

/// Design §5: child_rng with different indices diverges.
#[test]
fn test_child_rng_diverges_for_different_indices() {
    use rand::RngCore;
    let mut rng0 = child_rng(42, 0, 0);
    let mut rng1 = child_rng(42, 0, 1);
    let v0 = rng0.next_u64();
    let v1 = rng1.next_u64();
    assert_ne!(v0, v1, "child RNGs with different indices must diverge");
}

// ── OptimizationError display ─────────────────────────────────────────────────

/// REQ-013: Each OptimizationError variant's Display contains expected substring.
#[test]
fn test_error_display_messages() {
    let e = OptimizationError::InvalidConfig("bad pop size".to_owned());
    let msg = format!("{e}");
    assert!(
        msg.contains("config") || msg.contains("invalid"),
        "InvalidConfig display should mention 'config' or 'invalid', got: {msg}"
    );

    let e2 = OptimizationError::AllInfeasible;
    let msg2 = format!("{e2}");
    assert!(
        msg2.contains("feasible") || msg2.contains("infeasible"),
        "AllInfeasible display should mention feasibility, got: {msg2}"
    );

    let e3 = OptimizationError::EvaluatorFailure("solver error".to_owned());
    let msg3 = format!("{e3}");
    assert!(
        msg3.contains("solver") || msg3.contains("evaluat"),
        "EvaluatorFailure display should mention context, got: {msg3}"
    );

    let e4 = OptimizationError::NormValidationFailure("profile not found".to_owned());
    let msg4 = format!("{e4}");
    assert!(
        msg4.contains("norm") || msg4.contains("valid"),
        "NormValidationFailure display should mention norm/valid, got: {msg4}"
    );
}
