//! Open channel flow for intake structures and canals.
//!
//! Reference: Manning equation for open channels (1891)
//!   Q = (1/n) * A * R^(2/3) * S^(1/2)
//!
//! Faithful port of `hydro_engine/hydraulics/open_channel.py`.

// ── RectangularChannelResult ──────────────────────────────────────────────────

/// Full hydraulic analysis of a rectangular open channel.
///
/// Mirrors the dict returned by Python `rectangular_channel_flow(width, depth, slope, roughness)`.
/// All fields carry the same rounding as the Python oracle.
#[derive(Debug, Clone)]
pub struct RectangularChannelResult {
    pub area_m2: f64,
    pub wetted_perimeter_m: f64,
    pub hydraulic_radius_m: f64,
    /// Velocity rounded to 3 decimal places (mirrors Python `round(vel, 3)`).
    pub velocity_m_s: f64,
    /// Flow rounded to 6 decimal places (mirrors Python `round(flow, 6)`).
    pub flow_m3_s: f64,
    /// Flow in L/s, rounded to 3 decimal places (mirrors Python `round(flow * 1000, 3)`).
    pub flow_lps: f64,
    /// Froude number, rounded to 3 decimal places.
    pub froude_number: f64,
    /// "subcritical" | "supercritical" | "critical"
    pub regime: &'static str,
}

/// Full flow analysis in a rectangular open channel, matching Python
/// `rectangular_channel_flow(width, depth, slope, roughness)` dict output.
///
/// Geometry:
///   A = b * y
///   P = b + 2y
///   R = A / P
///   V = (1/n) * R^(2/3) * |S|^(1/2)
///   Q = V * A
///   Fr = V / sqrt(g * y)  (rectangular: hydraulic depth = y)
///
/// # Arguments
///
/// * `width_m`     – Channel bottom width b (m).
/// * `depth_m`     – Flow depth y (m).
/// * `slope`       – Channel bed slope S (m/m, sign ignored via abs()).
/// * `roughness_n` – Manning's n coefficient.
///
/// # Returns
///
/// [`RectangularChannelResult`] with rounded fields matching the Python dict.
pub fn rectangular_channel_flow_full(
    width_m: f64,
    depth_m: f64,
    slope: f64,
    roughness_n: f64,
) -> RectangularChannelResult {
    let area = width_m * depth_m;
    let wetted_perimeter = width_m + 2.0 * depth_m;
    let hydraulic_radius = if wetted_perimeter > 0.0 {
        area / wetted_perimeter
    } else {
        0.0
    };
    let vel = (1.0 / roughness_n) * hydraulic_radius.powf(2.0 / 3.0) * slope.abs().sqrt();
    let flow = vel * area;
    let froude = if depth_m > 0.0 {
        vel / (GRAVITY * depth_m).sqrt()
    } else {
        0.0
    };

    let regime = if froude < 1.0 {
        "subcritical"
    } else if froude > 1.0 {
        "supercritical"
    } else {
        "critical"
    };

    RectangularChannelResult {
        area_m2: round4(area),
        wetted_perimeter_m: round4(wetted_perimeter),
        hydraulic_radius_m: round4(hydraulic_radius),
        velocity_m_s: round3(vel),
        flow_m3_s: round6(flow),
        flow_lps: round3(flow * 1000.0),
        froude_number: round3(froude),
        regime,
    }
}

// ── Rounding helpers (mirrors Python round(x, n)) ─────────────────────────────

/// Round to 3 decimal places.
#[inline]
fn round3(x: f64) -> f64 {
    (x * 1000.0).round() / 1000.0
}

/// Round to 4 decimal places.
#[inline]
fn round4(x: f64) -> f64 {
    (x * 10000.0).round() / 10000.0
}

/// Round to 6 decimal places.
#[inline]
fn round6(x: f64) -> f64 {
    (x * 1_000_000.0).round() / 1_000_000.0
}

// ── Gravitational acceleration ─────────────────────────────────────────────────

