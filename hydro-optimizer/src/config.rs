//! OptimizationConfig — GA hyperparameters + constraint flags.
//!
//! Ports Python `hydro_engine/optimization/genetic.py` config class and
//! design §2 exact field list. `#[serde(skip)]` on `progress_callback`
//! because `Box<dyn Fn>` is not serializable.
//!
//! Design §12 determinism rule: no `HashMap`; constraint lists use `Vec`.

use serde::{Deserialize, Serialize};

use crate::errors::OptimizationError;

// ── Spatial constraint types ──────────────────────────────────────────────────

/// A polygon region that no pipe may pass through.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForbiddenZone {
    /// Polygon vertices as (x, y) pairs (any closed-ring polygon).
    pub vertices: Vec<[f64; 2]>,
    /// Optional description / id for reporting.
    #[serde(default)]
    pub label: Option<String>,
}

/// A corridor that at least one pipe segment must pass through.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MandatoryRoute {
    /// Centre-line of the required corridor as (x, y) waypoints.
    pub waypoints: Vec<[f64; 2]>,
    /// Half-width of the corridor in metres.
    pub corridor_width: f64,
    /// Optional description / id.
    #[serde(default)]
    pub label: Option<String>,
}

/// An existing infrastructure network that new pipes must keep clear of.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExistingNetwork {
    /// Polyline segments as (x, y) pairs.
    pub segments: Vec<[f64; 2]>,
    /// Required clearance from new pipes (metres).
    pub clearance_distance: f64,
    /// Optional description / id.
    #[serde(default)]
    pub label: Option<String>,
}

// ── ProgressCallback ──────────────────────────────────────────────────────────

/// Per-generation callback type alias.
///
/// Receives a `ProgressEvent` reference after each generation completes.
/// Defined here for use in `OptimizationConfig`; the struct is in `progress.rs`.
pub type ProgressCallback = Box<dyn Fn(&crate::progress::ProgressEvent) + Send + Sync>;

// ── OptimizationConfig ────────────────────────────────────────────────────────

/// All hyperparameters and constraint flags for one optimization run.
///
/// Serde-deserializable. `Default` impl uses oracle defaults (population=100,
/// generations=50, crossover=0.9, mutation=0.1, seed=42, max_time=300s).
///
/// Design §2 exact field list.
#[derive(Serialize, Deserialize)]
#[serde(default)]
pub struct OptimizationConfig {
    // ── GA hyperparameters ────────────────────────────────────────────────
    /// Population size (μ). Oracle default: 100.
    pub population_size: u32,
    /// Maximum number of generations to run. Oracle default: 50.
    pub generations: u32,
    /// SBX crossover probability per offspring. Oracle default: 0.9.
    pub crossover_prob: f64,
    /// Polynomial mutation probability per offspring. Oracle default: 0.1.
    pub mutation_prob: f64,
    /// Wall-clock time budget (seconds). Oracle default: 300.0.
    pub max_time_seconds: f64,
    /// Enable adaptive eta schedule for SBX / mutation. Oracle default: true.
    pub adaptive_eta: bool,
    /// Minimum eta value for adaptive schedule. Oracle default: 5.0.
    pub eta_min: f64,
    /// Maximum eta value for adaptive schedule. Oracle default: 50.0.
    pub eta_max: f64,
    /// RNG seed for full reproducibility. Oracle default: 42.
    pub seed: u64,
    /// Optional rayon thread-pool size. `None` = use rayon default.
    pub num_workers: Option<u32>,

    // ── Spatial constraints ───────────────────────────────────────────────
    /// Polygons that no pipe may enter.
    pub forbidden_zones: Vec<ForbiddenZone>,
    /// Corridors that at least one pipe segment must pass through.
    pub mandatory_routes: Vec<MandatoryRoute>,
    /// Existing infrastructure that new pipes must stay clear of.
    pub existing_networks: Vec<ExistingNetwork>,

