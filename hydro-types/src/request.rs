//! DesignRequest — full input schema with strict serde validation.
//!
//! Mirrors `hydro_engine/api/schemas.py` DesignRequest.
//!
//! Key differences from Python (by design, per spec):
//! - `terrain_points` is REQUIRED (not nullable) — S-01.
//! - `#[serde(deny_unknown_fields)]` rejects extra fields — S-03.
//! - Material aliases resolved at deserialization time — S-02.
//! - Norm aliases are stored as-is; resolution happens in hydro-norms — S-22.
//!
//! Cross-field and range validation is performed via `DesignRequest::validate()`.

use serde::{Deserialize, Serialize};

use crate::constraints::DesignConstraints;
use crate::enums::PipeMaterial;
use crate::error::HydroTypesError;

// ── Sub-schemas ───────────────────────────────────────────────────────────────

/// 2D coordinate (x, y).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PointXY {
    pub x: f64,
    pub y: f64,
}

/// 3D coordinate (x, y, z).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PointXYZ {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

/// Forbidden polygon constraint. Minimum 3 coordinates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolygonConstraint {
    /// At least 3 coordinate pairs.
    pub coords: Vec<PointXY>,
    /// External clearance distance in metres (grows polygon outward). 0..200.
    #[serde(default)]
    pub buffer: Option<f64>,
}

/// Mandatory route / corridor constraint. Minimum 2 coordinates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteConstraint {
    /// At least 2 coordinate pairs.
    pub coords: Vec<PointXY>,
    /// Allowed corridor width (m) around the mandatory line. 0..500.
    #[serde(default)]
    pub corridor_width: Option<f64>,
}

/// Existing network polyline for interference objective.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExistingNetworkConstraint {
    /// At least 2 coordinate pairs.
    pub coords: Vec<PointXY>,
    /// Burial depth in meters. 0..30.
    #[serde(default)]
    pub depth: Option<f64>,
    /// Per-entity horizontal clearance (m). Overrides global min_horizontal_separation.
    #[serde(default)]
    pub setback: Option<f64>,
}

/// Partial override of DesignConstraints accepted by the API.
///
/// All fields are optional. Validated ranges match Python `DesignConstraintOverrides`.
/// Cross-field validation is applied after merging with defaults.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DesignConstraintOverrides {
    pub min_slope: Option<f64>,
    pub max_slope: Option<f64>,
    pub min_cover: Option<f64>,
    pub max_depth: Option<f64>,
    pub min_velocity: Option<f64>,
    pub max_velocity: Option<f64>,
    pub min_diameter: Option<f64>,
    pub max_diameter: Option<f64>,
    pub available_diameters: Option<Vec<f64>>,
    pub max_manhole_spacing: Option<f64>,
    pub manhole_at_direction_change: Option<bool>,
    pub manhole_at_slope_change: Option<bool>,
    pub manhole_at_diameter_change: Option<bool>,
    pub min_pressure: Option<f64>,
    pub max_pressure: Option<f64>,
    pub default_material: Option<String>,
    pub default_roughness: Option<f64>,
    pub max_bend_angle: Option<f64>,
}

impl DesignConstraintOverrides {
    /// Apply overrides on top of a base `DesignConstraints`, returning the merged result.
    pub fn apply_to(&self, mut base: DesignConstraints) -> DesignConstraints {
        if let Some(v) = self.min_slope {
            base.min_slope = v;
        }
        if let Some(v) = self.max_slope {
            base.max_slope = v;
        }
        if let Some(v) = self.min_cover {
            base.min_cover = v;
        }
        if let Some(v) = self.max_depth {
            base.max_depth = v;
        }
        if let Some(v) = self.min_velocity {
            base.min_velocity = v;
        }
        if let Some(v) = self.max_velocity {
            base.max_velocity = v;
        }
        if let Some(v) = self.min_diameter {
            base.min_diameter = v;
        }
        if let Some(v) = self.max_diameter {
            base.max_diameter = v;
        }
        if let Some(ref v) = self.available_diameters {
            base.available_diameters = v.clone();
        }
        if let Some(v) = self.max_manhole_spacing {
            base.max_manhole_spacing = v;
        }
        if let Some(v) = self.manhole_at_direction_change {
            base.manhole_at_direction_change = v;
        }
        if let Some(v) = self.manhole_at_slope_change {
            base.manhole_at_slope_change = v;
        }
        if let Some(v) = self.manhole_at_diameter_change {
            base.manhole_at_diameter_change = v;
        }
        if let Some(v) = self.min_pressure {
            base.min_pressure = v;
        }
        if let Some(v) = self.max_pressure {
            base.max_pressure = v;
        }
        if let Some(ref v) = self.default_material {
            base.default_material = v.clone();
        }
        if let Some(v) = self.default_roughness {
            base.default_roughness = v;
        }
        if self.max_bend_angle.is_some() {
            base.max_bend_angle = self.max_bend_angle;
        }
        base
    }
}

