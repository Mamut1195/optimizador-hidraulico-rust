//! NormValidator — element-level norm validation for hydraulic solutions.
//!
//! Faithful port of Python `hydro_engine.norms.validator.NormValidator`.
//!
//! The validator iterates a NormProfile's rules for a given project type and
//! produces a `NormValidationResult` with per-element violations. The
//! `hard_violation_count` field is the primary signal: non-zero means the
//! solution is non-compliant.

use hydro_hydraulics::{hazen_williams, manning};
use hydro_types::{
    enums::{NodeType, ProjectType, Severity},
    network::{NetworkNode, NetworkPipe, PipeNetwork},
};

use crate::types::{ElementType, NormProfile, NormRule, NormValidationResult, NormViolation};

/// Tolerance used in `_check_rule` — mirrors Python `_TOLERANCE = 1e-9`.
const TOLERANCE: f64 = 1e-9;

// ── NormValidator ─────────────────────────────────────────────────────────────

/// Validates a pipe network against a norm profile.
///
/// Mirrors Python `NormValidator`. The `terrain` parameter is deferred to
/// hydro-solvers where ground elevation data is available; for hydro-norms,
/// `node.z` is used as the ground elevation (Python fallback when
/// `terrain=None`).
pub struct NormValidator {
    pub profile: NormProfile,
}

impl NormValidator {
    /// Construct a validator for the given norm profile.
    pub fn new(profile: NormProfile) -> Self {
        NormValidator { profile }
    }

    /// Validate a pipe network against the profile for the given project type.
    ///
    /// Mirrors Python `validate_solution`: iterates all rules for the project
    /// type and aggregates element-level violations. `compliant = hard_count == 0`.
    pub fn validate(
        &self,
        network: &PipeNetwork,
        project_type: ProjectType,
    ) -> NormValidationResult {
        let rules = self.profile.rules_for(project_type);
        let mut violations: Vec<NormViolation> = Vec::new();

        for rule in &rules {
            match rule.element_type {
                ElementType::Pipe => {
                    for pipe in &network.pipes {
                        if let Some(v) = self.validate_pipe_rule(network, project_type, rule, pipe)
                        {
                            violations.push(v);
                        }
                    }
                }
                ElementType::Node => {
                    for node in &network.nodes {
                        if let Some(v) = self.validate_node_rule(rule, node) {
                            violations.push(v);
                        }
                    }
                }
                ElementType::Network => {
                    // Network-level rules — not exercised in v1; skip silently.
                }
            }
        }

        let hard_count = violations
            .iter()
            .filter(|v| v.severity == Severity::Hard)
            .count();
        let warning_count = violations
            .iter()
            .filter(|v| v.severity == Severity::Warning)
            .count();

        NormValidationResult {
            compliant: hard_count == 0,
            hard_violation_count: hard_count,
            warning_count,
            violations,
        }
    }

    /// Check one pipe rule against one pipe.
    ///
    /// Returns `Some(NormViolation)` when the pipe violates the rule, `None` otherwise.
    /// Mirrors Python `_validate_pipe_rule` iteration body.
    fn validate_pipe_rule(
        &self,
        network: &PipeNetwork,
        project_type: ProjectType,
        rule: &NormRule,
        pipe: &NetworkPipe,
    ) -> Option<NormViolation> {
        let actual = match rule.key.as_str() {
            "min_slope" | "max_slope" => {
                // Pipes with bypassed_by_pump=true skip gravity slope rules.
                // Python: `if pipe.metadata.get("bypassed_by_pump"): continue`
                if pipe_is_pump_bypassed(pipe) {
                    return None;
                }
                pipe.slope.abs()
            }

            "min_cover" | "max_depth" => {
                // Cover = minimum of (start_cover, end_cover).
                // Python: `min(start_cover, end_cover)` where
                //   start_cover = ground_elevation(start_node) - pipe.start_invert
                //   end_cover   = ground_elevation(end_node)   - pipe.end_invert
                let start_node = network.get_node(&pipe.start_node_id)?;
                let end_node = network.get_node(&pipe.end_node_id)?;
                let start_cover = start_node.z - pipe.start_invert;
                let end_cover = end_node.z - pipe.end_invert;
                start_cover.min(end_cover)
            }

            "min_velocity" | "max_velocity" => {
                // Velocity computation depends on flow regime (project type).
                // Pressure types (WaterSupply, Distribution, Conveyance):
                //   use Hazen-Williams velocity(flow_m3s, diameter).
                //   `flow_m3s` must be in pipe.metadata["flow_m3s"].
                // Gravity types (Sewer, PumpStation, Intake):
                //   use Manning full_flow_velocity(diameter, slope, roughness).
                pipe_velocity(project_type, pipe)?
            }

            "min_diameter" | "max_diameter" => pipe.diameter,

            _ => {
                // Unknown rule key — skip (Python: `continue`).
                return None;
            }
        };

        check_rule(rule, &pipe.id.0, actual)
    }

