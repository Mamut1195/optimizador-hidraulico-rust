//! Gene encoding / decoding for the NSGA-III optimizer.
//!
//! Ports Python `hydro_engine/optimization/encoding.py` faithfully:
//! - `GeneSpec` — per-gene metadata (name, bounds, dtype).
//! - `GENE_SPECS` — static `BTreeMap` covering all 6 solver types.
//! - `SolverType` — enum for the 6 project types.
//! - `Individual` — raw float chromosome + optional cached fitness.
//! - `IndividualEncoder` — stateless encode/decode helpers.
//!
//! Design §12 determinism rule: uses `BTreeMap` (not `HashMap`).

use std::collections::BTreeMap;
use std::sync::OnceLock;

use rand::Rng;
use rand_chacha::ChaCha20Rng;

use crate::errors::OptimizationError;

// ── GeneType ──────────────────────────────────────────────────────────────────

/// Dtype of a gene: continuous float or discrete integer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeneType {
    Float,
    Int,
}

// ── GeneSpec ──────────────────────────────────────────────────────────────────

/// Specification of a single gene in the chromosome.
///
/// Mirrors `GeneSpec` dataclass in Python oracle `encoding.py`.
#[derive(Debug, Clone)]
pub struct GeneSpec {
    /// Human-readable parameter name (matches Python oracle).
    pub name: &'static str,
    /// Lower bound (inclusive).
    pub lower_bound: f64,
    /// Upper bound (inclusive).
    pub upper_bound: f64,
    /// Discrete integer or continuous float.
    pub dtype: GeneType,
}

// ── SolverType ────────────────────────────────────────────────────────────────

/// The six hydraulic project types — mirrors Python `GENE_SPECS` keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SolverType {
    Sewer,
    WaterSupply,
    Conveyance,
    Distribution,
    PumpStation,
    Intake,
}

impl From<SolverType> for &'static str {
    fn from(s: SolverType) -> &'static str {
        match s {
            SolverType::Sewer => "sewer",
            SolverType::WaterSupply => "water_supply",
            SolverType::Conveyance => "conveyance",
            SolverType::Distribution => "distribution",
            SolverType::PumpStation => "pump_station",
            SolverType::Intake => "intake",
        }
    }
}

impl std::fmt::Display for SolverType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s: &'static str = (*self).into();
        f.write_str(s)
    }
}

impl TryFrom<&str> for SolverType {
    type Error = OptimizationError;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        match s {
            "sewer" => Ok(SolverType::Sewer),
            "water_supply" => Ok(SolverType::WaterSupply),
            "conveyance" => Ok(SolverType::Conveyance),
            "distribution" => Ok(SolverType::Distribution),
            "pump_station" => Ok(SolverType::PumpStation),
            "intake" => Ok(SolverType::Intake),
            other => Err(OptimizationError::InvalidConfig(format!(
                "unknown solver type '{other}'; valid: sewer, water_supply, conveyance, distribution, pump_station, intake"
            ))),
        }
    }
}

// ── GENE_SPECS ────────────────────────────────────────────────────────────────

/// Static gene-specification registry for all 6 solver types.
///
/// Uses `BTreeMap` for deterministic iteration order (design §12).
/// Values are `Vec<GeneSpec>` matching the oracle `encoding.py` exactly.
pub static GENE_SPECS: OnceLock<BTreeMap<&'static str, Vec<GeneSpec>>> = OnceLock::new();

