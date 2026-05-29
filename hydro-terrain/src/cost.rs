//! CostFunction — parametrizable cost for terrain graph edges.
//!
//! Mirrors Python `hydro_engine.core.cost.CostFunction` exactly.
//! Formula: J = w_length*L + w_excavation*avg_depth*L + slope_terms
//! where avg_depth = min_cover + 0.1.

/// Weights for the multi-criteria edge cost function.
///
/// Mirrors Python `CostWeights` dataclass with the same defaults.
#[derive(Debug, Clone, PartialEq)]
pub struct CostWeights {
    /// Weight for pipe length component.
    pub w_length: f64,
    /// Weight for excavation depth component.
    pub w_excavation: f64,
    /// Weight for slope violations (too flat or too steep).
    pub w_slope_penalty: f64,
    /// Weight for needing a pump (uphill gravity segment).
    pub w_pump_penalty: f64,
    /// Weight for crossing obstacles (unused in add_edge; reserved).
    pub w_crossing: f64,
}

impl Default for CostWeights {
    fn default() -> Self {
        Self {
            w_length: 1.0,
            w_excavation: 2.0,
            w_slope_penalty: 5.0,
            w_pump_penalty: 50.0,
            w_crossing: 10.0,
        }
    }
}

/// Computes edge cost for potential pipe route segments.
///
/// Mirrors Python `CostFunction.__call__` exactly:
/// - excavation cost: `w_excavation * (min_cover + 0.1) * length`
/// - uphill → pump penalty
/// - too flat → slope penalty
/// - too steep → slope penalty
#[derive(Debug, Clone)]
pub struct CostFunction {
    weights: CostWeights,
    min_slope: f64,
    max_slope: f64,
    min_cover: f64,
}

impl Default for CostFunction {
    fn default() -> Self {
        Self {
            weights: CostWeights::default(),
            min_slope: 0.005,
            max_slope: 0.05,
            min_cover: 1.2,
        }
    }
}

impl CostFunction {
    /// Create a new `CostFunction` with custom weights and slope bounds.
    pub fn new(weights: CostWeights, min_slope: f64, max_slope: f64, min_cover: f64) -> Self {
        Self {
            weights,
            min_slope,
            max_slope,
            min_cover,
        }
    }

    /// Compute cost for a single edge.
    ///
    /// # Arguments
    ///
    /// * `length` - Horizontal distance between nodes (m).
    /// * `z_start` - Ground elevation at start node.
    /// * `z_end` - Ground elevation at end node.
    ///
    /// # Returns
    ///
    /// Weighted cost scalar (lower is better for pathfinding).
    pub fn call(&self, length: f64, z_start: f64, z_end: f64) -> f64 {
        let w = &self.weights;
        let mut cost = w.w_length * length;

        // Excavation cost — mirrors Python: avg_depth = min_cover + 0.1
        let avg_depth = self.min_cover + 0.1;
        cost += w.w_excavation * avg_depth * length;

        // Slope analysis (only meaningful when length > 0)
        if length > 0.0 {
            let natural_slope = (z_start - z_end) / length;

            if natural_slope < 0.0 {
                // Uphill — gravity system cannot flow uphill
                cost += w.w_pump_penalty * natural_slope.abs() * length;
            } else if natural_slope < self.min_slope {
                // Too flat — won't self-clean
                cost += w.w_slope_penalty * (self.min_slope - natural_slope) * length;
            } else if natural_slope > self.max_slope {
                // Too steep — erosion risk
                cost += w.w_slope_penalty * (natural_slope - self.max_slope) * length;
            }
        }

        cost
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flat_segment_has_length_plus_excavation_plus_slope_penalty() {
        let cf = CostFunction::default();
        // length=100, z_start=z_end → natural_slope=0 → too flat (< min_slope=0.005)
        // cost = 1.0*100 + 2.0*1.3*100 + 5.0*(0.005-0)*100 = 100 + 260 + 2.5 = 362.5
        let cost = cf.call(100.0, 10.0, 10.0);
        assert!((cost - 362.5).abs() < 1e-9, "expected 362.5, got {cost}");
    }

    #[test]
    fn optimal_slope_has_no_penalty() {
        let cf = CostFunction::default();
        // natural_slope = (z_start - z_end) / length = (10.0 - 9.7) / 100 = 0.003? no
        // let's use slope exactly at 0.01 (between 0.005 and 0.05)
        // z_start=10, z_end=10 - 100*0.01 = 9.0 → slope = 0.01 ✓
        let cost = cf.call(100.0, 10.0, 9.0);
        // cost = 1.0*100 + 2.0*1.3*100 + 0 (slope in [0.005, 0.05]) = 100 + 260 = 360
        assert!((cost - 360.0).abs() < 1e-9, "expected 360.0, got {cost}");
    }

    #[test]
    fn uphill_adds_pump_penalty() {
        let cf = CostFunction::default();
        // natural_slope = (10 - 11) / 100 = -0.01 → pump_penalty
        // cost = 1.0*100 + 2.0*1.3*100 + 50.0*0.01*100 = 100 + 260 + 50 = 410
        let cost = cf.call(100.0, 10.0, 11.0);
        assert!((cost - 410.0).abs() < 1e-9, "expected 410.0, got {cost}");
    }
}
