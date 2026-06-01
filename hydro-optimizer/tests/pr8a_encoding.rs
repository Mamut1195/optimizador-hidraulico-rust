//! PR-8a encoding tests (REQ-001 scenarios).
//!
//! RED → GREEN → REFACTOR per strict-tdd.md.
//! References production code in `hydro_optimizer::{encoding, errors}`.

use hydro_optimizer::encoding::{gene_specs, GeneType, Individual, IndividualEncoder, SolverType};
use hydro_optimizer::errors::OptimizationError;
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;

// ── GENE_SPECS table structure ────────────────────────────────────────────────

/// REQ-001: GENE_SPECS must cover all 6 solver types.
#[test]
fn test_gene_specs_table_completeness() {
    let specs = gene_specs();
    let expected = [
        "sewer",
        "water_supply",
        "conveyance",
        "distribution",
        "pump_station",
        "intake",
    ];
    for k in &expected {
        assert!(specs.contains_key(k), "GENE_SPECS missing key '{k}'");
    }
    assert_eq!(specs.len(), 6, "GENE_SPECS must have exactly 6 entries");
}

/// REQ-001: Sewer has 5 genes with correct bounds (from oracle encoding.py).
#[test]
fn test_gene_specs_sewer_bounds() {
    let all_specs = gene_specs();
    let specs = all_specs
        .get("sewer")
        .expect("sewer key must exist in GENE_SPECS");
    assert_eq!(specs.len(), 5, "sewer must have 5 genes");

    // route_variant: int [0, 9]
    assert_eq!(specs[0].name, "route_variant");
    assert_eq!(specs[0].lower_bound, 0.0);
    assert_eq!(specs[0].upper_bound, 9.0);
    assert_eq!(specs[0].dtype, GeneType::Int);

    // slope_factor: float [0.5, 2.0]
    assert_eq!(specs[1].name, "slope_factor");
    assert_eq!(specs[1].lower_bound, 0.5);
    assert_eq!(specs[1].upper_bound, 2.0);
    assert_eq!(specs[1].dtype, GeneType::Float);

    // cover_factor: float [1.0, 1.5]
    assert_eq!(specs[2].name, "cover_factor");
    assert_eq!(specs[2].lower_bound, 1.0);
    assert_eq!(specs[2].upper_bound, 1.5);
    assert_eq!(specs[2].dtype, GeneType::Float);

    // diameter_offset: int [0, 3]
    assert_eq!(specs[3].name, "diameter_offset");
    assert_eq!(specs[3].lower_bound, 0.0);
    assert_eq!(specs[3].upper_bound, 3.0);
    assert_eq!(specs[3].dtype, GeneType::Int);

    // manhole_spacing: float [60, 120]
    assert_eq!(specs[4].name, "manhole_spacing");
    assert_eq!(specs[4].lower_bound, 60.0);
    assert_eq!(specs[4].upper_bound, 120.0);
    assert_eq!(specs[4].dtype, GeneType::Float);
}

// ── random_individual ─────────────────────────────────────────────────────────

/// REQ-001 Scenario: Random individual is within bounds (sewer).
#[test]
fn test_random_individual_within_bounds_sewer() {
    let mut rng = ChaCha20Rng::seed_from_u64(42);
    let ind = IndividualEncoder::random_individual(SolverType::Sewer, &mut rng);
    let all_specs = gene_specs();
    let specs = all_specs.get("sewer").unwrap();
    assert_eq!(ind.genes.len(), specs.len());
    for (gene_val, spec) in ind.genes.iter().zip(specs.iter()) {
        assert!(
            *gene_val >= spec.lower_bound && *gene_val <= spec.upper_bound,
            "gene '{}' value {} out of bounds [{}, {}]",
            spec.name,
            gene_val,
            spec.lower_bound,
            spec.upper_bound
        );
    }
}

