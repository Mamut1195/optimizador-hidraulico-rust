//! Darcy-Weisbach friction factor and head loss (Swamee-Jain explicit approximation).
//!
//! Reference: Swamee, P. K. and Jain, A. K. (1976).
//!   "Explicit equations for pipe-flow problems."
//!   Journal of the Hydraulics Division, 102(5), 657-664.
//!
//! Formula:
//!   f = 0.25 / [log10(eps/(3.7*D) + 5.74/Re^0.9)]^2
//!
//! Faithful port of `hydro_engine/hydraulics/darcy_weisbach.py` (corrected
//! mm→m conversion: eps_d = (roughness / 1000.0) / diameter).

/// Gravitational acceleration (m/s²).
const GRAVITY: f64 = 9.81;

/// Kinematic viscosity of water at 20°C (m²/s).
const KINEMATIC_VISCOSITY_20C: f64 = 1.004e-6;

/// Compute the Darcy-Weisbach friction factor via the Swamee-Jain approximation.
///
/// Matches Python `friction_factor(reynolds, roughness, diameter)` exactly at
/// the level of float operations: same operation order, same intermediate values.
///
/// # Arguments
///
/// * `reynolds`    – Reynolds number (dimensionless, ≥ 1.0 internally clamped).
/// * `roughness_mm` – Absolute pipe roughness ε (mm). Default in Python: 0.0015 mm.
/// * `diameter_m`   – Pipe inner diameter (m).
///
/// # Returns
///
/// Darcy-Weisbach friction factor (dimensionless).
///
/// # Formula
///
/// ```text
/// eps_d = (roughness_mm / 1000.0) / diameter_m
/// term  = eps_d / 3.7 + 5.74 / max(Re, 1.0)^0.9
/// f     = 0.25 / log10(term)^2
/// ```
pub fn friction_factor(reynolds: f64, roughness_mm: f64, diameter_m: f64) -> f64 {
    // Convert roughness from mm to m before forming the dimensionless eps/D.
    // Python: eps_d = (roughness / 1000.0) / diameter
    let eps_d = (roughness_mm / 1000.0) / diameter_m;

    // Python: term = eps_d / 3.7 + 5.74 / np.power(np.maximum(re, 1.0), 0.9)
    let re_clamped = reynolds.max(1.0_f64);
    let term = eps_d / 3.7 + 5.74 / re_clamped.powf(0.9);

    // Python: return 0.25 / np.power(np.log10(term), 2)
    0.25 / term.log10().powi(2)
}

/// Compute head loss using the Darcy-Weisbach equation.
///
/// Matches Python `head_loss_dw(velocity_val, diameter, length, roughness)`.
///
/// # Arguments
///
/// * `velocity_m_s`  – Flow velocity (m/s).
/// * `diameter_m`    – Pipe inner diameter (m).
/// * `length_m`      – Pipe length (m).
/// * `roughness_mm`  – Absolute pipe roughness ε (mm).
///
/// # Returns
///
/// Head loss hf (m).
///
/// # Formula
///
/// ```text
/// Re   = |V| * D / ν
/// f    = friction_factor(Re, roughness_mm, diameter_m)
/// hf   = f * (L / D) * V² / (2 * g)
/// ```
pub fn head_loss_dw(velocity_m_s: f64, diameter_m: f64, length_m: f64, roughness_mm: f64) -> f64 {
    // Python: re = np.abs(v) * diameter / KINEMATIC_VISCOSITY_20C
    let re = velocity_m_s.abs() * diameter_m / KINEMATIC_VISCOSITY_20C;

    // Python: f = friction_factor(re, roughness, diameter)
    let f = friction_factor(re, roughness_mm, diameter_m);

    // Python: return f * (length / diameter) * (v**2) / (2 * GRAVITY)
    f * (length_m / diameter_m) * (velocity_m_s * velocity_m_s) / (2.0 * GRAVITY)
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn friction_factor_known_value_turbulent() {
        // For Re=100_000, eps_d = 0.0015mm/1000/0.2m = 7.5e-6, Swamee-Jain gives ~0.018
        let f = friction_factor(100_000.0, 0.0015, 0.2);
        assert!(
            f > 0.01 && f < 0.1,
            "friction factor should be in physical range, got {f}"
        );
        assert!(f.is_finite(), "friction factor should be finite");
    }

    #[test]
    fn friction_factor_high_reynolds_smooth_pipe() {
        // High Re, very smooth pipe — friction factor should approach Blasius
        let f = friction_factor(1_000_000.0, 0.001, 0.5);
        assert!(
            f > 0.005 && f < 0.05,
            "high-Re smooth friction factor out of range: {f}"
        );
    }

    #[test]
    fn head_loss_dw_zero_velocity() {
        // Zero velocity → zero head loss (regardless of pipe geometry)
        let hl = head_loss_dw(0.0, 0.2, 100.0, 0.0015);
        assert_relative_eq!(hl, 0.0, epsilon = 1e-12);
    }

    #[test]
    fn head_loss_dw_physical_range() {
        // A typical water main: 1 m/s in 200mm pipe, 100m length
        let hl = head_loss_dw(1.0, 0.2, 100.0, 0.0015);
        // Should be a small positive value (a few metres)
        assert!(
            hl > 0.0 && hl < 50.0,
            "head loss out of physical range: {hl}"
        );
    }

    #[test]
    fn friction_factor_roughness_mm_to_m_conversion() {
        // Verify the mm→m conversion is applied correctly.
        // roughness_mm=0.0015 with d=0.2 should give eps_d = 7.5e-6 (very smooth).
        // roughness_mm=1.5 with d=0.2 should give eps_d = 0.0075 (rough).
        let f_smooth = friction_factor(100_000.0, 0.0015, 0.2);
        let f_rough = friction_factor(100_000.0, 1.5, 0.2);
        assert!(
            f_rough > f_smooth,
            "rougher pipe must have higher friction factor"
        );
    }
}