// ── DesignRequest ─────────────────────────────────────────────────────────────

fn default_project_name() -> String {
    "Unnamed".to_owned()
}
fn default_material_str() -> String {
    "PVC".to_owned()
}
fn default_num_alternatives() -> u32 {
    3
}
fn default_norm() -> String {
    "CONAGUA_MX".to_owned()
}
fn default_strict_norm() -> bool {
    true
}
fn default_flow_per_service() -> f64 {
    0.002
}
fn default_grid_resolution() -> f64 {
    20.0
}
fn default_nsga_population_size() -> u32 {
    100
}
fn default_nsga_generations() -> u32 {
    50
}
fn default_nsga_max_time_seconds() -> u32 {
    300
}
fn default_enforce_bool() -> bool {
    false
}
fn default_weight_cost() -> f64 {
    0.30
}
fn default_weight_excavation() -> f64 {
    0.25
}
fn default_weight_pumping() -> f64 {
    0.15
}
fn default_weight_interference() -> f64 {
    0.10
}
fn default_weight_resilience() -> f64 {
    0.20
}
fn default_min_vertical_separation() -> f64 {
    0.3
}
fn default_min_horizontal_separation() -> f64 {
    1.0
}

/// Full DesignRequest schema.
///
/// `#[serde(deny_unknown_fields)]` enforces S-03: extra fields are rejected.
/// `terrain_points` is required (no default, no Option) — S-01.
/// Material aliases are resolved by a custom deserializer — R-01.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DesignRequest {
    /// Project type — one of: sewer, water_supply, conveyance, distribution,
    /// pump_station, intake. Lowercase/snake_case strings (R-01, S-02).
    pub project_type: ProjectTypeStr,

    #[serde(default = "default_project_name")]
    pub project_name: String,

    /// Terrain survey points (REQUIRED in Rust — S-01). Min 3 points.
    pub terrain_points: Vec<PointXYZ>,

    #[serde(default)]
    pub service_points: Option<Vec<PointXY>>,

    #[serde(default)]
    pub outlet: Option<PointXY>,

    #[serde(default)]
    pub source: Option<PointXY>,

    #[serde(default)]
    pub source_head: Option<f64>,

    #[serde(default)]
    pub constraints: Option<DesignConstraintOverrides>,

    #[serde(default)]
    pub forbidden_zones: Vec<PolygonConstraint>,

    #[serde(default)]
    pub mandatory_routes: Vec<RouteConstraint>,

    #[serde(default)]
    pub existing_networks: Vec<ExistingNetworkConstraint>,

    /// Pipe material string (canonical or alias). Stored as resolved canonical.
    #[serde(
        default = "default_material_str",
        deserialize_with = "deserialize_material_str"
    )]
    pub material: String,

    #[serde(default = "default_num_alternatives")]
    pub num_alternatives: u32,

    /// Norm code or alias — stored as-is; resolution happens in hydro-norms (S-22).
    #[serde(default = "default_norm", deserialize_with = "deserialize_norm_str")]
    pub norm: String,

    #[serde(default = "default_strict_norm")]
    pub strict_norm_compliance: bool,

    #[serde(default = "default_flow_per_service")]
    pub flow_per_service: f64,

    #[serde(default = "default_grid_resolution")]
    pub grid_resolution: f64,

    #[serde(default)]
    pub seed: Option<u64>,

    #[serde(default = "default_nsga_population_size")]
    pub nsga_population_size: u32,

    #[serde(default = "default_nsga_generations")]
    pub nsga_generations: u32,

    #[serde(default = "default_nsga_max_time_seconds")]
    pub nsga_max_time_seconds: u32,

    #[serde(default)]
    pub nsga_num_workers: Option<u32>,

    #[serde(default = "default_enforce_bool")]
    pub enforce_cover_depth: bool,

    #[serde(default = "default_enforce_bool")]
    pub enforce_existing_clearance: bool,

    #[serde(default = "default_enforce_bool")]
    pub enforce_segment_slopes: bool,

    #[serde(default = "default_enforce_bool")]
    pub enable_path_smoothing: bool,

    #[serde(default = "default_weight_cost")]
    pub weight_cost: f64,

    #[serde(default = "default_weight_excavation")]
    pub weight_excavation: f64,

    #[serde(default = "default_weight_pumping")]
    pub weight_pumping: f64,

    #[serde(default = "default_weight_interference")]
    pub weight_interference: f64,

    #[serde(default = "default_weight_resilience")]
    pub weight_resilience: f64,

    #[serde(default = "default_min_vertical_separation")]
    pub min_vertical_separation: f64,

    #[serde(default = "default_min_horizontal_separation")]
    pub min_horizontal_separation: f64,

    #[serde(default)]
    pub project_crs: Option<String>,
}