/// Initialise or return the global gene-spec table.
///
/// Called lazily on first access. Values are copied from the Python oracle.
pub fn gene_specs() -> &'static BTreeMap<&'static str, Vec<GeneSpec>> {
    GENE_SPECS.get_or_init(|| {
        let mut m = BTreeMap::new();

        // sewer — 5 genes
        m.insert(
            "sewer",
            vec![
                GeneSpec {
                    name: "route_variant",
                    lower_bound: 0.0,
                    upper_bound: 9.0,
                    dtype: GeneType::Int,
                },
                GeneSpec {
                    name: "slope_factor",
                    lower_bound: 0.5,
                    upper_bound: 2.0,
                    dtype: GeneType::Float,
                },
                GeneSpec {
                    name: "cover_factor",
                    lower_bound: 1.0,
                    upper_bound: 1.5,
                    dtype: GeneType::Float,
                },
                GeneSpec {
                    name: "diameter_offset",
                    lower_bound: 0.0,
                    upper_bound: 3.0,
                    dtype: GeneType::Int,
                },
                GeneSpec {
                    name: "manhole_spacing",
                    lower_bound: 60.0,
                    upper_bound: 120.0,
                    dtype: GeneType::Float,
                },
            ],
        );

        // water_supply — 5 genes
        m.insert(
            "water_supply",
            vec![
                GeneSpec {
                    name: "network_type",
                    lower_bound: 0.0,
                    upper_bound: 2.0,
                    dtype: GeneType::Int,
                },
                GeneSpec {
                    name: "diameter_offset",
                    lower_bound: 0.0,
                    upper_bound: 3.0,
                    dtype: GeneType::Int,
                },
                GeneSpec {
                    name: "source_head",
                    lower_bound: 20.0,
                    upper_bound: 60.0,
                    dtype: GeneType::Float,
                },
                GeneSpec {
                    name: "loop_density",
                    lower_bound: 0.0,
                    upper_bound: 1.0,
                    dtype: GeneType::Float,
                },
                GeneSpec {
                    name: "valve_spacing",
                    lower_bound: 100.0,
                    upper_bound: 500.0,
                    dtype: GeneType::Float,
                },
            ],
        );

        // conveyance — 4 genes
        m.insert(
            "conveyance",
            vec![
                GeneSpec {
                    name: "route_variant",
                    lower_bound: 0.0,
                    upper_bound: 9.0,
                    dtype: GeneType::Int,
                },
                GeneSpec {
                    name: "diameter_idx",
                    lower_bound: 0.0,
                    upper_bound: 8.0,
                    dtype: GeneType::Int,
                },
                GeneSpec {
                    name: "cover_factor",
                    lower_bound: 1.0,
                    upper_bound: 2.0,
                    dtype: GeneType::Float,
                },
                GeneSpec {
                    name: "valve_spacing",
                    lower_bound: 200.0,
                    upper_bound: 1000.0,
                    dtype: GeneType::Float,
                },
            ],
        );

        // distribution — 5 genes
        m.insert(
            "distribution",
            vec![
                GeneSpec {
                    name: "mesh_density",
                    lower_bound: 0.3,
                    upper_bound: 1.0,
                    dtype: GeneType::Float,
                },
                GeneSpec {
                    name: "diameter_offset",
                    lower_bound: 0.0,
                    upper_bound: 3.0,
                    dtype: GeneType::Int,
                },
                GeneSpec {
                    name: "source_head",
                    lower_bound: 20.0,
                    upper_bound: 60.0,
                    dtype: GeneType::Float,
                },
                GeneSpec {
                    name: "valve_spacing",
                    lower_bound: 100.0,
                    upper_bound: 500.0,
                    dtype: GeneType::Float,
                },
                GeneSpec {
                    name: "hydrant_spacing",
                    lower_bound: 100.0,
                    upper_bound: 300.0,
                    dtype: GeneType::Float,
                },
            ],
        );

        // pump_station — 4 genes
        m.insert(
            "pump_station",
            vec![
                GeneSpec {
                    name: "num_pumps",
                    lower_bound: 2.0,
                    upper_bound: 6.0,
                    dtype: GeneType::Int,
                },
                GeneSpec {
                    name: "suction_d_idx",
                    lower_bound: 0.0,
                    upper_bound: 5.0,
                    dtype: GeneType::Int,
                },
                GeneSpec {
                    name: "discharge_d_idx",
                    lower_bound: 0.0,
                    upper_bound: 5.0,
                    dtype: GeneType::Int,
                },
                GeneSpec {
                    name: "wet_well_factor",
                    lower_bound: 1.0,
                    upper_bound: 2.0,
                    dtype: GeneType::Float,
                },
            ],
        );

        // intake — 4 genes
        m.insert(
            "intake",
            vec![
                GeneSpec {
                    name: "channel_width_factor",
                    lower_bound: 1.0,
                    upper_bound: 2.0,
                    dtype: GeneType::Float,
                },
                GeneSpec {
                    name: "channel_slope_factor",
                    lower_bound: 0.5,
                    upper_bound: 2.0,
                    dtype: GeneType::Float,
                },
                GeneSpec {
                    name: "weir_type",
                    lower_bound: 0.0,
                    upper_bound: 1.0,
                    dtype: GeneType::Int,
                },
                GeneSpec {
                    name: "screen_velocity_factor",
                    lower_bound: 0.8,
                    upper_bound: 1.5,
                    dtype: GeneType::Float,
                },
            ],
        );

        m
    })
}

