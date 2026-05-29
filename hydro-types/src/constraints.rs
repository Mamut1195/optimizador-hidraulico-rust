//! DesignConstraints — normative and user-defined design constraints.
//!
//! Mirrors `hydro_engine/core/constraints.py` DesignConstraints exactly.
//! All default values are pinned from the Python oracle (spec R-08, S-10, S-11).

use serde::{Deserialize, Serialize};

use crate::error::HydroTypesError;

/// Default available diameters (meters) — pinned from Python oracle (R-08).
fn default_available_diameters() -> Vec<f64> {
    vec![0.2, 0.25, 0.3, 0.38, 0.45, 0.61, 0.76, 0.91, 1.07, 1.22]
}

fn default_min_slope() -> f64 {
    0.005
}
fn default_max_slope() -> f64 {
    0.05
}
fn default_min_cover() -> f64 {
    1.2
}
fn default_max_depth() -> f64 {
    5.0
}
fn default_min_velocity() -> f64 {
    0.6
}
fn default_max_velocity() -> f64 {
    5.0
}
fn default_min_diameter() -> f64 {
    0.2
}
fn default_max_diameter() -> f64 {
    1.2
}
fn default_max_manhole_spacing() -> f64 {
    100.0
}
fn default_manhole_bool() -> bool {
    true
}
fn default_min_pressure() -> f64 {
    15.0
}
fn default_max_pressure() -> f64 {
    50.0
}
fn default_material() -> String {
    "PVC".to_owned()
}
fn default_roughness() -> f64 {
    0.009
}

/// Design constraints for a hydraulic network.
///
/// Combines normative requirements (e.g., CONAGUA) with user overrides.
/// All defaults are pinned from the Python oracle (spec R-08).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DesignConstraints {
    /// Minimum pipe slope (dimensionless). Default: 0.005.
    #[serde(default = "default_min_slope")]
    pub min_slope: f64,

    /// Maximum pipe slope (dimensionless). Default: 0.05.
    #[serde(default = "default_max_slope")]
    pub max_slope: f64,

    /// Minimum earth cover in meters. Default: 1.2.
    #[serde(default = "default_min_cover")]
    pub min_cover: f64,

    /// Maximum trench depth in meters. Default: 5.0.
    #[serde(default = "default_max_depth")]
    pub max_depth: f64,

    /// Minimum flow velocity in m/s (self-cleaning). Default: 0.6.
    #[serde(default = "default_min_velocity")]
    pub min_velocity: f64,

    /// Maximum flow velocity in m/s (erosion limit). Default: 5.0.
    #[serde(default = "default_max_velocity")]
    pub max_velocity: f64,

    /// Minimum pipe diameter in meters. Default: 0.2.
    #[serde(default = "default_min_diameter")]
    pub min_diameter: f64,

    /// Maximum pipe diameter in meters. Default: 1.2.
    #[serde(default = "default_max_diameter")]
    pub max_diameter: f64,

    /// Commercially available diameters in meters, sorted ascending.
    /// Default: [0.2, 0.25, 0.3, 0.38, 0.45, 0.61, 0.76, 0.91, 1.07, 1.22].
    #[serde(default = "default_available_diameters")]
    pub available_diameters: Vec<f64>,

    /// Maximum distance between manholes in meters. Default: 100.0.
    #[serde(default = "default_max_manhole_spacing")]
    pub max_manhole_spacing: f64,

    /// Require manhole at direction changes. Default: true.
    #[serde(default = "default_manhole_bool")]
    pub manhole_at_direction_change: bool,

    /// Require manhole at slope changes. Default: true.
    #[serde(default = "default_manhole_bool")]
    pub manhole_at_slope_change: bool,

    /// Require manhole at diameter changes. Default: true.
    #[serde(default = "default_manhole_bool")]
    pub manhole_at_diameter_change: bool,

    /// Minimum service pressure in mca (water supply). Default: 15.0.
    #[serde(default = "default_min_pressure")]
    pub min_pressure: f64,

    /// Maximum service pressure in mca (water supply). Default: 50.0.
    #[serde(default = "default_max_pressure")]
    pub max_pressure: f64,

    /// Default pipe material name. Default: "PVC".
    #[serde(default = "default_material")]
    pub default_material: String,

    /// Manning's n for the default material. Default: 0.009 (PVC).
    #[serde(default = "default_roughness")]
    pub default_roughness: f64,

    /// Maximum bend angle in degrees. None = no restriction.
    #[serde(default)]
    pub max_bend_angle: Option<f64>,
}

