//! Norm domain types for the hydro-norms crate.
//!
//! Mirrors Python `hydro_engine.norms.profile`:
//! NormSource, NormRule, NormProfile, NormViolation, NormValidationResult,
//! ElementType, ValueBasis.
//!
//! `Severity` is re-exported from hydro-types to avoid redefining it.

use std::collections::HashMap;

use serde::{Deserialize, Deserializer, Serialize};

pub use hydro_types::Severity;
use hydro_types::ProjectType;

// ── ElementType ───────────────────────────────────────────────────────────────

/// The type of hydraulic element a rule applies to.
///
/// Serializes as lowercase strings ("pipe", "node", "network") mirroring
/// Python `NormElementType = Literal["pipe", "node", "network"]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ElementType {
    Pipe,
    Node,
    Network,
}

// ── ValueBasis ────────────────────────────────────────────────────────────────

/// Basis for the numeric value of a rule.
///
/// Serializes as snake_case strings mirroring Python `NormValueBasis`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValueBasis {
    PublishedStandard,
    EngineeringDefault,
    UserDefined,
}

impl Default for ValueBasis {
    fn default() -> Self {
        ValueBasis::PublishedStandard
    }
}

// ── NormSource ────────────────────────────────────────────────────────────────

/// Traceability metadata for a normative rule value.
///
/// Mirrors Python `NormSource` Pydantic model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NormSource {
    pub agency: String,
    pub document: String,
    pub section: String,
    pub version: String,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub value_basis: ValueBasis,
    #[serde(default)]
    pub note: Option<String>,
}

// ── NormRule ──────────────────────────────────────────────────────────────────

/// A single normative rule applied to a solution element.
///
/// Mirrors Python `NormRule` Pydantic model.
/// `severity` defaults to `"hard"` matching the Python default.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NormRule {
    pub key: String,
    pub element_type: ElementType,
    #[serde(default = "default_severity")]
    pub severity: Severity,
    pub min_value: Option<f64>,
    pub max_value: Option<f64>,
    pub units: String,
    pub description: String,
    #[serde(default)]
    pub source: Option<NormSource>,
}

fn default_severity() -> Severity {
    Severity::Hard
}

// ── NormProfile ───────────────────────────────────────────────────────────────

/// Country/agency-specific set of hydraulic design rules.
///
/// `project_rules` keys are ProjectType variants. The JSON files use
/// SCREAMING_SNAKE_CASE keys ("SEWER", "WATER_SUPPLY", …) which is the
/// Rust enum variant name — we use a custom deserializer to map those.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NormProfile {
    pub code: String,
    #[serde(default)]
    pub country: Option<String>,
    #[serde(default)]
    pub agency: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default = "default_certified")]
    pub certified: bool,
    #[serde(default)]
    pub certification_note: Option<String>,
    /// Rules keyed by project type.
    ///
    /// NOTE: this field is populated by `NormRegistry::load_profile_data` which
    /// handles the raw JSON format (SCREAMING_CASE keys, copy_from, source_id
    /// references). When constructing a NormProfile directly, supply a plain
    /// HashMap<ProjectType, Vec<NormRule>>.
    #[serde(default)]
    pub project_rules: HashMap<ProjectType, Vec<NormRule>>,
}

fn default_certified() -> bool {
    true
}

impl NormProfile {
    /// Return all rules for the given project type.
    pub fn rules_for(&self, project_type: ProjectType) -> Vec<NormRule> {
        self.project_rules
            .get(&project_type)
            .cloned()
            .unwrap_or_default()
    }
}

// ── NormViolation ─────────────────────────────────────────────────────────────

/// Element-level norm violation.
///
/// Mirrors Python `NormViolation` Pydantic model.
/// `actual` and `limit` are pre-rounded to 6 decimal places by the validator.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NormViolation {
    pub rule_key: String,
    pub severity: Severity,
    pub element_type: ElementType,
    pub element_id: String,
    pub actual: f64,
    pub limit: f64,
    pub units: String,
    pub message: String,
}

// ── NormValidationResult ──────────────────────────────────────────────────────

/// Validation result for a full solution.
///
/// Mirrors Python `NormValidationResult` Pydantic model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NormValidationResult {
    pub compliant: bool,
    pub hard_violation_count: usize,
    pub warning_count: usize,
    pub violations: Vec<NormViolation>,
}

// ── ProjectType SCREAMING_CASE deserializer ───────────────────────────────────

