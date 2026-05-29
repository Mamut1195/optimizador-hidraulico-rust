//! Pump curve construction and evaluation.
//!
//! Provides quadratic pump characteristic curves (Q-H), built from shutoff head
//! and best-efficiency-point flow, and head evaluation at arbitrary flow.
//!
//! Faithful port of `hydro_engine/hydraulics/pump_curves.py`.

/// Quadratic pump characteristic curve: H(Q) = a*Q² + b*Q + c.
///
/// Built from shutoff head and BEP flow via [`make_curve`].
#[derive(Debug, Clone, PartialEq)]
pub struct PumpCurve {
    /// Quadratic coefficient (typically negative).
    pub a: f64,
    /// Linear coefficient.
    pub b: f64,
    /// Shutoff head — H at Q = 0 (m).
    pub c: f64,
    /// Maximum flow at zero head (m³/s).
    pub q_max: f64,
}

/// Build a quadratic pump curve from shutoff head and BEP flow.
///
/// Assumes the pump produces zero head at Q_max = 1.5 * Q_bep.
///   H(Q) = c + b*Q + a*Q²   where c = shutoff_head
///
/// Matches Python `make_curve(shutoff_head, q_bep)` exactly, preserving the
/// same operation order and determinant computation.
///
/// # Arguments
///
/// * `shutoff_head_m` – Head at zero flow H₀ (m).
/// * `q_bep_m3s`      – Flow at best-efficiency-point Q_bep (m³/s).
///
/// # Returns
///
/// [`PumpCurve`] with coefficients (a, b, c, q_max).
///
/// When the determinant is below 1e-15, returns a degenerate flat curve
/// with a=0.0, b=0.0, c=shutoff_head (matching Python behaviour).
pub fn make_curve(shutoff_head_m: f64, q_bep_m3s: f64) -> PumpCurve {
    // Python: q_max = 1.5 * q_bep
    let q_max = 1.5 * q_bep_m3s;

    // Python: h_bep = 0.85 * shutoff_head
    let h_bep = 0.85 * shutoff_head_m;

    let c = shutoff_head_m;

    // Python:
    // det = q_bep**2 * q_max - q_max**2 * q_bep
    let det = q_bep_m3s * q_bep_m3s * q_max - q_max * q_max * q_bep_m3s;

    // Python: if abs(det) < 1e-15: return PumpCurve(a=0.0, b=0.0, c=c, q_max=q_max)
    if det.abs() < 1e-15 {
        return PumpCurve {
            a: 0.0,
            b: 0.0,
            c,
            q_max,
        };
    }

    // Python:
    // a = ((h_bep - c) * q_max - (-c) * q_bep) / det
    // b = (q_bep**2 * (-c) - q_max**2 * (h_bep - c)) / det
    let a = ((h_bep - c) * q_max - (-c) * q_bep_m3s) / det;
    let b = (q_bep_m3s * q_bep_m3s * (-c) - q_max * q_max * (h_bep - c)) / det;

    PumpCurve { a, b, c, q_max }
}

/// Evaluate pump head at a given flow.
///
/// H(Q) = a*Q² + b*Q + c, clipped to ≥ 0.
///
/// Matches Python `head_at_flow(curve, q)`.
///
/// # Arguments
///
/// * `curve` – [`PumpCurve`] coefficients.
/// * `q`     – Query flow Q (m³/s).
///
/// # Returns
///
/// Head H (m), clipped to ≥ 0.0.
pub fn head_at_flow(curve: &PumpCurve, q: f64) -> f64 {
    // Python: h = curve.a * np.asarray(q) ** 2 + curve.b * np.asarray(q) + curve.c
    let h = curve.a * q * q + curve.b * q + curve.c;
    // Python: return np.clip(h, 0.0, None)
    h.max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn make_curve_shutoff_head_at_q_zero() {
        // H(0) must equal the shutoff head exactly
        let curve = make_curve(30.0, 0.05);
        let h = head_at_flow(&curve, 0.0);
        assert_relative_eq!(h, 30.0, max_relative = 1e-12);
    }

    #[test]
    fn make_curve_head_at_bep_is_eighty_five_percent() {
        // H(Q_bep) ≈ 0.85 * shutoff_head (by construction)
        let shutoff = 25.0_f64;
        let q_bep = 0.035_f64;
        let curve = make_curve(shutoff, q_bep);
        let h_bep = head_at_flow(&curve, q_bep);
        assert_relative_eq!(h_bep, 0.85 * shutoff, max_relative = 1e-10);
    }

    #[test]
    fn make_curve_head_at_qmax_near_zero() {
        // H(Q_max) ≈ 0 (constructed to be exactly 0 before clip)
        let curve = make_curve(20.0, 0.02);
        let h_qmax = head_at_flow(&curve, curve.q_max);
        assert!(h_qmax.abs() < 1e-10, "H(Q_max) should be ≈ 0, got {h_qmax}");
    }

    #[test]
    fn head_at_flow_clipped_to_zero() {
        // Beyond Q_max, head would be negative; clip ensures ≥ 0
        let curve = make_curve(10.0, 0.005);
        let h_beyond = head_at_flow(&curve, 2.0 * curve.q_max);
        assert!(
            h_beyond >= 0.0,
            "head_at_flow must not return negative values"
        );
    }

    #[test]
    fn make_curve_degenerate_q_bep_zero() {
        // q_bep = 0 → det = 0 → degenerate flat curve
        let curve = make_curve(10.0, 0.0);
        assert_relative_eq!(curve.a, 0.0, epsilon = 1e-15);
        assert_relative_eq!(curve.b, 0.0, epsilon = 1e-15);
        assert_relative_eq!(curve.c, 10.0, epsilon = 1e-15);
    }
}
