//! Output structs: SolutionScore, Solution, Diagnostics, DesignResult.
//!
//! Mirrors the Python oracle (`hydro_engine/solvers/base.py`,
//! `hydro_engine/optimization/results.py`) and the JSON output schema (R-02).
//! The engine MUST NOT include `mcp_instructions` in output (spec S-23).

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::network::PipeNetwork;

// ── SolutionScore ─────────────────────────────────────────────────────────────

/// Ranking score for a design solution.
///
/// Mirrors `SolutionScore` from `hydro_engine/solvers/base.py`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SolutionScore {
    pub total_cost: f64,
    pub total_length: f64,
    pub total_excavation: f64,
    pub norm_violations: i64,
    pub pump_count: i64,
    /// Opaque per-solver dict preserved for traceability.
    #[serde(default)]
    pub details: Value,
}

impl Default for SolutionScore {
    fn default() -> Self {
        SolutionScore {
            total_cost: 0.0,
            total_length: 0.0,
            total_excavation: 0.0,
            norm_violations: 0,
            pump_count: 0,
            details: Value::Object(serde_json::Map::new()),
        }
    }
}

// ── SolutionMetadata ──────────────────────────────────────────────────────────

/// Per-solution metadata (fitness vector, genetic params, norm validation).
///
/// Typed-where-known + `extra` overflow map for forward-compatibility.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SolutionMetadata {
    /// 5-objective NSGA-III fitness tuple: (cost, excavation, pumping, interference, resilience).
    #[serde(default)]
    pub fitness: Option<[f64; 5]>,

    /// Genetic algorithm parameters for this individual.
    #[serde(default)]
    pub genetic_params: Value,

    /// Norm validation result per element id.
    #[serde(default)]
    pub norm_validation: Value,

    /// Catch-all for any additional metadata keys from the Python oracle.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, Value>,
}

// ── ConvergenceRecord ─────────────────────────────────────────────────────────

/// One entry in the NSGA-III convergence history (per-generation stats).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConvergenceRecord {
    pub generation: i64,
    pub min_cost: f64,
    pub min_excavation: f64,
    pub min_resilience_deficit: f64,
}

// ── Diagnostics ───────────────────────────────────────────────────────────────

/// Optimization diagnostics emitted by the engine.
///
/// Mirrors the `diagnostics` object in the JSON output schema (spec R-02).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Diagnostics {
    pub total_solutions: i64,
    pub feasible_count: i64,
    pub infeasible_count: i64,
    pub pareto_count: i64,
    pub norm_compliant_count: i64,
    pub generations_run: i64,
    pub population_size: i64,
    pub elapsed_seconds: f64,
    /// Always "rayon-native" in the Rust engine.
    pub compute_backend: String,
    /// SHA-256 of (request_json + seed + engine_version).
    pub audit_hash: String,
    /// Per-generation convergence history.
    #[serde(default)]
    pub convergence: Vec<ConvergenceRecord>,
}

// ── Solution ──────────────────────────────────────────────────────────────────

/// A complete Pareto-optimal design solution.
///
/// Mirrors the Solution object in the JSON output schema (spec R-02).
/// NOTE: Does NOT contain `mcp_instructions` (spec S-23).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Solution {
    /// 1-based rank (1 = lowest total_cost).
    pub rank: i64,
    pub network: PipeNetwork,
    pub score: SolutionScore,
    #[serde(default)]
    pub metadata: SolutionMetadata,
}

// ── DesignResult ─────────────────────────────────────────────────────────────
// Note: PipeNetwork already has nodes/pipes/total_length. The JSON schema also
// needs node_count/pipe_count; callers emit those via PipeNetwork methods.

/// Top-level engine output (spec R-02).
///
/// The engine MUST NOT include `mcp_instructions` (spec S-23).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DesignResult {
    pub success: bool,
    pub project_name: String,
    pub project_type: String,
    pub elapsed_seconds: f64,
    #[serde(default)]
    pub seed: Option<i64>,
    pub norm: String,
    /// Pareto-optimal alternatives sorted by total_cost ascending.
    pub solutions: Vec<Solution>,
    /// solutions[0] — lowest total_cost.
    pub best_solution: Option<Solution>,
    pub diagnostics: Diagnostics,
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests (T-1.4 RED → GREEN)
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // T-1.4: SolutionScore default + JSON round-trip
    #[test]
    fn solution_score_default_values() {
        let s = SolutionScore::default();
        assert_eq!(s.total_cost, 0.0);
        assert_eq!(s.norm_violations, 0);
        assert_eq!(s.pump_count, 0);
    }

    #[test]
    fn solution_score_json_roundtrip() {
        let s = SolutionScore {
            total_cost: 123_456.78,
            total_length: 500.0,
            total_excavation: 200.0,
            norm_violations: 0,
            pump_count: 1,
            details: serde_json::json!({"cost_per_meter": 246.91}),
        };
        let json = serde_json::to_string(&s).unwrap();
        let back: SolutionScore = serde_json::from_str(&json).unwrap();
        assert!((back.total_cost - s.total_cost).abs() < 1e-9);
        assert_eq!(back.norm_violations, s.norm_violations);
    }

    // T-1.4: Diagnostics default + compute_backend field
    #[test]
    fn diagnostics_default_and_compute_backend() {
        let d = Diagnostics {
            compute_backend: "rayon-native".to_owned(),
            ..Diagnostics::default()
        };
        assert_eq!(d.compute_backend, "rayon-native");
        let json = serde_json::to_string(&d).unwrap();
        assert!(json.contains("rayon-native"));
    }

    // T-1.4: DesignResult does NOT contain mcp_instructions (S-23)
    #[test]
    fn design_result_json_does_not_contain_mcp_instructions() {
        let result = DesignResult {
            success: true,
            project_name: "Test".to_owned(),
            project_type: "sewer".to_owned(),
            elapsed_seconds: 1.0,
            seed: Some(42),
            norm: "CONAGUA_MX".to_owned(),
            solutions: vec![],
            best_solution: None,
            diagnostics: Diagnostics {
                compute_backend: "rayon-native".to_owned(),
                ..Diagnostics::default()
            },
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(
            !json.contains("mcp_instructions"),
            "mcp_instructions MUST NOT appear in DesignResult JSON"
        );
    }

    // T-1.4: DesignResult JSON round-trip preserves key fields
    #[test]
    fn design_result_json_roundtrip() {
        let result = DesignResult {
            success: true,
            project_name: "Red Norte".to_owned(),
            project_type: "sewer".to_owned(),
            elapsed_seconds: 12.5,
            seed: Some(1234),
            norm: "CONAGUA_MX".to_owned(),
            solutions: vec![],
            best_solution: None,
            diagnostics: Diagnostics::default(),
        };
        let json = serde_json::to_string(&result).unwrap();
        let back: DesignResult = serde_json::from_str(&json).unwrap();
        assert_eq!(back.project_name, result.project_name);
        assert_eq!(back.success, result.success);
    }

    // T-1.4: ConvergenceRecord round-trip
    #[test]
    fn convergence_record_roundtrip() {
        let cr = ConvergenceRecord {
            generation: 5,
            min_cost: 99_000.0,
            min_excavation: 150.0,
            min_resilience_deficit: 0.05,
        };
        let json = serde_json::to_string(&cr).unwrap();
        let back: ConvergenceRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(back.generation, cr.generation);
        assert!((back.min_cost - cr.min_cost).abs() < 1e-9);
    }
}