/// Deserialize a `ProjectType` from SCREAMING_SNAKE_CASE strings as used in
/// the JSON profile files ("SEWER", "WATER_SUPPLY", "PUMP_STATION", …).
pub fn deserialize_project_type_screaming<'de, D>(
    deserializer: D,
) -> Result<ProjectType, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    match s.as_str() {
        "SEWER" => Ok(ProjectType::Sewer),
        "WATER_SUPPLY" => Ok(ProjectType::WaterSupply),
        "CONVEYANCE" => Ok(ProjectType::Conveyance),
        "DISTRIBUTION" => Ok(ProjectType::Distribution),
        "PUMP_STATION" => Ok(ProjectType::PumpStation),
        "INTAKE" => Ok(ProjectType::Intake),
        _ => Err(serde::de::Error::custom(format!(
            "unknown ProjectType key: '{s}'"
        ))),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests — T-3.1 RED → GREEN
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // T-3.1 acceptance criterion 1: NormRule serde round-trip
    #[test]
    fn norm_rule_serde_round_trip() {
        let json = r#"{"key":"min_slope","element_type":"pipe","severity":"hard",
            "min_value":0.005,"max_value":null,"units":"m/m","description":"Minimum slope.","source":null}"#;
        let rule: NormRule = serde_json::from_str(json).unwrap();
        assert_eq!(rule.key, "min_slope");
        assert_eq!(rule.element_type, ElementType::Pipe);
        assert_eq!(rule.severity, Severity::Hard);
        assert_eq!(rule.min_value, Some(0.005));
        assert!(rule.max_value.is_none());
        // round-trip
        let back = serde_json::to_string(&rule).unwrap();
        let rule2: NormRule = serde_json::from_str(&back).unwrap();
        assert_eq!(rule, rule2);
    }

    // T-3.1: NormViolation serde round-trip
    #[test]
    fn norm_violation_serde_round_trip() {
        let v = NormViolation {
            rule_key: "min_slope".into(),
            severity: Severity::Hard,
            element_type: ElementType::Pipe,
            element_id: "p1".into(),
            actual: 0.001,
            limit: 0.005,
            units: "m/m".into(),
            message: "Pipe p1 min_slope is below the allowed limit.".into(),
        };
        let json = serde_json::to_string(&v).unwrap();
        let back: NormViolation = serde_json::from_str(&json).unwrap();
        assert_eq!(v, back);
    }

    // T-3.1: Severity re-export matches hydro-types
    #[test]
    fn severity_re_export_matches_hydro_types() {
        // Severity is re-exported from hydro_types; verify the same enum values
        // are visible without importing hydro_types directly.
        let hard: Severity = serde_json::from_str("\"hard\"").unwrap();
        assert_eq!(hard, hydro_types::Severity::Hard);
        let soft: Severity = serde_json::from_str("\"soft\"").unwrap();
        assert_eq!(soft, hydro_types::Severity::Soft);
        let warning: Severity = serde_json::from_str("\"warning\"").unwrap();
        assert_eq!(warning, hydro_types::Severity::Warning);
    }

    // T-3.1: ElementType all 3 variants serialize to lowercase string
    #[test]
    fn element_type_all_variants_serialize_lowercase() {
        assert_eq!(serde_json::to_string(&ElementType::Pipe).unwrap(), "\"pipe\"");
        assert_eq!(serde_json::to_string(&ElementType::Node).unwrap(), "\"node\"");
        assert_eq!(
            serde_json::to_string(&ElementType::Network).unwrap(),
            "\"network\""
        );
        // and deserialize
        let p: ElementType = serde_json::from_str("\"pipe\"").unwrap();
        assert_eq!(p, ElementType::Pipe);
    }

    // T-3.1: NormRule default severity is "hard"
    #[test]
    fn norm_rule_default_severity_is_hard() {
        let json = r#"{"key":"min_slope","element_type":"pipe","min_value":0.005,"units":"m/m","description":"X"}"#;
        let rule: NormRule = serde_json::from_str(json).unwrap();
        assert_eq!(rule.severity, Severity::Hard);
    }

    // T-3.1: NormValidationResult serde round-trip
    #[test]
    fn norm_validation_result_serde_round_trip() {
        let result = NormValidationResult {
            compliant: false,
            hard_violation_count: 2,
            warning_count: 0,
            violations: vec![],
        };
        let json = serde_json::to_string(&result).unwrap();
        let back: NormValidationResult = serde_json::from_str(&json).unwrap();
        assert_eq!(result, back);
    }
}