// ── ProjectTypeStr ────────────────────────────────────────────────────────────

/// Validated project type string (one of the six canonical snake_case values).
///
/// Stored as a string (not the enum) so that downstream crates that don't
/// depend on the enum can still deserialize DesignRequest. The enum is
/// available via `ProjectTypeStr::as_enum()`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(transparent)]
pub struct ProjectTypeStr(pub String);

impl ProjectTypeStr {
    const VALID: &'static [&'static str] = &[
        "sewer",
        "water_supply",
        "conveyance",
        "distribution",
        "pump_station",
        "intake",
    ];

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for ProjectTypeStr {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        let normalized = raw.trim().to_lowercase().replace([' ', '-'], "_");
        if Self::VALID.contains(&normalized.as_str()) {
            Ok(ProjectTypeStr(normalized))
        } else {
            Err(serde::de::Error::custom(format!(
                "Unknown project_type '{raw}'. Valid values: {}",
                Self::VALID.join(", ")
            )))
        }
    }
}

// ── Custom deserializers ──────────────────────────────────────────────────────

/// Deserialize material string with alias resolution.
fn deserialize_material_str<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<String, D::Error> {
    let s = String::deserialize(deserializer)?;
    PipeMaterial::from_str_with_aliases(&s)
        .map(|m| m.as_str().to_owned())
        .ok_or_else(|| serde::de::Error::custom(format!("Unknown pipe material: '{s}'")))
}

/// Deserialize and normalize norm string (uppercase, spaces→underscores).
/// Alias resolution is deferred to hydro-norms (S-22).
fn deserialize_norm_str<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<String, D::Error> {
    let s = String::deserialize(deserializer)?;
    let normalized = s.trim().to_uppercase().replace(['-', ' '], "_");
    Ok(normalized)
}

// ── Validation ────────────────────────────────────────────────────────────────