// ── Individual ────────────────────────────────────────────────────────────────

/// A single GA individual: raw float chromosome + optional cached fitness.
///
/// `fitness = None` means this individual has not been evaluated yet
/// (or was invalidated by crossover/mutation per DEAP varOr semantics).
#[derive(Debug, Clone)]
pub struct Individual {
    /// Raw float gene vector; length == `GENE_SPECS[solver_type].len()`.
    pub genes: Vec<f64>,
    /// Cached 5-objective fitness; `None` if not yet evaluated.
    pub fitness: Option<[f64; 5]>,
}

// ── IndividualEncoder ─────────────────────────────────────────────────────────

/// Stateless encoder/decoder: maps between raw float chromosomes and named
/// parameter maps. Mirrors `IndividualEncoder` in Python oracle `encoding.py`.
pub struct IndividualEncoder;

impl IndividualEncoder {
    /// Generate a random individual with genes uniformly sampled within bounds.
    ///
    /// Mirrors `IndividualEncoder.random_individual()` in the Python oracle.
    pub fn random_individual(solver_type: SolverType, rng: &mut ChaCha20Rng) -> Individual {
        let key: &str = solver_type.into();
        let specs = gene_specs()
            .get(key)
            .expect("SolverType must be present in GENE_SPECS");
        let genes = specs
            .iter()
            .map(|s| rng.gen_range(s.lower_bound..=s.upper_bound))
            .collect();
        Individual {
            genes,
            fitness: None,
        }
    }

    /// Decode a raw chromosome into a named-parameter map.
    ///
    /// Values are clamped to bounds. Integer genes are cast via `round()`.
    /// Returns `Err(InvalidConfig("chromosome length mismatch"))` if the
    /// chromosome length does not match the solver's gene count.
    ///
    /// Mirrors `IndividualEncoder.decode()` in the Python oracle.
    pub fn decode(
        individual: &Individual,
        solver_type: SolverType,
    ) -> Result<BTreeMap<&'static str, f64>, OptimizationError> {
        let key: &str = solver_type.into();
        let specs = gene_specs()
            .get(key)
            .expect("SolverType must be present in GENE_SPECS");

        if individual.genes.len() != specs.len() {
            return Err(OptimizationError::InvalidConfig(format!(
                "chromosome length mismatch: expected {} genes for {solver_type}, got {}",
                specs.len(),
                individual.genes.len()
            )));
        }

        let mut params = BTreeMap::new();
        for (gene_val, spec) in individual.genes.iter().zip(specs.iter()) {
            let clamped = gene_val.clamp(spec.lower_bound, spec.upper_bound);
            let decoded = if spec.dtype == GeneType::Int {
                clamped.round()
            } else {
                clamped
            };
            params.insert(spec.name, decoded);
        }
        Ok(params)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests (REQ-001: gene specs, random_individual, decode)
// Previously in tests/pr8a_encoding.rs and tests/pr8b_verify_fixes.rs.
// Moved inline (REQ-014: no pub re-exports for internal types).
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    fn make_rng(seed: u64) -> ChaCha20Rng {
        ChaCha20Rng::seed_from_u64(seed)
    }