impl Default for DesignConstraints {
    fn default() -> Self {
        DesignConstraints {
            min_slope: default_min_slope(),
            max_slope: default_max_slope(),
            min_cover: default_min_cover(),
            max_depth: default_max_depth(),
            min_velocity: default_min_velocity(),
            max_velocity: default_max_velocity(),
            min_diameter: default_min_diameter(),
            max_diameter: default_max_diameter(),
            available_diameters: default_available_diameters(),
            max_manhole_spacing: default_max_manhole_spacing(),
            manhole_at_direction_change: true,
            manhole_at_slope_change: true,
            manhole_at_diameter_change: true,
            min_pressure: default_min_pressure(),
            max_pressure: default_max_pressure(),
            default_material: default_material(),
            default_roughness: default_roughness(),
            max_bend_angle: None,
        }
    }
}

impl DesignConstraints {
    /// Validate cross-field constraints (spec S-04, S-10, S-11).
    ///
    /// Returns `Err` if any cross-field rule is violated.
    pub fn validate(&self) -> Result<(), HydroTypesError> {
        if self.min_slope > self.max_slope {
            return Err(HydroTypesError::CrossFieldViolation {
                message: format!(
                    "min_slope ({}) cannot be greater than max_slope ({})",
                    self.min_slope, self.max_slope
                ),
            });
        }
        if self.min_velocity > self.max_velocity {
            return Err(HydroTypesError::CrossFieldViolation {
                message: format!(
                    "min_velocity ({}) cannot be greater than max_velocity ({})",
                    self.min_velocity, self.max_velocity
                ),
            });
        }
        if self.min_diameter > self.max_diameter {
            return Err(HydroTypesError::CrossFieldViolation {
                message: format!(
                    "min_diameter ({}) cannot be greater than max_diameter ({})",
                    self.min_diameter, self.max_diameter
                ),
            });
        }
        if self.min_pressure > self.max_pressure {
            return Err(HydroTypesError::CrossFieldViolation {
                message: format!(
                    "min_pressure ({}) cannot be greater than max_pressure ({})",
                    self.min_pressure, self.max_pressure
                ),
            });
        }
        if self.available_diameters.iter().any(|&d| d <= 0.0) {
            return Err(HydroTypesError::InvalidDiameter);
        }
        Ok(())
    }

    /// Sort `available_diameters` ascending (spec S-11: must be sorted).
    pub fn sort_diameters(&mut self) {
        self.available_diameters
            .sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    }

