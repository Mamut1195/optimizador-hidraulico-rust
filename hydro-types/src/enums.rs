//! Core domain enums for the hydraulic optimization engine.
//!
//! These mirror the Python oracle at `hydro_engine/core/types.py` exactly.
//! Serde renames match the JSON contract defined in R-01 / R-02 of the spec.

use serde::{Deserialize, Serialize};

// ── ProjectType ──────────────────────────────────────────────────────────────

/// Hydraulic infrastructure project category.
///
/// Serializes to / from the lowercase snake_case strings used in the
/// JSON DesignRequest `project_type` field (spec R-01, S-02).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectType {
    Sewer,
    WaterSupply,
    Conveyance,
    Distribution,
    PumpStation,
    Intake,
}

// ── NodeType ─────────────────────────────────────────────────────────────────

/// Classification of a network node.
///
/// Serializes to / from the SCREAMING_SNAKE_CASE strings used in the
/// JSON Node `node_type` field (spec R-02).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NodeType {
    Manhole,
    Junction,
    Outlet,
    Inlet,
    Pump,
    Valve,
    Hydrant,
    Tank,
    Reservoir,
    Service,
}

// ── PipeMaterial ─────────────────────────────────────────────────────────────

/// Pipe material, with Manning's n pinned per R-06.
///
/// Serializes using the canonical UPPER strings (PVC, CONCRETE, HDPE, STEEL,
/// CAST_IRON). A custom deserializer handles the Spanish aliases
/// (PEAD → HDPE, CONCRETO → CONCRETE, ACERO → STEEL, FIERRO_FUNDIDO → CAST_IRON)
/// that the Python oracle accepts in the `material` field (spec R-01, S-21).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PipeMaterial {
    Pvc,
    Concrete,
    Hdpe,
    Steel,
    CastIron,
}

impl PipeMaterial {
    /// Manning's roughness coefficient `n` for this material.
    ///
    /// Values pinned from the Python oracle (spec R-06, S-21).
    pub fn manning_n(self) -> f64 {
        match self {
            PipeMaterial::Pvc => 0.009,
            PipeMaterial::Concrete => 0.013,
            PipeMaterial::Hdpe => 0.009,
            PipeMaterial::Steel => 0.012,
            PipeMaterial::CastIron => 0.013,
        }
    }

    /// Canonical string representation used in JSON output.
    pub fn as_str(self) -> &'static str {
        match self {
            PipeMaterial::Pvc => "PVC",
            PipeMaterial::Concrete => "CONCRETE",
            PipeMaterial::Hdpe => "HDPE",
            PipeMaterial::Steel => "STEEL",
            PipeMaterial::CastIron => "CAST_IRON",
        }
    }

    /// Resolve from a string including Spanish aliases.
    ///
    /// Accepts: PVC, CONCRETE, HDPE, STEEL, CAST_IRON (canonical)
    ///          PEAD, CONCRETO, ACERO, FIERRO_FUNDIDO, CASTIRON (aliases)
    pub fn from_str_with_aliases(s: &str) -> Option<Self> {
        let normalized = s.trim().to_uppercase().replace([' ', '-'], "_");
        match normalized.as_str() {
            "PVC" => Some(PipeMaterial::Pvc),
            "CONCRETE" | "CONCRETO" => Some(PipeMaterial::Concrete),
            "HDPE" | "PEAD" => Some(PipeMaterial::Hdpe),
            "STEEL" | "ACERO" => Some(PipeMaterial::Steel),
            "CAST_IRON" | "FIERRO_FUNDIDO" | "CASTIRON" => Some(PipeMaterial::CastIron),
            _ => None,
        }
    }
}

impl Serialize for PipeMaterial {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for PipeMaterial {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        PipeMaterial::from_str_with_aliases(&s)
            .ok_or_else(|| serde::de::Error::custom(format!("Unknown pipe material: '{s}'")))
    }
}

// ── FlowType ─────────────────────────────────────────────────────────────────

/// Flow regime classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FlowType {
    Gravity,
    Pressure,
}

// ── Severity ──────────────────────────────────────────────────────────────────

