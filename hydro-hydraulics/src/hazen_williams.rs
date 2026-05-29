//! Hazen-Williams equation for pressurized pipe flow.
//!
//! Reference: Hazen-Williams (1906)
//!   V = Q / A = Q / (π D² / 4)
//!   hf = 10.67 * |Q|^1.852 / (C^1.852 * D^4.87) * L
//!
//! Faithful port of `hydro_engine/hydraulics/hazen_williams.py`.

use std::f64::consts::PI;

/// Compute flow velocity in a full circular pipe.
///
/// Matches Python `velocity(flow, diameter)`:
///   area = π * (d / 2.0)² → V = Q / area (0.0 when area = 0)
///
/// # Arguments
///
/// * `flow_m3s`  – Flow rate Q (m³/s).
/// * `diameter_m` – Pipe inner diameter D (m).
///
/// # Returns
///
/// Velocity V (m/s). Returns 0.0 when diameter is zero.
pub fn velocity(flow_m3s: f64, diameter_m: f64) -> f64 {
    // Python: area = np.pi * (d / 2.0) ** 2
    let area = PI * (diameter_m / 2.0) * (diameter_m / 2.0);
    // Python: return np.where(area > 0, q / area, 0.0)
    if area > 0.0 {
        flow_m3s / area
    } else {
        0.0
    }
}

/// Compute friction head loss using the Hazen-Williams formula (SI units).
///
/// Matches Python `head_loss(flow, diameter, length, c)`:
///   hf = 10.67 * |Q|^1.852 / (C^1.852 * D^4.87) * L
///
/// # Arguments
///
/// * `flow_m3s`  – Flow rate Q (m³/s).
/// * `diameter_m` – Pipe inner diameter D (m).
/// * `length_m`   – Pipe length L (m).
/// * `c`          – Hazen-Williams coefficient (dimensionless).
///
/// # Returns
///
/// Head loss hf (m).
pub fn head_loss(flow_m3s: f64, diameter_m: f64, length_m: f64, c: f64) -> f64 {
    // Python: return 10.67 * np.power(np.abs(q), 1.852) / (np.power(c, 1.852) * np.power(d, 4.87)) * el
    10.67 * flow_m3s.abs().powf(1.852) / (c.powf(1.852) * diameter_m.powf(4.87)) * length_m
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn velocity_zero_diameter_returns_zero() {
        assert_relative_eq!(velocity(0.1, 0.0), 0.0, epsilon = 1e-15);
    }

    #[test]
    fn velocity_known_case() {
        // Q=0.01 m³/s, D=0.2 m → A = π*(0.1)² ≈ 0.031416 → V ≈ 0.3183
        let v = velocity(0.01, 0.2);
        let expected = 0.01 / (PI * 0.01);
        assert_relative_eq!(v, expected, max_relative = 1e-12);
    }

    #[test]
    fn head_loss_positive_for_positive_flow() {
        let hl = head_loss(0.01, 0.2, 100.0, 150.0);
        assert!(hl > 0.0, "head loss must be positive for positive flow");
    }

    #[test]
    fn head_loss_symmetric_in_flow_sign() {
        // |Q|^1.852 is symmetric, so hf(Q) = hf(-Q)
        let hl_pos = head_loss(0.01, 0.2, 100.0, 150.0);
        let hl_neg = head_loss(-0.01, 0.2, 100.0, 150.0);
        assert_relative_eq!(hl_pos, hl_neg, max_relative = 1e-14);
    }
}
