//! Manning equation for gravity flow in circular pipes.
//!
//! Reference: Manning's equation (1891)
//!   Q = (1/n) * A * R^(2/3) * S^(1/2)
//!   V = (1/n) * R^(2/3) * S^(1/2)
//!
//! Faithful port of `hydro_engine/hydraulics/manning.py`.

use std::f64::consts::PI;

/// Compute full-pipe flow velocity using Manning's equation.
///
/// For a full circular pipe the hydraulic radius R = D/4:
///   V = (1/n) * (D/4)^(2/3) * |S|^(1/2)
///
/// Matches Python `full_flow_velocity(diameter, slope, roughness)`.
///
/// # Arguments
///
/// * `diameter_m`  – Pipe inner diameter D (m).
/// * `slope`       – Pipe bed slope S (m/m, sign ignored — absolute value taken).
/// * `roughness_n` – Manning's roughness coefficient n.
///
/// # Returns
///
/// Full-pipe velocity V (m/s).
pub fn full_flow_velocity(diameter_m: f64, slope: f64, roughness_n: f64) -> f64 {
    // Python: r = d / 4.0
    let r = diameter_m / 4.0;
    // Python: return (1.0 / roughness) * np.power(r, 2.0 / 3.0) * np.sqrt(s)
    // where s = np.asarray(np.abs(slope), ...)
    let s = slope.abs();
    (1.0 / roughness_n) * r.powf(2.0 / 3.0) * s.sqrt()
}

/// Compute full-pipe flow capacity using Manning's equation.
///
/// Q = V * A = V * π * (D/2)²
///
/// Matches Python `full_flow_capacity(diameter, slope, roughness)`.
///
/// # Arguments
///
/// * `diameter_m`  – Pipe inner diameter D (m).
/// * `slope`       – Pipe bed slope S (m/m).
/// * `roughness_n` – Manning's roughness coefficient n.
///
/// # Returns
///
/// Full-pipe flow capacity Q (m³/s).
pub fn full_flow_capacity(diameter_m: f64, slope: f64, roughness_n: f64) -> f64 {
    let v = full_flow_velocity(diameter_m, slope, roughness_n);
    // Python: area = np.pi * (d / 2.0) ** 2
    let area = PI * (diameter_m / 2.0) * (diameter_m / 2.0);
    // Python: return v * area
    v * area
}

/// Compute hydraulic element ratios for partial flow in a circular pipe.
///
/// Given y/D (depth-to-diameter ratio), returns the geometric ratios via the
/// trigonometric (arccos) solution for a circular cross-section.
///
/// Reference: Chow, V. T. (1959). Open-Channel Hydraulics, Table 2-1.
///
/// The central angle θ satisfies:
///   y/D = (1 - cos(θ/2)) / 2
///   → θ = 2 * arccos(1 - 2 * (y/D))
///
/// Matches Python `partial_flow_ratio(y_over_d)`.
///
/// # Arguments
///
/// * `y_over_d` – Depth-to-diameter ratio (clamped to [0.001, 0.999]).
///
/// # Returns
///
/// `(area_ratio, radius_ratio, flow_ratio)` where:
/// - `area_ratio`   = A_partial / A_full
/// - `radius_ratio` = R_partial / R_full
/// - `flow_ratio`   = Q_partial / Q_full
pub fn partial_flow_ratio(y_over_d: f64) -> (f64, f64, f64) {
    // Python: yd = np.clip(np.asarray(y_over_d, dtype=np.float64), 0.001, 0.999)
    let yd = y_over_d.clamp(0.001, 0.999);

    // Python: theta = 2.0 * np.arccos(1.0 - 2.0 * yd)
    let theta = 2.0 * (1.0 - 2.0 * yd).acos();

    // Python: area_ratio = (theta - np.sin(theta)) / (2.0 * np.pi)
    let area_ratio = (theta - theta.sin()) / (2.0 * PI);

    // Python: perimeter_ratio = theta / (2.0 * np.pi)
    let perimeter_ratio = theta / (2.0 * PI);

    // Python: radius_ratio = np.where(perimeter_ratio > 0, area_ratio / perimeter_ratio, 0.0)
    let radius_ratio = if perimeter_ratio > 0.0 {
        area_ratio / perimeter_ratio
    } else {
        0.0
    };

    // Python: flow_ratio = area_ratio * np.power(radius_ratio, 2.0 / 3.0)
    let flow_ratio = area_ratio * radius_ratio.powf(2.0 / 3.0);

    (area_ratio, radius_ratio, flow_ratio)
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn full_flow_velocity_known_case() {
        // 200mm PVC pipe at 0.5% slope, n=0.009
        // V = (1/0.009) * (0.05)^(2/3) * sqrt(0.005) ≈ 0.756
        let v = full_flow_velocity(0.2, 0.005, 0.009);
        assert!(v > 0.5 && v < 1.5, "velocity out of expected range: {v}");
    }

    #[test]
    fn full_flow_velocity_slope_sign_invariant() {
        // Absolute value of slope is taken
        let v_pos = full_flow_velocity(0.3, 0.01, 0.009);
        let v_neg = full_flow_velocity(0.3, -0.01, 0.009);
        assert_relative_eq!(v_pos, v_neg, max_relative = 1e-14);
    }

    #[test]
    fn full_flow_capacity_is_velocity_times_area() {
        let d = 0.3_f64;
        let s = 0.005_f64;
        let n = 0.009_f64;
        let v = full_flow_velocity(d, s, n);
        let a = PI * (d / 2.0).powi(2);
        let q_manual = v * a;
        let q = full_flow_capacity(d, s, n);
        assert_relative_eq!(q, q_manual, max_relative = 1e-14);
    }

    #[test]
    fn partial_flow_ratio_half_full() {
        // y/D = 0.5 → area_ratio ≈ 0.5 (exact for semicircle)
        let (area_ratio, radius_ratio, flow_ratio) = partial_flow_ratio(0.5);
        assert!(
            area_ratio > 0.49 && area_ratio < 0.51,
            "area_ratio at y/D=0.5: {area_ratio}"
        );
        assert!(radius_ratio > 0.0, "radius_ratio must be positive");
        assert!(flow_ratio > 0.0, "flow_ratio must be positive");
    }

    #[test]
    fn partial_flow_ratio_clamps_extremes() {
        // Values at extremes should not panic or produce NaN/Inf
        let (ar_lo, rr_lo, fr_lo) = partial_flow_ratio(0.0);
        let (ar_hi, rr_hi, fr_hi) = partial_flow_ratio(1.0);
        assert!(ar_lo.is_finite() && ar_hi.is_finite());
        assert!(rr_lo.is_finite() && rr_hi.is_finite());
        assert!(fr_lo.is_finite() && fr_hi.is_finite());
    }

    #[test]
    fn partial_flow_ratio_monotonic_in_depth() {
        // Deeper fill → more flow
        let (_, _, fr1) = partial_flow_ratio(0.3);
        let (_, _, fr2) = partial_flow_ratio(0.6);
        assert!(fr2 > fr1, "flow_ratio should increase with depth");
    }
}