    // ── Geometric constraint flags ────────────────────────────────────────
    /// Maximum allowed deflection angle between consecutive pipe segments (deg).
    /// Oracle default: 45.0.
    pub max_bend_angle: f64,
    /// Whether to enforce minimum cover depth constraint. Oracle default: false.
    pub enforce_cover_depth: bool,
    /// Whether to enforce minimum slope constraint. Oracle default: false.
    pub enforce_slope: bool,
    /// Whether to enforce clearance from existing networks. Oracle default: false.
    pub enforce_clearance: bool,

    // ── Path smoothing ────────────────────────────────────────────────────
    /// Enable A*-based path smoothing after the main GA loop. Oracle default: false.
    pub enable_path_smoothing: bool,

    // ── Multi-objective weights for best_balanced() ───────────────────────
    pub weight_cost: f64,
    pub weight_excavation: f64,
    pub weight_pumping: f64,
    pub weight_interference: f64,
    pub weight_resilience: f64,

    // ── Norm compliance ───────────────────────────────────────────────────
    /// Norm profile identifier (string name). `None` = no norm checking.
    pub norm_profile: Option<String>,
    /// If true, solutions violating hard norm rules are discarded.
    pub strict_norm_compliance: bool,

    // ── Callback (not serialized) ─────────────────────────────────────────
    /// Optional per-generation progress callback. Skipped on (de)serialize.
    #[serde(skip)]
    pub progress_callback: Option<ProgressCallback>,
}

impl std::fmt::Debug for OptimizationConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OptimizationConfig")
            .field("population_size", &self.population_size)
            .field("generations", &self.generations)
            .field("crossover_prob", &self.crossover_prob)
            .field("mutation_prob", &self.mutation_prob)
            .field("max_time_seconds", &self.max_time_seconds)
            .field("adaptive_eta", &self.adaptive_eta)
            .field("eta_min", &self.eta_min)
            .field("eta_max", &self.eta_max)
            .field("seed", &self.seed)
            .field("num_workers", &self.num_workers)
            .field("enable_path_smoothing", &self.enable_path_smoothing)
            .field("strict_norm_compliance", &self.strict_norm_compliance)
            .field(
                "progress_callback",
                &self.progress_callback.as_ref().map(|_| "<callback>"),
            )
            .finish_non_exhaustive()
    }
}

impl Clone for OptimizationConfig {
    fn clone(&self) -> Self {
        OptimizationConfig {
            population_size: self.population_size,
            generations: self.generations,
            crossover_prob: self.crossover_prob,
            mutation_prob: self.mutation_prob,
            max_time_seconds: self.max_time_seconds,
            adaptive_eta: self.adaptive_eta,
            eta_min: self.eta_min,
            eta_max: self.eta_max,
            seed: self.seed,
            num_workers: self.num_workers,
            forbidden_zones: self.forbidden_zones.clone(),
            mandatory_routes: self.mandatory_routes.clone(),
            existing_networks: self.existing_networks.clone(),
            max_bend_angle: self.max_bend_angle,
            enforce_cover_depth: self.enforce_cover_depth,
            enforce_slope: self.enforce_slope,
            enforce_clearance: self.enforce_clearance,
            enable_path_smoothing: self.enable_path_smoothing,
            weight_cost: self.weight_cost,
            weight_excavation: self.weight_excavation,
            weight_pumping: self.weight_pumping,
            weight_interference: self.weight_interference,
            weight_resilience: self.weight_resilience,
            norm_profile: self.norm_profile.clone(),
            strict_norm_compliance: self.strict_norm_compliance,
            progress_callback: None, // callbacks are not cloneable
        }
    }
}