const GRAVITY: f64 = 9.81;

// ─────────────────────────────────────────────────────────────────────────────

/// Compute velocity in a rectangular open channel using Manning's equation.
///
/// Geometry:
///   A = b * y
///   P = b + 2y
///   R = A / P
///   V = (1/n) * R^(2/3) * |S|^(1/2)
///
/// Matches the `vel` computation inside Python `rectangular_channel_flow(width, depth, slope, roughness)`.
///
/// # Arguments
///
/// * `width_m`     – Channel bottom width b (m).
/// * `depth_m`     – Flow depth y (m).
/// * `slope`       – Channel bed slope S (m/m, sign ignored via abs()).
/// * `roughness_n` – Manning's n coefficient.
///
/// # Returns
///
/// Flow velocity V (m/s). Returns 0.0 when wetted perimeter is zero.
pub fn rectangular_channel_velocity(
    width_m: f64,
    depth_m: f64,
    slope: f64,
    roughness_n: f64,
) -> f64 {
    // Python: area = width * depth
    let area = width_m * depth_m;
    // Python: wetted_perimeter = width + 2 * depth
    let wetted_perimeter = width_m + 2.0 * depth_m;
    // Python: hydraulic_radius = area / wetted_perimeter if wetted_perimeter > 0 else 0
    let hydraulic_radius = if wetted_perimeter > 0.0 {
        area / wetted_perimeter
    } else {
        0.0
    };
    // Python: vel = (1.0 / roughness) * (hydraulic_radius ** (2.0 / 3.0)) * (abs(slope) ** 0.5)
    (1.0 / roughness_n) * hydraulic_radius.powf(2.0 / 3.0) * slope.abs().powf(0.5)
}

/// Compute flow rate in a rectangular open channel using Manning's equation.
///
/// Q = V * A = V * (width * depth)
///
/// Matches the `flow` computation inside Python `rectangular_channel_flow(width, depth, slope, roughness)`.
///
/// # Arguments
///
/// * `width_m`     – Channel bottom width b (m).
/// * `depth_m`     – Flow depth y (m).
/// * `slope`       – Channel bed slope S (m/m).
/// * `roughness_n` – Manning's n coefficient.
///
/// # Returns
///
/// Flow rate Q (m³/s).
pub fn rectangular_channel_flow(width_m: f64, depth_m: f64, slope: f64, roughness_n: f64) -> f64 {
    let vel = rectangular_channel_velocity(width_m, depth_m, slope, roughness_n);
    // Python: flow = vel * area
    let area = width_m * depth_m;
    vel * area
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn rectangular_channel_zero_depth_gives_zero_velocity() {
        let v = rectangular_channel_velocity(1.0, 0.0, 0.001, 0.013);
        assert_relative_eq!(v, 0.0, epsilon = 1e-12);
    }

    #[test]
    fn rectangular_channel_flow_equals_v_times_area() {
        let w = 2.0_f64;
        let d = 0.5_f64;
        let s = 0.002_f64;
        let n = 0.013_f64;
        let v = rectangular_channel_velocity(w, d, s, n);
        let area = w * d;
        let q_manual = v * area;
        let q = rectangular_channel_flow(w, d, s, n);
        assert_relative_eq!(q, q_manual, max_relative = 1e-14);
    }

    #[test]
    fn rectangular_channel_slope_sign_invariant() {
        let v_pos = rectangular_channel_velocity(1.5, 0.4, 0.005, 0.013);
        let v_neg = rectangular_channel_velocity(1.5, 0.4, -0.005, 0.013);
        assert_relative_eq!(v_pos, v_neg, max_relative = 1e-14);
    }

    #[test]
    fn rectangular_channel_physical_values() {
        // 1m-wide channel, 0.5m deep, 0.001 slope, n=0.013 (concrete)
        let v = rectangular_channel_velocity(1.0, 0.5, 0.001, 0.013);
        assert!(v > 0.1 && v < 5.0, "velocity out of physical range: {v}");
    }
}