impl DesignRequest {
    /// Validate the request after deserialization.
    ///
    /// Checks: terrain_points >= 3, numeric ranges, cross-field constraints.
    pub fn validate(&self) -> Result<(), HydroTypesError> {
        // S-01: terrain_points required + min 3 points
        if self.terrain_points.len() < 3 {
            return Err(HydroTypesError::MissingRequired {
                field: "terrain_points",
            });
        }

        // Terrain points must all be finite
        for p in &self.terrain_points {
            if !p.x.is_finite() || !p.y.is_finite() || !p.z.is_finite() {
                return Err(HydroTypesError::OutOfRange {
                    field: "terrain_points",
                    value: f64::NAN,
                    min: f64::NEG_INFINITY,
                    max: f64::INFINITY,
                });
            }
        }

        // R-01: num_alternatives in 1..10
        if !(1..=10).contains(&self.num_alternatives) {
            return Err(HydroTypesError::OutOfRange {
                field: "num_alternatives",
                value: self.num_alternatives as f64,
                min: 1.0,
                max: 10.0,
            });
        }

        // R-01: flow_per_service in (0, 10]
        if self.flow_per_service <= 0.0 || self.flow_per_service > 10.0 {
            return Err(HydroTypesError::OutOfRange {
                field: "flow_per_service",
                value: self.flow_per_service,
                min: 0.0,
                max: 10.0,
            });
        }

        // R-01: grid_resolution in [1, 500]
        if self.grid_resolution < 1.0 || self.grid_resolution > 500.0 {
            return Err(HydroTypesError::OutOfRange {
                field: "grid_resolution",
                value: self.grid_resolution,
                min: 1.0,
                max: 500.0,
            });
        }

        // R-01: seed in 0..2_147_483_647 when present
        if let Some(seed) = self.seed {
            if seed > 2_147_483_647 {
                return Err(HydroTypesError::OutOfRange {
                    field: "seed",
                    value: seed as f64,
                    min: 0.0,
                    max: 2_147_483_647.0,
                });
            }
        }

        // R-01: nsga_population_size in [20, 500]
        if !(20..=500).contains(&self.nsga_population_size) {
            return Err(HydroTypesError::OutOfRange {
                field: "nsga_population_size",
                value: self.nsga_population_size as f64,
                min: 20.0,
                max: 500.0,
            });
        }

        // R-01: nsga_generations in [10, 500]
        if !(10..=500).contains(&self.nsga_generations) {
            return Err(HydroTypesError::OutOfRange {
                field: "nsga_generations",
                value: self.nsga_generations as f64,
                min: 10.0,
                max: 500.0,
            });
        }

        // R-01: nsga_max_time_seconds in [30, 7200]
        if !(30..=7200).contains(&self.nsga_max_time_seconds) {
            return Err(HydroTypesError::OutOfRange {
                field: "nsga_max_time_seconds",
                value: self.nsga_max_time_seconds as f64,
                min: 30.0,
                max: 7200.0,
            });
        }

        // R-01: nsga_num_workers in [1, 256] when present
        if let Some(w) = self.nsga_num_workers {
            if !(1..=256).contains(&w) {
                return Err(HydroTypesError::OutOfRange {
                    field: "nsga_num_workers",
                    value: w as f64,
                    min: 1.0,
                    max: 256.0,
                });
            }
        }

        // R-01: weight fields in [0, 1]
        for (field, val) in [
            ("weight_cost", self.weight_cost),
            ("weight_excavation", self.weight_excavation),
            ("weight_pumping", self.weight_pumping),
            ("weight_interference", self.weight_interference),
            ("weight_resilience", self.weight_resilience),
        ] {
            if !(0.0..=1.0).contains(&val) {
                return Err(HydroTypesError::OutOfRange {
                    field,
                    value: val,
                    min: 0.0,
                    max: 1.0,
                });
            }
        }

        // R-01: min_vertical_separation in [0, 10]
        if !(0.0..=10.0).contains(&self.min_vertical_separation) {
            return Err(HydroTypesError::OutOfRange {
                field: "min_vertical_separation",
                value: self.min_vertical_separation,
                min: 0.0,
                max: 10.0,
            });
        }

        // R-01: min_horizontal_separation in [0, 20]
        if !(0.0..=20.0).contains(&self.min_horizontal_separation) {
            return Err(HydroTypesError::OutOfRange {
                field: "min_horizontal_separation",
                value: self.min_horizontal_separation,
                min: 0.0,
                max: 20.0,
            });
        }

        // Validate constraint overrides if present
        if let Some(ref overrides) = self.constraints {
            let merged = overrides.apply_to(DesignConstraints::default());
            merged.validate()?;
        }

        // Validate polygon constraints
        for zone in &self.forbidden_zones {
            if zone.coords.len() < 3 {
                return Err(HydroTypesError::TooFewCoords {
                    got: zone.coords.len(),
                });
            }
            if let Some(buf) = zone.buffer {
                if !(0.0..=200.0).contains(&buf) {
                    return Err(HydroTypesError::OutOfRange {
                        field: "forbidden_zones.buffer",
                        value: buf,
                        min: 0.0,
                        max: 200.0,
                    });
                }
            }
        }

        // Validate mandatory route constraints
        for route in &self.mandatory_routes {
            if route.coords.len() < 2 {
                return Err(HydroTypesError::TooFewCoords {
                    got: route.coords.len(),
                });
            }
            if let Some(w) = route.corridor_width {
                if !(0.0..=500.0).contains(&w) {
                    return Err(HydroTypesError::OutOfRange {
                        field: "mandatory_routes.corridor_width",
                        value: w,
                        min: 0.0,
                        max: 500.0,
                    });
                }
            }
        }

        Ok(())
    }

