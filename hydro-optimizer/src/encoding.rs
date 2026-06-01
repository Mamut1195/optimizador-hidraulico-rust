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