    /// Check one node rule against one node.
    ///
    /// Only handles `min_pressure`/`max_pressure`. Skips Tank/Reservoir nodes
    /// and nodes where `pressure_mca` is not in metadata.
    /// Mirrors Python `_validate_node_rule`.
    fn validate_node_rule(&self, rule: &NormRule, node: &NetworkNode) -> Option<NormViolation> {
        if !matches!(rule.key.as_str(), "min_pressure" | "max_pressure") {
            return None;
        }
        // Skip storage nodes — they operate under static head, not service pressure.
        if matches!(node.node_type, NodeType::Tank | NodeType::Reservoir) {
            return None;
        }
        // Skip nodes without pressure metadata.
        let pressure_val = node.metadata.get("pressure_mca")?;
        let pressure = pressure_val.as_f64()?;
        check_rule(rule, &node.id.0, pressure)
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Returns true when the pipe's `bypassed_by_pump` metadata flag is set to `true`.
///
/// Python: `pipe.metadata.get("bypassed_by_pump")` — truthy when the key
/// exists and is truthy (including `true` JSON bool).
fn pipe_is_pump_bypassed(pipe: &NetworkPipe) -> bool {
    pipe.metadata
        .get("bypassed_by_pump")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

/// Compute pipe velocity for a given project type.
///
/// Returns `None` when the required data is unavailable (missing `flow_m3s`
/// for pressure types).
///
/// Mirrors Python `_pipe_velocity`.
fn pipe_velocity(project_type: ProjectType, pipe: &NetworkPipe) -> Option<f64> {
    match project_type {
        ProjectType::WaterSupply | ProjectType::Distribution | ProjectType::Conveyance => {
            // Pressure type: needs explicit flow in metadata.
            // Python: `flow = pipe.metadata.get("flow_m3s"); if flow is None: return None`
            let flow_val = pipe.metadata.get("flow_m3s")?;
            let flow = flow_val.as_f64()?;
            // Python: `return float(pressure_velocity(max(flow, 0.0), pipe.diameter))`
            Some(hazen_williams::velocity(flow.max(0.0), pipe.diameter))
        }
        ProjectType::Sewer | ProjectType::PumpStation | ProjectType::Intake => {
            // Gravity type: Manning full-pipe velocity.
            // Python: `return float(full_flow_velocity(pipe.diameter, abs(pipe.slope), pipe.roughness))`
            Some(manning::full_flow_velocity(
                pipe.diameter,
                pipe.slope.abs(),
                pipe.roughness,
            ))
        }
    }
}

/// Check a single rule against an actual value, returning a violation or None.
///
/// Mirrors Python `_check_rule` exactly:
///   - `actual < min_value - TOLERANCE` → violation
///   - `actual > max_value + TOLERANCE` → violation
///   - Violation `actual` and `limit` fields: `round(actual, 6)` (Python).
fn check_rule(rule: &NormRule, element_id: &str, actual: f64) -> Option<NormViolation> {
    if let Some(min_val) = rule.min_value {
        if actual < min_val - TOLERANCE {
            return Some(make_violation(rule, element_id, actual, min_val, "below"));
        }
    }
    if let Some(max_val) = rule.max_value {
        if actual > max_val + TOLERANCE {
            return Some(make_violation(rule, element_id, actual, max_val, "above"));
        }
    }
    None
}

/// Construct a `NormViolation` with rounded actual/limit fields.
///
/// Mirrors Python `_violation`:
///   `actual=round(float(actual), 6)`, `limit=round(float(limit), 6)`.
fn make_violation(
    rule: &NormRule,
    element_id: &str,
    actual: f64,
    limit: f64,
    relation: &str,
) -> NormViolation {
    // Python: `round(float(actual), 6)` — round to 6 decimal places.
    let actual_r = round6(actual);
    let limit_r = round6(limit);
    let element_type_str = match rule.element_type {
        ElementType::Pipe => "pipe",
        ElementType::Node => "node",
        ElementType::Network => "network",
    };
    NormViolation {
        rule_key: rule.key.clone(),
        severity: rule.severity,
        element_type: rule.element_type,
        element_id: element_id.to_string(),
        actual: actual_r,
        limit: limit_r,
        units: rule.units.clone(),
        message: format!(
            "{} {} {} is {} the allowed limit.",
            capitalize(element_type_str),
            element_id,
            rule.key,
            relation
        ),
    }
}

/// Round to 6 decimal places — mirrors Python `round(x, 6)`.
#[inline]
fn round6(x: f64) -> f64 {
    (x * 1_000_000.0).round() / 1_000_000.0
}

/// Capitalize the first character of a string (Python `.title()` on single word).
fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().to_string() + chars.as_str(),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Unit tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ElementType;

    #[test]
    fn round6_matches_python_round_6_decimal_places() {
        // 0.0010001 rounded to 6 decimal places = 0.000000 → 0.001000 (6 places after dot)
        // round(0.0010001, 6) in Python = 0.0010001 (has 7 sig digits but 6 decimal places)
        // Actually: 0.0010001 has 7 decimal places; round to 6 = 0.0010001 rounds at 7th → 0.001000
        // Python: round(0.0010001, 6) == 0.0010001? No: 0.001000|1 → digit 7 = 1 < 5, so 0.0010
        // Actually 0.0010001 at 6 decimals: 0.001000 (the 1 at 7th decimal is < 5).
        assert_eq!(round6(0.001_000_1), 0.001_000); // 7th decimal 1 → rounds down
        assert_eq!(round6(1.234_567_89), 1.234_568); // 7th decimal 8 → rounds up
        assert_eq!(round6(0.0), 0.0);
    }

    #[test]
    fn check_rule_returns_none_when_within_bounds() {
        let rule = crate::types::NormRule {
            key: "min_slope".into(),
            element_type: ElementType::Pipe,
            severity: Severity::Hard,
            min_value: Some(0.005),
            max_value: Some(0.05),
            units: "m/m".into(),
            description: "test".into(),
            source: None,
        };
        // Within bounds: 0.02 ∈ [0.005, 0.05]
        assert!(check_rule(&rule, "p1", 0.02).is_none());
    }

    #[test]
    fn check_rule_returns_violation_when_below_min() {
        let rule = crate::types::NormRule {
            key: "min_slope".into(),
            element_type: ElementType::Pipe,
            severity: Severity::Hard,
            min_value: Some(0.005),
            max_value: None,
            units: "m/m".into(),
            description: "test".into(),
            source: None,
        };
        let v = check_rule(&rule, "p1", 0.001).unwrap();
        assert_eq!(v.actual, 0.001);
        assert_eq!(v.limit, 0.005);
        assert_eq!(v.severity, Severity::Hard);
    }

    #[test]
    fn check_rule_tolerance_boundary_is_respected() {
        // actual = min_value - epsilon where epsilon < TOLERANCE → no violation
        let rule = crate::types::NormRule {
            key: "min_slope".into(),
            element_type: ElementType::Pipe,
            severity: Severity::Hard,
            min_value: Some(0.005),
            max_value: None,
            units: "m/m".into(),
            description: "test".into(),
            source: None,
        };
        // actual = 0.005 - 1e-10 (within TOLERANCE) → no violation
        assert!(check_rule(&rule, "p1", 0.005 - 1e-10).is_none());
        // actual = 0.005 - 2e-9 (outside TOLERANCE) → violation
        assert!(check_rule(&rule, "p1", 0.005 - 2e-9).is_some());
    }
}