    /// Select the smallest commercially available diameter >= `required`.
    ///
    /// Mirrors `DesignConstraints.select_diameter()` from Python oracle.
    /// If `required` exceeds all available diameters, returns the largest.
    pub fn select_diameter(&self, required: f64) -> f64 {
        let mut sorted = self.available_diameters.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        for &d in &sorted {
            if d >= required {
                return d;
            }
        }
        // Fallback: return the largest available diameter (matches Python oracle).
        sorted.last().copied().unwrap_or(required)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests (T-1.2 RED → GREEN)
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // T-1.2: Default values match Python oracle (R-08)
    #[test]
    fn default_values_match_oracle() {
        let c = DesignConstraints::default();
        assert!((c.min_slope - 0.005).abs() < 1e-12);
        assert!((c.max_slope - 0.05).abs() < 1e-12);
        assert!((c.min_cover - 1.2).abs() < 1e-12);
        assert!((c.max_depth - 5.0).abs() < 1e-12);
        assert!((c.min_velocity - 0.6).abs() < 1e-12);
        assert!((c.max_velocity - 5.0).abs() < 1e-12);
        assert!((c.min_diameter - 0.2).abs() < 1e-12);
        assert!((c.max_diameter - 1.2).abs() < 1e-12);
        assert!((c.max_manhole_spacing - 100.0).abs() < 1e-12);
        assert!(c.manhole_at_direction_change);
        assert!(c.manhole_at_slope_change);
        assert!(c.manhole_at_diameter_change);
        assert!((c.min_pressure - 15.0).abs() < 1e-12);
        assert!((c.max_pressure - 50.0).abs() < 1e-12);
        assert_eq!(c.default_material, "PVC");
        assert!((c.default_roughness - 0.009).abs() < 1e-12);
        assert!(c.max_bend_angle.is_none());
    }

    #[test]
    fn default_available_diameters_match_oracle() {
        let expected = [0.2f64, 0.25, 0.3, 0.38, 0.45, 0.61, 0.76, 0.91, 1.07, 1.22];
        let c = DesignConstraints::default();
        assert_eq!(c.available_diameters.len(), expected.len());
        for (&a, &b) in c.available_diameters.iter().zip(expected.iter()) {
            assert!((a - b).abs() < 1e-12, "diameter mismatch: {a} vs {b}");
        }
    }

    // T-1.2: Serialization round-trip (S-10)
    #[test]
    fn design_constraints_json_roundtrip() {
        let c = DesignConstraints::default();
        let json = serde_json::to_string(&c).unwrap();
        let back: DesignConstraints = serde_json::from_str(&json).unwrap();
        assert_eq!(c, back);
    }

    // T-1.2: Cross-field validation (S-04)
    #[test]
    fn validate_min_slope_gt_max_slope_returns_err() {
        let c = DesignConstraints {
            min_slope: 0.05,
            max_slope: 0.01,
            ..Default::default()
        };
        assert!(c.validate().is_err());
    }

    #[test]
    fn validate_min_velocity_gt_max_velocity_returns_err() {
        let c = DesignConstraints {
            min_velocity: 6.0,
            max_velocity: 1.0,
            ..Default::default()
        };
        assert!(c.validate().is_err());
    }

    #[test]
    fn validate_min_diameter_gt_max_diameter_returns_err() {
        let c = DesignConstraints {
            min_diameter: 1.5,
            max_diameter: 0.5,
            ..Default::default()
        };
        assert!(c.validate().is_err());
    }

    #[test]
    fn validate_min_pressure_gt_max_pressure_returns_err() {
        let c = DesignConstraints {
            min_pressure: 60.0,
            max_pressure: 30.0,
            ..Default::default()
        };
        assert!(c.validate().is_err());
    }

    #[test]
    fn validate_default_constraints_are_valid() {
        let c = DesignConstraints::default();
        assert!(c.validate().is_ok());
    }

    // T-1.2: available_diameters sorted ascending (S-11)
    #[test]
    fn sort_diameters_produces_ascending_order() {
        let mut c = DesignConstraints {
            available_diameters: vec![1.2, 0.3, 0.5, 0.2],
            ..Default::default()
        };
        c.sort_diameters();
        let expected = [0.2_f64, 0.3, 0.5, 1.2];
        for (&a, &b) in c.available_diameters.iter().zip(expected.iter()) {
            assert!((a - b).abs() < 1e-12);
        }
    }

    // T-1.6: select_diameter mirrors Python oracle
    #[test]
    fn select_diameter_picks_smallest_gte_required() {
        let c = DesignConstraints::default();
        // required 0.25 -> should pick 0.25
        let d = c.select_diameter(0.25);
        assert!((d - 0.25).abs() < 1e-12, "got {d}");
        // required 0.26 -> should pick 0.3
        let d = c.select_diameter(0.26);
        assert!((d - 0.3).abs() < 1e-12, "got {d}");
    }

    #[test]
    fn select_diameter_returns_largest_when_required_exceeds_all() {
        let c = DesignConstraints::default();
        let d = c.select_diameter(2.0);
        // Python: returns available_diameters[-1] = 1.22
        assert!((d - 1.22).abs() < 1e-12, "got {d}");
    }

    #[test]
    fn select_diameter_works_with_unsorted_input() {
        let c = DesignConstraints {
            available_diameters: vec![0.61, 0.3, 0.91, 0.2],
            ..Default::default()
        };
        // required 0.4 -> sorted: 0.2,0.3,0.61,0.91 -> pick 0.61
        let d = c.select_diameter(0.4);
        assert!((d - 0.61).abs() < 1e-12, "got {d}");
    }
}