    // ── GENE_SPECS table ──────────────────────────────────────────────────────

    /// REQ-001: GENE_SPECS must cover all 6 solver types.
    #[test]
    fn test_gene_specs_table_completeness() {
        let specs = gene_specs();
        for k in &[
            "sewer",
            "water_supply",
            "conveyance",
            "distribution",
            "pump_station",
            "intake",
        ] {
            assert!(specs.contains_key(k), "GENE_SPECS missing key '{k}'");
        }
        assert_eq!(specs.len(), 6);
    }

    /// REQ-001: Sewer has 5 genes with correct bounds.
    #[test]
    fn test_gene_specs_sewer_bounds() {
        let all_specs = gene_specs();
        let specs = all_specs.get("sewer").expect("sewer key must exist");
        assert_eq!(specs.len(), 5);
        assert_eq!(specs[0].name, "route_variant");
        assert_eq!(specs[0].lower_bound, 0.0);
        assert_eq!(specs[0].upper_bound, 9.0);
        assert_eq!(specs[0].dtype, GeneType::Int);
        assert_eq!(specs[1].name, "slope_factor");
        assert_eq!(specs[1].lower_bound, 0.5);
        assert_eq!(specs[1].upper_bound, 2.0);
        assert_eq!(specs[1].dtype, GeneType::Float);
        assert_eq!(specs[4].name, "manhole_spacing");
        assert_eq!(specs[4].lower_bound, 60.0);
        assert_eq!(specs[4].upper_bound, 120.0);
    }

    // ── random_individual ─────────────────────────────────────────────────────