    /// Resolve the effective `DesignConstraints` by merging request.constraints
    /// overrides on top of the library defaults.
    pub fn effective_constraints(&self) -> DesignConstraints {
        match &self.constraints {
            Some(overrides) => overrides.apply_to(DesignConstraints::default()),
            None => DesignConstraints::default(),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests (T-1.5 RED → GREEN)
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn minimal_request_json() -> serde_json::Value {
        json!({
            "project_type": "sewer",
            "terrain_points": [
                {"x": 0.0, "y": 0.0, "z": 10.0},
                {"x": 10.0, "y": 0.0, "z": 9.0},
                {"x": 10.0, "y": 10.0, "z": 8.5}
            ]
        })
    }

    // S-01: terrain_points required
    #[test]
    fn missing_terrain_points_fails_validation() {
        let req: DesignRequest = serde_json::from_value(json!({
            "project_type": "sewer",
            "terrain_points": [
                {"x": 0.0, "y": 0.0, "z": 10.0},
                {"x": 10.0, "y": 0.0, "z": 9.0}
            ]
        }))
        .unwrap();
        let err = req.validate().unwrap_err();
        match err {
            crate::error::HydroTypesError::MissingRequired { field } => {
                assert_eq!(field, "terrain_points");
            }
            other => panic!("expected MissingRequired, got {other:?}"),
        }
    }

    #[test]
    fn terrain_points_missing_from_json_is_a_deserialization_error() {
        // terrain_points has no default — must be present in JSON
        let result: Result<DesignRequest, _> = serde_json::from_value(json!({
            "project_type": "sewer"
        }));
        assert!(result.is_err(), "should fail when terrain_points is absent");
    }

    // S-02: unknown project_type rejected
    #[test]
    fn unknown_project_type_is_rejected() {
        let result: Result<DesignRequest, _> = serde_json::from_value(json!({
            "project_type": "aqueduct",
            "terrain_points": [
                {"x": 0.0, "y": 0.0, "z": 10.0},
                {"x": 10.0, "y": 0.0, "z": 9.0},
                {"x": 10.0, "y": 10.0, "z": 8.5}
            ]
        }));
        assert!(result.is_err());
    }

    // S-03: extra fields rejected
    #[test]
    fn extra_fields_are_rejected() {
        let result: Result<DesignRequest, _> = serde_json::from_value(json!({
            "project_type": "sewer",
            "terrain_points": [
                {"x": 0.0, "y": 0.0, "z": 10.0},
                {"x": 10.0, "y": 0.0, "z": 9.0},
                {"x": 10.0, "y": 10.0, "z": 8.5}
            ],
            "foo": 42
        }));
        assert!(result.is_err());
    }

    // S-04: constraint cross-field validation
    #[test]
    fn constraint_min_slope_gt_max_slope_fails_validation() {
        let req: DesignRequest = serde_json::from_value(json!({
            "project_type": "sewer",
            "terrain_points": [
                {"x": 0.0, "y": 0.0, "z": 10.0},
                {"x": 10.0, "y": 0.0, "z": 9.0},
                {"x": 10.0, "y": 10.0, "z": 8.5}
            ],
            "constraints": {"min_slope": 0.05, "max_slope": 0.01}
        }))
        .unwrap();
        assert!(req.validate().is_err());
    }

    // Material alias resolution (R-01)
    #[test]
    fn material_pead_alias_resolves_to_hdpe() {
        let req: DesignRequest = serde_json::from_value(json!({
            "project_type": "sewer",
            "terrain_points": [
                {"x": 0.0, "y": 0.0, "z": 10.0},
                {"x": 10.0, "y": 0.0, "z": 9.0},
                {"x": 10.0, "y": 10.0, "z": 8.5}
            ],
            "material": "PEAD"
        }))
        .unwrap();
        assert_eq!(req.material, "HDPE");
    }

    #[test]
    fn material_concreto_alias_resolves_to_concrete() {
        let req: DesignRequest = serde_json::from_value(json!({
            "project_type": "sewer",
            "terrain_points": [
                {"x": 0.0, "y": 0.0, "z": 10.0},
                {"x": 10.0, "y": 0.0, "z": 9.0},
                {"x": 10.0, "y": 10.0, "z": 8.5}
            ],
            "material": "CONCRETO"
        }))
        .unwrap();
        assert_eq!(req.material, "CONCRETE");
    }

    // Norm normalization (S-22)
    #[test]
    fn norm_conagua_is_normalized_to_uppercase() {
        let req: DesignRequest = serde_json::from_value(json!({
            "project_type": "sewer",
            "terrain_points": [
                {"x": 0.0, "y": 0.0, "z": 10.0},
                {"x": 10.0, "y": 0.0, "z": 9.0},
                {"x": 10.0, "y": 10.0, "z": 8.5}
            ],
            "norm": "CONAGUA"
        }))
        .unwrap();
        // stored as normalized uppercase
        assert_eq!(req.norm, "CONAGUA");
    }

    // Default values match spec R-01
    #[test]
    fn default_values_match_spec() {
        let req: DesignRequest = serde_json::from_value(minimal_request_json()).unwrap();
        assert_eq!(req.project_name, "Unnamed");
        assert_eq!(req.material, "PVC");
        assert_eq!(req.num_alternatives, 3);
        assert_eq!(req.norm, "CONAGUA_MX");
        assert!(req.strict_norm_compliance);
        assert!((req.flow_per_service - 0.002).abs() < 1e-12);
        assert!((req.grid_resolution - 20.0).abs() < 1e-12);
        assert_eq!(req.nsga_population_size, 100);
        assert_eq!(req.nsga_generations, 50);
        assert_eq!(req.nsga_max_time_seconds, 300);
        assert!(!req.enforce_cover_depth);
        assert!(!req.enforce_existing_clearance);
        assert!(!req.enforce_segment_slopes);
        assert!(!req.enable_path_smoothing);
        assert!((req.weight_cost - 0.30).abs() < 1e-12);
        assert!((req.weight_excavation - 0.25).abs() < 1e-12);
        assert!((req.weight_pumping - 0.15).abs() < 1e-12);
        assert!((req.weight_interference - 0.10).abs() < 1e-12);
        assert!((req.weight_resilience - 0.20).abs() < 1e-12);
        assert!((req.min_vertical_separation - 0.3).abs() < 1e-12);
        assert!((req.min_horizontal_separation - 1.0).abs() < 1e-12);
    }

    // Minimal valid request passes validation
    #[test]
    fn minimal_valid_request_passes_validation() {
        let req: DesignRequest = serde_json::from_value(minimal_request_json()).unwrap();
        assert!(req.validate().is_ok());
    }

    // effective_constraints() merges overrides on top of defaults
    #[test]
    fn effective_constraints_applies_overrides() {
        let req: DesignRequest = serde_json::from_value(json!({
            "project_type": "sewer",
            "terrain_points": [
                {"x": 0.0, "y": 0.0, "z": 10.0},
                {"x": 10.0, "y": 0.0, "z": 9.0},
                {"x": 10.0, "y": 10.0, "z": 8.5}
            ],
            "constraints": {"min_slope": 0.003, "max_depth": 7.0}
        }))
        .unwrap();
        let c = req.effective_constraints();
        assert!((c.min_slope - 0.003).abs() < 1e-12);
        assert!((c.max_depth - 7.0).abs() < 1e-12);
        // Non-overridden fields stay at defaults
        assert!((c.min_cover - 1.2).abs() < 1e-12);
    }
}