/// Norm rule severity level, matching Python NormSeverity literals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Hard,
    Soft,
    Warning,
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests (T-1.1 RED → GREEN)
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // T-1.1: ProjectType serde round-trip
    #[test]
    fn project_type_sewer_deserializes() {
        let pt: ProjectType = serde_json::from_str("\"sewer\"").unwrap();
        assert_eq!(pt, ProjectType::Sewer);
    }

    #[test]
    fn project_type_water_supply_deserializes() {
        let pt: ProjectType = serde_json::from_str("\"water_supply\"").unwrap();
        assert_eq!(pt, ProjectType::WaterSupply);
    }

    #[test]
    fn project_type_all_variants_roundtrip() {
        let variants = [
            ProjectType::Sewer,
            ProjectType::WaterSupply,
            ProjectType::Conveyance,
            ProjectType::Distribution,
            ProjectType::PumpStation,
            ProjectType::Intake,
        ];
        for v in variants {
            let json = serde_json::to_string(&v).unwrap();
            let back: ProjectType = serde_json::from_str(&json).unwrap();
            assert_eq!(v, back, "round-trip failed for {v:?}");
        }
    }

    #[test]
    fn project_type_unknown_variant_returns_err() {
        let result: Result<ProjectType, _> = serde_json::from_str("\"aqueduct\"");
        assert!(
            result.is_err(),
            "expected Err for unknown variant 'aqueduct'"
        );
    }

    // T-1.1: NodeType serde round-trip
    #[test]
    fn node_type_manhole_deserializes_from_screaming_case() {
        let nt: NodeType = serde_json::from_str("\"MANHOLE\"").unwrap();
        assert_eq!(nt, NodeType::Manhole);
    }

    #[test]
    fn node_type_all_variants_roundtrip() {
        let variants = [
            NodeType::Manhole,
            NodeType::Junction,
            NodeType::Outlet,
            NodeType::Inlet,
            NodeType::Pump,
            NodeType::Valve,
            NodeType::Hydrant,
            NodeType::Tank,
            NodeType::Reservoir,
            NodeType::Service,
        ];
        for v in variants {
            let json = serde_json::to_string(&v).unwrap();
            let back: NodeType = serde_json::from_str(&json).unwrap();
            assert_eq!(v, back, "round-trip failed for {v:?}");
        }
    }

    // T-1.1: PipeMaterial serde round-trip + alias resolution
    #[test]
    fn pipe_material_pvc_roundtrip() {
        let m: PipeMaterial = serde_json::from_str("\"PVC\"").unwrap();
        assert_eq!(m, PipeMaterial::Pvc);
        let json = serde_json::to_string(&m).unwrap();
        assert_eq!(json, "\"PVC\"");
    }

    #[test]
    fn pipe_material_pead_alias_resolves_to_hdpe() {
        let m: PipeMaterial = serde_json::from_str("\"PEAD\"").unwrap();
        assert_eq!(m, PipeMaterial::Hdpe);
    }

    #[test]
    fn pipe_material_concreto_alias_resolves_to_concrete() {
        let m: PipeMaterial = serde_json::from_str("\"CONCRETO\"").unwrap();
        assert_eq!(m, PipeMaterial::Concrete);
    }

    #[test]
    fn pipe_material_acero_alias_resolves_to_steel() {
        let m: PipeMaterial = serde_json::from_str("\"ACERO\"").unwrap();
        assert_eq!(m, PipeMaterial::Steel);
    }

    #[test]
    fn pipe_material_fierro_fundido_alias_resolves_to_cast_iron() {
        let m: PipeMaterial = serde_json::from_str("\"FIERRO_FUNDIDO\"").unwrap();
        assert_eq!(m, PipeMaterial::CastIron);
    }

    #[test]
    fn pipe_material_unknown_returns_err() {
        let result: Result<PipeMaterial, _> = serde_json::from_str("\"CLAY\"");
        assert!(result.is_err());
    }

    #[test]
    fn pipe_material_manning_n_values_match_spec() {
        // R-06: Manning's n table
        assert_eq!(PipeMaterial::Pvc.manning_n(), 0.009);
        assert_eq!(PipeMaterial::Concrete.manning_n(), 0.013);
        assert_eq!(PipeMaterial::Hdpe.manning_n(), 0.009);
        assert_eq!(PipeMaterial::Steel.manning_n(), 0.012);
        assert_eq!(PipeMaterial::CastIron.manning_n(), 0.013);
    }

    // T-1.1: FlowType
    #[test]
    fn flow_type_gravity_roundtrip() {
        let ft: FlowType = serde_json::from_str("\"GRAVITY\"").unwrap();
        assert_eq!(ft, FlowType::Gravity);
        let json = serde_json::to_string(&ft).unwrap();
        assert_eq!(json, "\"GRAVITY\"");
    }

    // T-1.1: Severity
    #[test]
    fn severity_hard_roundtrip() {
        let s: Severity = serde_json::from_str("\"hard\"").unwrap();
        assert_eq!(s, Severity::Hard);
        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(json, "\"hard\"");
    }
}