    /// REQ-001 Scenario: Random individual is within bounds (sewer).
    #[test]
    fn test_random_individual_within_bounds_sewer() {
        let mut rng = make_rng(42);
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
            let mut rng = make_rng(42);
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

    // ── decode ────────────────────────────────────────────────────────────────

    /// REQ-001 Scenario: Decode integer gene rounds correctly.
    #[test]
    fn test_decode_integer_gene_rounds() {
        let ind = Individual {
            genes: vec![0.0, 1.0, 1.0, 2.7, 60.0],
            fitness: None,
        };
        let decoded = IndividualEncoder::decode(&ind, SolverType::Sewer).unwrap();
        assert_eq!(*decoded.get("diameter_offset").unwrap(), 3.0);
    }

    /// REQ-001: Decode float gene clamps to upper bound.
    #[test]
    fn test_decode_float_gene_clamps_upper() {
        let ind = Individual {
            genes: vec![0.0, 2.1, 1.0, 0.0, 60.0],
            fitness: None,
        };
        let decoded = IndividualEncoder::decode(&ind, SolverType::Sewer).unwrap();
        let sf = decoded.get("slope_factor").unwrap();
        assert!(
            (sf - 2.0).abs() < 1e-12,
            "slope_factor 2.1 should clamp to 2.0, got {sf}"
        );
    }

    /// REQ-001 Scenario: Decode rejects wrong-length chromosome.
    #[test]
    fn test_decode_rejects_wrong_length() {
        let ind = Individual {
            genes: vec![0.0, 1.0, 1.0],
            fitness: None,
        };
        match IndividualEncoder::decode(&ind, SolverType::Sewer) {
            Err(crate::errors::OptimizationError::InvalidConfig(msg)) => {
                assert!(msg.contains("chromosome length mismatch"), "got: {msg}");
            }
            other => panic!("expected Err(InvalidConfig), got {other:?}"),
        }
    }

    /// REQ-001: encode→decode round-trip for sewer.
    #[test]
    fn test_encode_decode_roundtrip_sewer() {
        let mut rng = make_rng(99);
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

    /// REQ-001: encode→decode round-trip for water_supply (WARNING-1 fix).
    #[test]
    fn test_encode_decode_roundtrip_water_supply() {
        let mut rng = make_rng(1);
        let ind = IndividualEncoder::random_individual(SolverType::WaterSupply, &mut rng);
        let decoded = IndividualEncoder::decode(&ind, SolverType::WaterSupply).unwrap();
        let specs = gene_specs().get("water_supply").unwrap();
        for spec in specs {
            let val = decoded.get(spec.name).unwrap();
            assert!(*val >= spec.lower_bound && *val <= spec.upper_bound);
            if spec.dtype == GeneType::Int {
                assert_eq!(*val, val.round());
            }
        }
    }

    /// REQ-001: encode→decode round-trip for conveyance.
    #[test]
    fn test_encode_decode_roundtrip_conveyance() {
        let mut rng = make_rng(2);
        let ind = IndividualEncoder::random_individual(SolverType::Conveyance, &mut rng);
        let decoded = IndividualEncoder::decode(&ind, SolverType::Conveyance).unwrap();
        let specs = gene_specs().get("conveyance").unwrap();
        for spec in specs {
            let val = decoded.get(spec.name).unwrap();
            assert!(*val >= spec.lower_bound && *val <= spec.upper_bound);
            if spec.dtype == GeneType::Int {
                assert_eq!(*val, val.round());
            }
        }
    }

    /// REQ-001: encode→decode round-trip for distribution.
    #[test]
    fn test_encode_decode_roundtrip_distribution() {
        let mut rng = make_rng(3);
        let ind = IndividualEncoder::random_individual(SolverType::Distribution, &mut rng);
        let decoded = IndividualEncoder::decode(&ind, SolverType::Distribution).unwrap();
        let specs = gene_specs().get("distribution").unwrap();
        for spec in specs {
            let val = decoded.get(spec.name).unwrap();
            assert!(*val >= spec.lower_bound && *val <= spec.upper_bound);
            if spec.dtype == GeneType::Int {
                assert_eq!(*val, val.round());
            }
        }
    }

    /// REQ-001: encode→decode round-trip for pump_station.
    #[test]
    fn test_encode_decode_roundtrip_pump_station() {
        let mut rng = make_rng(4);
        let ind = IndividualEncoder::random_individual(SolverType::PumpStation, &mut rng);
        let decoded = IndividualEncoder::decode(&ind, SolverType::PumpStation).unwrap();
        let specs = gene_specs().get("pump_station").unwrap();
        for spec in specs {
            let val = decoded.get(spec.name).unwrap();
            assert!(*val >= spec.lower_bound && *val <= spec.upper_bound);
            if spec.dtype == GeneType::Int {
                assert_eq!(*val, val.round());
            }
        }
    }

    /// REQ-001: encode→decode round-trip for intake.
    #[test]
    fn test_encode_decode_roundtrip_intake() {
        let mut rng = make_rng(5);
        let ind = IndividualEncoder::random_individual(SolverType::Intake, &mut rng);
        let decoded = IndividualEncoder::decode(&ind, SolverType::Intake).unwrap();
        let specs = gene_specs().get("intake").unwrap();
        for spec in specs {
            let val = decoded.get(spec.name).unwrap();
            assert!(*val >= spec.lower_bound && *val <= spec.upper_bound);
            if spec.dtype == GeneType::Int {
                assert_eq!(*val, val.round());
            }
        }
    }

    /// REQ-001: negative value clamps to lower_bound.
    #[test]
    fn test_decode_integer_gene_clamps_below_lower_bound() {
        let ind = Individual {
            genes: vec![-0.5, 1.0, 1.0, 0.0, 60.0],
            fitness: None,
        };
        let decoded = IndividualEncoder::decode(&ind, SolverType::Sewer).unwrap();
        assert_eq!(*decoded.get("route_variant").unwrap(), 0.0);
    }

    /// REQ-001: value above upper_bound clamps.
    #[test]
    fn test_decode_integer_gene_clamps_above_upper_bound() {
        let ind = Individual {
            genes: vec![9.7, 1.0, 1.0, 0.0, 60.0],
            fitness: None,
        };
        let decoded = IndividualEncoder::decode(&ind, SolverType::Sewer).unwrap();
        assert_eq!(*decoded.get("route_variant").unwrap(), 9.0);
    }
}