/// REQ-001: random_individual within bounds for all 6 solver types.
#[test]
fn test_random_individual_within_bounds_all_types() {
    let types = [
        SolverType::Sewer,
        SolverType::WaterSupply,
        SolverType::Conveyance,
        SolverType::Distribution,
        SolverType::PumpStation,
        SolverType::Intake,
    ];
    let all_specs = gene_specs();
    for solver_type in types {
        let mut rng = ChaCha20Rng::seed_from_u64(42);
        let key: &str = solver_type.into();
        let ind = IndividualEncoder::random_individual(solver_type, &mut rng);
        let specs = all_specs.get(key).unwrap();
        assert_eq!(
            ind.genes.len(),
            specs.len(),
            "wrong gene count for {solver_type:?}"
        );
        for (gene_val, spec) in ind.genes.iter().zip(specs.iter()) {
            assert!(
                *gene_val >= spec.lower_bound && *gene_val <= spec.upper_bound,
                "gene '{}' in {solver_type:?}: value {} out of [{}, {}]",
                spec.name,
                gene_val,
                spec.lower_bound,
                spec.upper_bound
            );
        }
    }
}

// ── decode ───────────────────────────────────────────────────────────────────

/// REQ-001 Scenario: Decode integer gene rounds correctly (value 2.7 for int [0,3] → 3).
#[test]
fn test_decode_integer_gene_rounds() {
    // diameter_offset is genes[3] for sewer (int, [0,3])
    let ind = Individual {
        genes: vec![0.0, 1.0, 1.0, 2.7, 60.0],
        fitness: None,
    };
    let decoded = IndividualEncoder::decode(&ind, SolverType::Sewer).unwrap();
    let diameter_offset = decoded.get("diameter_offset").unwrap();
    assert_eq!(
        *diameter_offset, 3.0,
        "2.7 rounded to nearest int should be 3"
    );
}

/// REQ-001: Decode float gene clamps to upper bound.
#[test]
fn test_decode_float_gene_clamps_upper() {
    // slope_factor is genes[1] for sewer (float, [0.5, 2.0])
    // value 2.1 should be clamped to 2.0
    let ind = Individual {
        genes: vec![0.0, 2.1, 1.0, 0.0, 60.0],
        fitness: None,
    };
    let decoded = IndividualEncoder::decode(&ind, SolverType::Sewer).unwrap();
    let slope_factor = decoded.get("slope_factor").unwrap();
    assert!(
        (*slope_factor - 2.0).abs() < 1e-12,
        "slope_factor 2.1 should be clamped to 2.0, got {slope_factor}"
    );
}

/// REQ-001 Scenario: Decode rejects wrong-length chromosome.
#[test]
fn test_decode_rejects_wrong_length() {
    // Sewer expects 5 genes; provide 3
    let ind = Individual {
        genes: vec![0.0, 1.0, 1.0],
        fitness: None,
    };
    let result = IndividualEncoder::decode(&ind, SolverType::Sewer);
    match result {
        Err(OptimizationError::InvalidConfig(msg)) => {
            assert!(
                msg.contains("chromosome length mismatch"),
                "error message should mention 'chromosome length mismatch', got: {msg}"
            );
        }
        other => panic!("expected Err(InvalidConfig), got {other:?}"),
    }
}

/// REQ-001: encode→decode round-trip for sewer — all decoded values within bounds, dtypes correct.
#[test]
fn test_encode_decode_roundtrip_sewer() {
    let mut rng = ChaCha20Rng::seed_from_u64(99);
    let ind = IndividualEncoder::random_individual(SolverType::Sewer, &mut rng);
    let decoded = IndividualEncoder::decode(&ind, SolverType::Sewer).unwrap();

    let all_specs = gene_specs();
    let specs = all_specs.get("sewer").unwrap();
    for spec in specs {
        let val = decoded
            .get(spec.name)
            .expect("decoded must contain spec name");
        assert!(
            *val >= spec.lower_bound && *val <= spec.upper_bound,
            "decoded '{}' = {} out of bounds",
            spec.name,
            val
        );
        if spec.dtype == GeneType::Int {
            // Integer genes must be exact integers after decode
            assert_eq!(
                *val,
                val.round(),
                "integer gene '{}' = {} is not an integer",
                spec.name,
                val
            );
        }
    }
}