impl Default for OptimizationConfig {
    fn default() -> Self {
        OptimizationConfig {
            population_size: 100,
            generations: 50,
            crossover_prob: 0.9,
            mutation_prob: 0.1,
            max_time_seconds: 300.0,
            adaptive_eta: true,
            eta_min: 5.0,
            eta_max: 50.0,
            seed: 42,
            num_workers: None,
            forbidden_zones: Vec::new(),
            mandatory_routes: Vec::new(),
            existing_networks: Vec::new(),
            max_bend_angle: 45.0,
            enforce_cover_depth: false,
            enforce_slope: false,
            enforce_clearance: false,
            enable_path_smoothing: false,
            weight_cost: 1.0,
            weight_excavation: 1.0,
            weight_pumping: 1.0,
            weight_interference: 1.0,
            weight_resilience: 1.0,
            norm_profile: None,
            strict_norm_compliance: false,
            progress_callback: None,
        }
    }
}

impl OptimizationConfig {
    /// Validate that the config's values are internally consistent.
    ///
    /// Called by `GeneticOptimizer::new`. Returns `Err(InvalidConfig)` on the
    /// first violation found.
    pub fn validate(&self) -> Result<(), OptimizationError> {
        if self.population_size == 0 {
            return Err(OptimizationError::InvalidConfig(
                "population_size must be > 0".to_owned(),
            ));
        }
        if self.generations == 0 {
            return Err(OptimizationError::InvalidConfig(
                "generations must be > 0".to_owned(),
            ));
        }
        if !(0.0..=1.0).contains(&self.crossover_prob) {
            return Err(OptimizationError::InvalidConfig(format!(
                "crossover_prob must be in [0, 1], got {}",
                self.crossover_prob
            )));
        }
        if !(0.0..=1.0).contains(&self.mutation_prob) {
            return Err(OptimizationError::InvalidConfig(format!(
                "mutation_prob must be in [0, 1], got {}",
                self.mutation_prob
            )));
        }
        if self.max_time_seconds <= 0.0 {
            return Err(OptimizationError::InvalidConfig(
                "max_time_seconds must be > 0".to_owned(),
            ));
        }
        if self.eta_min <= 0.0 || self.eta_max <= 0.0 {
            return Err(OptimizationError::InvalidConfig(
                "eta_min and eta_max must be > 0".to_owned(),
            ));
        }
        if self.eta_min > self.eta_max {
            return Err(OptimizationError::InvalidConfig(format!(
                "eta_min ({}) must be <= eta_max ({})",
                self.eta_min, self.eta_max
            )));
        }
        // Weight validation — each weight must be non-negative
        for (name, w) in [
            ("weight_cost", self.weight_cost),
            ("weight_excavation", self.weight_excavation),
            ("weight_pumping", self.weight_pumping),
            ("weight_interference", self.weight_interference),
            ("weight_resilience", self.weight_resilience),
        ] {
            if w < 0.0 {
                return Err(OptimizationError::InvalidConfig(format!(
                    "{name} must be >= 0, got {w}"
                )));
            }
        }
        Ok(())
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests (REQ-002, REQ-013)
// Previously in tests/pr8a_config.rs and tests/pr8b_verify_fixes.rs.
// Moved inline (REQ-014: no pub re-exports for internal types).
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use crate::errors::OptimizationError;
    use crate::rng::{child_rng, root_rng};

    /// REQ-002 Scenario: Default config is valid.
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

    /// REQ-002: Serde round-trip.
    #[test]
    fn test_config_serde_roundtrip() {
        let cfg = OptimizationConfig::default();
        let json = serde_json::to_string(&cfg).expect("serialize must succeed");
        let back: OptimizationConfig =
            serde_json::from_str(&json).expect("deserialize must succeed");
        assert_eq!(back.population_size, cfg.population_size);
        assert_eq!(back.generations, cfg.generations);
        assert_eq!(back.seed, cfg.seed);
    }

    /// REQ-002 Scenario: Invalid population size is rejected.
    #[test]
    fn test_config_invalid_population_rejected() {
        let cfg = OptimizationConfig {
            population_size: 0,
            ..Default::default()
        };
        match cfg.validate() {
            Err(OptimizationError::InvalidConfig(_)) => {}
            other => panic!("expected Err(InvalidConfig), got {other:?}"),
        }
    }

    /// REQ-002: validate() rejects generations=0.
    #[test]
    fn test_config_rejects_generations_zero() {
        let cfg = OptimizationConfig {
            generations: 0,
            ..Default::default()
        };
        match cfg.validate() {
            Err(OptimizationError::InvalidConfig(msg)) => {
                assert!(msg.contains("generations"), "got: {msg}");
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
                assert!(msg.contains("crossover"), "got: {msg}");
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
                assert!(msg.contains("crossover"), "got: {msg}");
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
                assert!(msg.contains("mutation"), "got: {msg}");
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
                assert!(msg.contains("mutation"), "got: {msg}");
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
                assert!(msg.contains("max_time"), "got: {msg}");
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
                assert!(msg.contains("eta"), "got: {msg}");
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
                assert!(msg.contains("eta"), "got: {msg}");
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
                assert!(msg.contains("weight"), "got: {msg}");
            }
            other => panic!("expected Err(InvalidConfig), got {other:?}"),
        }
    }

    // ── RNG helpers (REQ-015) ─────────────────────────────────────────────────

    /// Design §5: root_rng with same seed produces same sequence.
    #[test]
    fn test_rng_root_produces_reproducible_sequence() {
        use rand::RngCore;
        let mut rng1 = root_rng(42);
        let mut rng2 = root_rng(42);
        assert_eq!(rng1.next_u64(), rng2.next_u64());
        assert_eq!(rng1.next_u64(), rng2.next_u64());
    }

    /// Design §5: child_rng with different indices diverges.
    #[test]
    fn test_child_rng_diverges_for_different_indices() {
        use rand::RngCore;
        let mut rng0 = child_rng(42, 0, 0);
        let mut rng1 = child_rng(42, 0, 1);
        assert_ne!(rng0.next_u64(), rng1.next_u64());
    }

    // ── OptimizationError display (REQ-013) ───────────────────────────────────

    #[test]
    fn test_error_display_messages() {
        let e = OptimizationError::InvalidConfig("bad pop size".to_owned());
        let msg = format!("{e}");
        assert!(
            msg.contains("config") || msg.contains("invalid"),
            "got: {msg}"
        );

        let e2 = OptimizationError::AllInfeasible;
        let msg2 = format!("{e2}");
        assert!(
            msg2.contains("feasible") || msg2.contains("infeasible"),
            "got: {msg2}"
        );

        let e3 = OptimizationError::EvaluatorFailure("solver error".to_owned());
        let msg3 = format!("{e3}");
        assert!(
            msg3.contains("solver") || msg3.contains("evaluat"),
            "got: {msg3}"
        );

        let e4 = OptimizationError::NormValidationFailure("profile not found".to_owned());
        let msg4 = format!("{e4}");
        assert!(
            msg4.contains("norm") || msg4.contains("valid"),
            "got: {msg4}"
        );
    }

    #[test]
    fn test_error_solver_variant_display() {
        use hydro_solvers::SolverError;
        let inner = SolverError::BuildFailed("test".to_owned());
        let e = OptimizationError::from(inner);
        let msg = format!("{e}");
        assert!(
            msg.contains("solver") || msg.contains("error"),
            "got: {msg}"
        );
    }

    #[test]
    fn test_error_time_budget_exceeded_display() {
        let e = OptimizationError::TimeBudgetExceeded(12.5);
        let msg = format!("{e}");
        assert!(
            msg.contains("budget") || msg.contains("12.5") || msg.contains("wall"),
            "got: {msg}"
        );
    }

    #[test]
    fn test_error_internal_variant_display() {
        let e = OptimizationError::Internal("niche count overflow".to_owned());
        let msg = format!("{e}");
        assert!(
            msg.contains("internal") || msg.contains("invariant"),
            "got: {msg}"
        );
    }
}
