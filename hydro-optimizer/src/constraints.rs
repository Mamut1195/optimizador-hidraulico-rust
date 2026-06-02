//! Constraint checker for the NSGA-III optimizer (REQ-004).
//!
//! Faithful port of Python oracle `constraints.py` (`ConstraintChecker`).
//!
//! # Spec penalty formula (REQ-004, tasks #578)
//! penalty = sum over violations of (1e12 + violation_magnitude * 1e9)
//!
//! NOTE: The explore §4 formula `(1e12 + dist) * 5` was found inconsistent
//! with the spec. The spec formula is authoritative per tasks #578.
//!
//! # Design §12 determinism rule
//! No `HashMap`; geometry uses `Vec<(f64,f64)>` polylines.
//!
//! Many items in this module are defined for Phase 7 use (full geometric
//! constraint evaluation against solver-produced networks). They are
//! exercised in the inline test suite below.
#![allow(dead_code)]

use std::f64::consts::PI;

use hydro_types::network::PipeNetwork;
use hydro_types::response::Solution;

// ── ConstraintViolationKind ───────────────────────────────────────────────────

/// Category of a constraint violation. Mirrors oracle constraint types.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConstraintViolationKind {
    /// Gene value outside its lower/upper bounds.
    Bounds,
    /// Pipe route passes through a forbidden zone polygon.
    ForbiddenZone,
    /// Required mandatory route corridor is not traversed.
    MandatoryRoute,
    /// Consecutive pipe bend angle violates the max_bend_angle limit.
    BendAngle,
    /// Pipe cover depth below min_cover (enforce_cover_depth=true).
    CoverDepth,
    /// Pipe trench too deep (enforce_max_depth=true).
    MaxDepth,
    /// Insufficient clearance from an existing network.
    ExistingClearance,
    /// Pipe slope outside [min_slope, max_slope] (enforce_slopes=true).
    Slope,
}

// ── ConstraintViolation ───────────────────────────────────────────────────────

/// A single constraint violation with its scalar magnitude.
///
/// `magnitude` is the scalar distance from the feasible region for this
/// specific violation (always ≥ 0). Used in the spec penalty formula.
#[derive(Debug, Clone)]
pub struct ConstraintViolation {
    /// Category of the violation.
    pub kind: ConstraintViolationKind,
    /// Scalar distance from the feasible region (≥ 0).
    pub magnitude: f64,
    /// Human-readable description (mirrors oracle violation strings).
    pub description: String,
}

// ── ConstraintReport ──────────────────────────────────────────────────────────

/// Output of a constraint check.
///
/// - `feasible`: true iff all constraints pass.
/// - `violations`: list of individual violations with their magnitudes.
/// - `penalty`: aggregate penalty per spec formula:
///   `sum_i(1e12 + violation_i.magnitude * 1e9)`
///
/// When `feasible == true`, `violations` is empty and `penalty == 0.0`.
#[derive(Debug, Clone)]
pub struct ConstraintReport {
    /// True iff all checked constraints are satisfied.
    pub feasible: bool,
    /// Per-violation details (empty when feasible).
    pub violations: Vec<ConstraintViolation>,
    /// Aggregate penalty (0.0 when feasible).
    pub penalty: f64,
}

impl ConstraintReport {
    /// Build a feasible (no-violation) report.
    fn feasible() -> Self {
        ConstraintReport {
            feasible: true,
            violations: vec![],
            penalty: 0.0,
        }
    }

    /// Build an infeasible report from a list of violations.
    fn infeasible(violations: Vec<ConstraintViolation>) -> Self {
        let penalty: f64 = violations.iter().map(|v| 1e12 + v.magnitude * 1e9).sum();
        ConstraintReport {
            feasible: false,
            violations,
            penalty,
        }
    }
}

// ── ExistingNetworkEntry ──────────────────────────────────────────────────────

/// Descriptor for an existing underground network used in clearance checks.
///
/// Mirrors oracle `existing_networks` list entries (constraints.py line 113).
#[derive(Debug, Clone)]
pub struct ExistingNetworkEntry {
    /// Polyline vertices (≥2 points).
    pub coords: Vec<(f64, f64)>,
    /// Burial depth of the existing network (m). Default 1.5 m.
    pub depth: f64,
    /// Per-entity horizontal setback override (m). `None` = use global.
    pub setback: Option<f64>,
}

// ── DesignConstraints ─────────────────────────────────────────────────────────

/// Design limits for cover depth and slope checks.
///
/// Mirrors `hydro_engine.core.constraints.DesignConstraints`.
#[derive(Debug, Clone)]
pub struct DesignConstraints {
    /// Minimum cover depth (m). Oracle default: 0.9 m.
    pub min_cover: f64,
    /// Maximum trench depth (m). Oracle default: 6.0 m.
    pub max_depth: f64,
    /// Minimum downhill slope (dimensionless). Oracle default: 0.003.
    pub min_slope: f64,
    /// Maximum downhill slope (dimensionless). Oracle default: 0.15.
    pub max_slope: f64,
}

impl Default for DesignConstraints {
    fn default() -> Self {
        DesignConstraints {
            min_cover: 0.9,
            max_depth: 6.0,
            min_slope: 0.003,
            max_slope: 0.15,
        }
    }
}

// ── ConstraintChecker ─────────────────────────────────────────────────────────

/// Checks hard constraints against an individual and / or a decoded solution.
///
/// Faithful port of oracle `ConstraintChecker` (constraints.py).
///
/// # Constraint layers
/// 1. **Gene-level bounds** (fast, pre-decode): `check_bounds(&genes)`.
/// 2. **Route-level forbidden zones**: `check_route(&coords)`.
/// 3. **Solution-level geometric**: `check_solution(&solution)`.
/// 4. **Combined**: `check_full(&genes, &solution)` — bounds first, then
///    geometric only when bounds pass (mirrors oracle short-circuit).
///
/// # Penalty formula (spec REQ-004)
/// `penalty = Σ_i (1e12 + violation_i.magnitude × 1e9)`
pub struct ConstraintChecker {
    // ── Bounds (gene-level) ───────────────────────────────────────────────
    /// Lower bound per gene. Empty = unchecked.
    lower: Vec<f64>,
    /// Upper bound per gene. Empty = unchecked.
    upper: Vec<f64>,

    // ── Geometric constraints ─────────────────────────────────────────────
    /// Forbidden zone polygons (each ≥3 vertices).
    forbidden: Vec<Vec<(f64, f64)>>,
    /// Mandatory route corridors.
    mandatory: Vec<MandatoryRouteSpec>,

    // ── Bend angle ────────────────────────────────────────────────────────
    /// Maximum allowed bend angle (deg). `None` = unchecked.
    max_bend_angle: Option<f64>,

    // ── Cover depth / trench depth ────────────────────────────────────────
    /// Design constraints for cover + slope. `None` = unchecked.
    design: Option<DesignConstraints>,
    /// Enforce minimum cover depth. Oracle default: true.
    enforce_cover_depth: bool,
    /// Enforce maximum trench depth. Oracle default: false.
    enforce_max_depth: bool,

    // ── Existing network clearance ────────────────────────────────────────
    existing: Vec<ExistingNetworkEntry>,
    /// Minimum vertical separation at crossings (m). Oracle default: 0.3.
    min_vertical_separation: f64,
    /// Minimum horizontal separation for parallel runs (m). Oracle default: 1.0.
    min_horizontal_separation: f64,
    /// Enforce clearance. Oracle default: false.
    enforce_existing_clearance: bool,

    // ── Slope ─────────────────────────────────────────────────────────────
    /// Enforce slope bounds. Oracle default: false.
    enforce_segment_slopes: bool,
}

// Internal mandatory route spec
struct MandatoryRouteSpec {
    /// Centre-line polyline.
    coords: Vec<(f64, f64)>,
    /// Half-width of the corridor (m). 0 = proximity check only.
    corridor_half_width: f64,
}

impl ConstraintChecker {
    // ── Constructors (test-facing) ────────────────────────────────────────

    /// Bounds-only checker (no geometric constraints).
    pub fn new_bounds_only(lower: Vec<f64>, upper: Vec<f64>) -> Self {
        ConstraintChecker::new_raw(
            lower,
            upper,
            vec![],
            vec![],
            None,
            None,
            false,
            false,
            vec![],
            0.3,
            1.0,
            false,
            false,
        )
    }

    /// No-bounds checker (always feasible on bounds check).
    pub fn new_no_bounds() -> Self {
        ConstraintChecker::new_raw(
            vec![],
            vec![],
            vec![],
            vec![],
            None,
            None,
            false,
            false,
            vec![],
            0.3,
            1.0,
            false,
            false,
        )
    }

    /// Geometric-only checker (no bounds).
    pub fn new_geometric(
        forbidden_zones: Vec<crate::config::ForbiddenZone>,
        mandatory_routes: Vec<crate::config::MandatoryRoute>,
        max_bend_angle: Option<f64>,
    ) -> Self {
        let forbidden = forbidden_zones
            .into_iter()
            .filter(|z| z.vertices.len() >= 3)
            .map(|z| z.vertices.iter().map(|&[x, y]| (x, y)).collect())
            .collect();
        let mandatory = mandatory_routes
            .into_iter()
            .filter(|r| r.waypoints.len() >= 2)
            .map(|r| MandatoryRouteSpec {
                coords: r.waypoints.iter().map(|&[x, y]| (x, y)).collect(),
                corridor_half_width: r.corridor_width / 2.0,
            })
            .collect();
        ConstraintChecker::new_raw(
            vec![],
            vec![],
            forbidden,
            mandatory,
            max_bend_angle,
            None,
            false,
            false,
            vec![],
            0.3,
            1.0,
            false,
            false,
        )
    }

    /// Cover-depth checker.
    pub fn new_with_cover(min_cover: f64, enforce_max_depth: bool, max_depth: f64) -> Self {
        let dc = DesignConstraints {
            min_cover,
            max_depth,
            ..DesignConstraints::default()
        };
        ConstraintChecker::new_raw(
            vec![],
            vec![],
            vec![],
            vec![],
            None,
            Some(dc),
            true,
            enforce_max_depth,
            vec![],
            0.3,
            1.0,
            false,
            false,
        )
    }

    /// Clearance-from-existing-networks checker.
    pub fn new_with_clearance(
        existing: Vec<ExistingNetworkEntry>,
        min_vertical: f64,
        min_horizontal: f64,
        enforce: bool,
    ) -> Self {
        ConstraintChecker::new_raw(
            vec![],
            vec![],
            vec![],
            vec![],
            None,
            None,
            false,
            false,
            existing,
            min_vertical,
            min_horizontal,
            enforce,
            false,
        )
    }

    /// Slope checker.
    pub fn new_with_slopes(min_slope: f64, max_slope: f64, enforce: bool) -> Self {
        let dc = DesignConstraints {
            min_slope,
            max_slope,
            ..DesignConstraints::default()
        };
        ConstraintChecker::new_raw(
            vec![],
            vec![],
            vec![],
            vec![],
            None,
            Some(dc),
            false,
            false,
            vec![],
            0.3,
            1.0,
            false,
            enforce,
        )
    }

    /// Full-featured checker (bounds + geometric).
    pub fn new_full(
        lower: Vec<f64>,
        upper: Vec<f64>,
        forbidden_zones: Vec<crate::config::ForbiddenZone>,
        mandatory_routes: Vec<crate::config::MandatoryRoute>,
        max_bend_angle: Option<f64>,
    ) -> Self {
        let forbidden = forbidden_zones
            .into_iter()
            .filter(|z| z.vertices.len() >= 3)
            .map(|z| z.vertices.iter().map(|&[x, y]| (x, y)).collect())
            .collect();
        let mandatory = mandatory_routes
            .into_iter()
            .filter(|r| r.waypoints.len() >= 2)
            .map(|r| MandatoryRouteSpec {
                coords: r.waypoints.iter().map(|&[x, y]| (x, y)).collect(),
                corridor_half_width: r.corridor_width / 2.0,
            })
            .collect();
        ConstraintChecker::new_raw(
            lower,
            upper,
            forbidden,
            mandatory,
            max_bend_angle,
            None,
            false,
            false,
            vec![],
            0.3,
            1.0,
            false,
            false,
        )
    }

    // ── Canonical constructor (used by GeneticOptimizer / SolutionEvaluator) ─

    /// Full constructor matching the oracle `ConstraintChecker.__init__` signature.
    ///
    /// This is the constructor used in production by `SolutionEvaluator`.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        lower: Vec<f64>,
        upper: Vec<f64>,
        forbidden_zones: Vec<crate::config::ForbiddenZone>,
        mandatory_routes: Vec<crate::config::MandatoryRoute>,
        existing_networks: Vec<ExistingNetworkEntry>,
        max_bend_angle: Option<f64>,
        design_constraints: Option<DesignConstraints>,
        min_vertical_separation: f64,
        min_horizontal_separation: f64,
        enforce_cover_depth: bool,
        enforce_max_depth: bool,
        enforce_existing_clearance: bool,
        enforce_segment_slopes: bool,
    ) -> Self {
        let forbidden = forbidden_zones
            .into_iter()
            .filter(|z| z.vertices.len() >= 3)
            .map(|z| z.vertices.iter().map(|&[x, y]| (x, y)).collect())
            .collect();
        let mandatory = mandatory_routes
            .into_iter()
            .filter(|r| r.waypoints.len() >= 2)
            .map(|r| MandatoryRouteSpec {
                coords: r.waypoints.iter().map(|&[x, y]| (x, y)).collect(),
                corridor_half_width: r.corridor_width / 2.0,
            })
            .collect();
        ConstraintChecker::new_raw(
            lower,
            upper,
            forbidden,
            mandatory,
            max_bend_angle,
            design_constraints,
            enforce_cover_depth,
            enforce_max_depth,
            existing_networks,
            min_vertical_separation,
            min_horizontal_separation,
            enforce_existing_clearance,
            enforce_segment_slopes,
        )
    }

    // ── Internal constructor ──────────────────────────────────────────────

    #[allow(clippy::too_many_arguments)]
    fn new_raw(
        lower: Vec<f64>,
        upper: Vec<f64>,
        forbidden: Vec<Vec<(f64, f64)>>,
        mandatory: Vec<MandatoryRouteSpec>,
        max_bend_angle: Option<f64>,
        design: Option<DesignConstraints>,
        enforce_cover_depth: bool,
        enforce_max_depth: bool,
        existing: Vec<ExistingNetworkEntry>,
        min_vertical_separation: f64,
        min_horizontal_separation: f64,
        enforce_existing_clearance: bool,
        enforce_segment_slopes: bool,
    ) -> Self {
        ConstraintChecker {
            lower,
            upper,
            forbidden,
            mandatory,
            max_bend_angle,
            design,
            enforce_cover_depth,
            enforce_max_depth,
            existing,
            min_vertical_separation,
            min_horizontal_separation,
            enforce_existing_clearance,
            enforce_segment_slopes,
        }
    }

    // ── Public check methods ──────────────────────────────────────────────

    /// Gene-level bounds check (fast, pre-decode).
    ///
    /// Mirrors oracle `is_feasible` + `distance_to_feasible`.
    /// Tolerance of 0.001 matches oracle (constraints.py line 148).
    pub fn check_bounds(&self, genes: &[f64]) -> ConstraintReport {
        if self.lower.is_empty() || self.upper.is_empty() {
            return ConstraintReport::feasible();
        }
        // Tolerance matches oracle (constraints.py line 148):
        // infeasible only when v < lo - 0.001 or v > hi + 0.001.
        // Magnitude = raw distance (lo - v or v - hi), not the tolerance-reduced value.
        const TOL: f64 = 0.001;
        let mut violations = Vec::new();
        for (i, ((v, lo), hi)) in genes
            .iter()
            .zip(self.lower.iter())
            .zip(self.upper.iter())
            .enumerate()
        {
            let infeasible_below = *v < lo - TOL;
            let infeasible_above = *v > hi + TOL;
            if infeasible_below {
                let mag = lo - v; // raw distance
                violations.push(ConstraintViolation {
                    kind: ConstraintViolationKind::Bounds,
                    magnitude: mag,
                    description: format!(
                        "gene[{i}] = {v:.4} < lower bound {lo:.4} (violation = {mag:.4})"
                    ),
                });
            } else if infeasible_above {
                let mag = v - hi; // raw distance
                violations.push(ConstraintViolation {
                    kind: ConstraintViolationKind::Bounds,
                    magnitude: mag,
                    description: format!(
                        "gene[{i}] = {v:.4} > upper bound {hi:.4} (violation = {mag:.4})"
                    ),
                });
            }
        }
        if violations.is_empty() {
            ConstraintReport::feasible()
        } else {
            ConstraintReport::infeasible(violations)
        }
    }

    /// Route-level forbidden zone check.
    ///
    /// Mirrors oracle `check_route_feasibility` (constraints.py line 427).
    pub fn check_route(&self, route_coords: &[(f64, f64)]) -> ConstraintReport {
        if route_coords.is_empty() || self.forbidden.is_empty() {
            return ConstraintReport::feasible();
        }
        let mut violations = Vec::new();
        for (idx, zone) in self.forbidden.iter().enumerate() {
            if polyline_intersects_polygon(route_coords, zone) {
                let area = polygon_area(zone);
                violations.push(ConstraintViolation {
                    kind: ConstraintViolationKind::ForbiddenZone,
                    magnitude: 1.0, // presence-based: any intersection = magnitude 1
                    description: format!(
                        "Route crosses forbidden zone #{} (area={:.1} m^2)",
                        idx + 1,
                        area
                    ),
                });
            }
        }
        if violations.is_empty() {
            ConstraintReport::feasible()
        } else {
            ConstraintReport::infeasible(violations)
        }
    }

    /// Solution-level geometric checks.
    ///
    /// Checks forbidden zones, mandatory routes, bend angles, cover depth,
    /// existing clearance, and slopes depending on which flags are enabled.
    /// Mirrors oracle `check_solution_feasibility` (constraints.py line 168).
    pub fn check_solution(&self, solution: &Solution) -> ConstraintReport {
        let mut violations = Vec::new();
        let network = &solution.network;
        let route_lines = solution_pipe_lines(network);

        // ── Forbidden zones ───────────────────────────────────────────────
        for line in &route_lines {
            for (idx, zone) in self.forbidden.iter().enumerate() {
                if polyline_intersects_polygon(line, zone) {
                    let area = polygon_area(zone);
                    violations.push(ConstraintViolation {
                        kind: ConstraintViolationKind::ForbiddenZone,
                        magnitude: 1.0,
                        description: format!(
                            "Pipe route crosses forbidden zone #{} (area={:.1} m^2)",
                            idx + 1,
                            area
                        ),
                    });
                }
            }
        }

        // ── Mandatory routes ──────────────────────────────────────────────
        for (idx, route) in self.mandatory.iter().enumerate() {
            if route.coords.len() < 2 {
                continue;
            }
            let traversed = route_lines.iter().any(|line| {
                polylines_intersect(line, &route.coords)
                    || polyline_distance(line, &route.coords) <= 0.001
            });
            if !traversed {
                violations.push(ConstraintViolation {
                    kind: ConstraintViolationKind::MandatoryRoute,
                    magnitude: 1.0,
                    description: format!("Solution does not traverse mandatory route #{}", idx + 1),
                });
                continue;
            }
            // Corridor containment check (only for pipes that touch the corridor)
            if route.corridor_half_width > 0.0 {
                for line in &route_lines {
                    // Check if this pipe is near the mandatory route corridor
                    if polyline_distance(line, &route.coords) > route.corridor_half_width + 0.001 {
                        continue; // not near the corridor
                    }
                    // Check if the pipe stays within the corridor
                    if !polyline_within_buffer(line, &route.coords, route.corridor_half_width) {
                        violations.push(ConstraintViolation {
                            kind: ConstraintViolationKind::MandatoryRoute,
                            magnitude: 1.0,
                            description: format!(
                                "Pipe near mandatory route #{} exits corridor of width {:.2} m",
                                idx + 1,
                                route.corridor_half_width * 2.0
                            ),
                        });
                    }
                }
            }
        }

        // ── Bend angles ───────────────────────────────────────────────────
        if self.max_bend_angle.is_some() {
            violations.extend(self.check_bend_angles_inner(network));
        }

        // ── Cover depth / trench depth ────────────────────────────────────
        if let Some(ref dc) = self.design {
            if self.enforce_cover_depth || self.enforce_max_depth {
                violations.extend(self.check_cover_depth_inner(network, dc));
            }
            if self.enforce_segment_slopes {
                violations.extend(self.check_slopes_inner(network, dc));
            }
        }

        // ── Existing clearance ────────────────────────────────────────────
        if self.enforce_existing_clearance && !self.existing.is_empty() {
            violations.extend(self.check_existing_clearance_inner(network));
        }

        if violations.is_empty() {
            ConstraintReport::feasible()
        } else {
            ConstraintReport::infeasible(violations)
        }
    }

    /// Combined check: bounds first (fast), then geometric if bounds pass.
    ///
    /// This matches the oracle's DEAP integration where `is_feasible` (bounds)
    /// gates the expensive geometric checks via `DeltaPenalty`.
    pub fn check_full(&self, genes: &[f64], solution: &Solution) -> ConstraintReport {
        let bounds_report = self.check_bounds(genes);
        if !bounds_report.feasible {
            return bounds_report;
        }
        self.check_solution(solution)
    }

    // ── Internal geometric helpers ────────────────────────────────────────

    /// Bend angle check. Oracle line 258.
    fn check_bend_angles_inner(&self, network: &PipeNetwork) -> Vec<ConstraintViolation> {
        let max_angle = match self.max_bend_angle {
            Some(a) => a,
            None => return vec![],
        };
        // Build per-node direction vectors (same as oracle node_pipes dict)
        let mut node_dirs: std::collections::BTreeMap<String, Vec<(f64, f64)>> =
            std::collections::BTreeMap::new();
        for pipe in &network.pipes {
            let start = match network.get_node(&pipe.start_node_id) {
                Some(n) => n,
                None => continue,
            };
            let end = match network.get_node(&pipe.end_node_id) {
                Some(n) => n,
                None => continue,
            };
            let dx = end.x - start.x;
            let dy = end.y - start.y;
            node_dirs
                .entry(pipe.start_node_id.0.clone())
                .or_default()
                .push((dx, dy));
            node_dirs
                .entry(pipe.end_node_id.0.clone())
                .or_default()
                .push((-dx, -dy));
        }
        let mut violations = Vec::new();
        for (node_id, dirs) in &node_dirs {
            if dirs.len() < 2 {
                continue;
            }
            for i in 0..dirs.len() {
                for j in (i + 1)..dirs.len() {
                    let angle = angle_between(dirs[i], dirs[j]);
                    if angle < max_angle {
                        violations.push(ConstraintViolation {
                            kind: ConstraintViolationKind::BendAngle,
                            magnitude: max_angle - angle,
                            description: format!(
                                "Node {}: bend angle {:.1} deg < min allowed {:.1} deg",
                                node_id, angle, max_angle
                            ),
                        });
                    }
                }
            }
        }
        violations
    }

    /// Cover depth check. Oracle line 286.
    fn check_cover_depth_inner(
        &self,
        network: &PipeNetwork,
        dc: &DesignConstraints,
    ) -> Vec<ConstraintViolation> {
        const EPS: f64 = 1e-3;
        let mut violations = Vec::new();
        for pipe in &network.pipes {
            let start = match network.get_node(&pipe.start_node_id) {
                Some(n) => n,
                None => continue,
            };
            let end = match network.get_node(&pipe.end_node_id) {
                Some(n) => n,
                None => continue,
            };
            // Skip nodes with no elevation data (both at z == 0)
            if start.z == 0.0 && end.z == 0.0 {
                continue;
            }
            let cover_start = start.z - pipe.start_invert;
            let cover_end = end.z - pipe.end_invert;
            if self.enforce_cover_depth {
                if cover_start < dc.min_cover - EPS {
                    violations.push(ConstraintViolation {
                        kind: ConstraintViolationKind::CoverDepth,
                        magnitude: dc.min_cover - cover_start,
                        description: format!(
                            "Pipe {}: cover at start {:.2} m < min {:.2} m",
                            pipe.id, cover_start, dc.min_cover
                        ),
                    });
                }
                if cover_end < dc.min_cover - EPS {
                    violations.push(ConstraintViolation {
                        kind: ConstraintViolationKind::CoverDepth,
                        magnitude: dc.min_cover - cover_end,
                        description: format!(
                            "Pipe {}: cover at end {:.2} m < min {:.2} m",
                            pipe.id, cover_end, dc.min_cover
                        ),
                    });
                }
            }
            if self.enforce_max_depth {
                if cover_start > dc.max_depth + EPS {
                    violations.push(ConstraintViolation {
                        kind: ConstraintViolationKind::MaxDepth,
                        magnitude: cover_start - dc.max_depth,
                        description: format!(
                            "Pipe {}: trench depth at start {:.2} m > max {:.2} m",
                            pipe.id, cover_start, dc.max_depth
                        ),
                    });
                }
                if cover_end > dc.max_depth + EPS {
                    violations.push(ConstraintViolation {
                        kind: ConstraintViolationKind::MaxDepth,
                        magnitude: cover_end - dc.max_depth,
                        description: format!(
                            "Pipe {}: trench depth at end {:.2} m > max {:.2} m",
                            pipe.id, cover_end, dc.max_depth
                        ),
                    });
                }
            }
        }
        violations
    }

    /// Slope check. Oracle line 338.
    fn check_slopes_inner(
        &self,
        network: &PipeNetwork,
        dc: &DesignConstraints,
    ) -> Vec<ConstraintViolation> {
        const EPS: f64 = 1e-6;
        let mut violations = Vec::new();
        for pipe in &network.pipes {
            if pipe.length <= 0.0 {
                continue;
            }
            // Skip pipes with no invert data (both at 0)
            if pipe.start_invert == 0.0 && pipe.end_invert == 0.0 {
                continue;
            }
            let slope = pipe.slope;
            // Adverse (negative/uphill) slopes handled by pumping objective — skip
            if slope <= 0.0 {
                continue;
            }
            if slope < dc.min_slope - EPS {
                violations.push(ConstraintViolation {
                    kind: ConstraintViolationKind::Slope,
                    magnitude: dc.min_slope - slope,
                    description: format!(
                        "Pipe {}: slope {:.4} < min {:.4}",
                        pipe.id, slope, dc.min_slope
                    ),
                });
            } else if slope > dc.max_slope + EPS {
                violations.push(ConstraintViolation {
                    kind: ConstraintViolationKind::Slope,
                    magnitude: slope - dc.max_slope,
                    description: format!(
                        "Pipe {}: slope {:.4} > max {:.4}",
                        pipe.id, slope, dc.max_slope
                    ),
                });
            }
        }
        violations
    }

    /// Existing network clearance check. Oracle line 368.
    fn check_existing_clearance_inner(&self, network: &PipeNetwork) -> Vec<ConstraintViolation> {
        let mut violations = Vec::new();
        for pipe in &network.pipes {
            let start = match network.get_node(&pipe.start_node_id) {
                Some(n) => n,
                None => continue,
            };
            let end = match network.get_node(&pipe.end_node_id) {
                Some(n) => n,
                None => continue,
            };
            // Build pipe polyline: start → waypoints → end
            let mut pipe_coords: Vec<(f64, f64)> = Vec::with_capacity(2 + pipe.waypoints.len());
            pipe_coords.push((start.x, start.y));
            for wp in &pipe.waypoints {
                pipe_coords.push((wp[0], wp[1]));
            }
            pipe_coords.push((end.x, end.y));

            let has_elev = !(start.z == 0.0 && end.z == 0.0);
            let pipe_avg_depth = if has_elev {
                Some(((start.z - pipe.start_invert) + (end.z - pipe.end_invert)) / 2.0)
            } else {
                None
            };

            for (idx, existing) in self.existing.iter().enumerate() {
                let required_hsep = existing.setback.unwrap_or(self.min_horizontal_separation);
                if polylines_intersect(&pipe_coords, &existing.coords) {
                    // Crossing: check vertical separation
                    if let Some(d) = pipe_avg_depth {
                        let vsep = (d - existing.depth).abs();
                        if vsep < self.min_vertical_separation {
                            violations.push(ConstraintViolation {
                                kind: ConstraintViolationKind::ExistingClearance,
                                magnitude: self.min_vertical_separation - vsep,
                                description: format!(
                                    "Pipe {} crosses existing network #{} with vertical separation {:.2} m < min {:.2} m",
                                    pipe.id, idx + 1, vsep, self.min_vertical_separation
                                ),
                            });
                        }
                    }
                } else {
                    // Parallel: check horizontal separation
                    let hsep = polyline_min_distance(&pipe_coords, &existing.coords);
                    if hsep < required_hsep {
                        violations.push(ConstraintViolation {
                            kind: ConstraintViolationKind::ExistingClearance,
                            magnitude: required_hsep - hsep,
                            description: format!(
                                "Pipe {} runs parallel to existing network #{} at {:.2} m < min {:.2} m",
                                pipe.id, idx + 1, hsep, required_hsep
                            ),
                        });
                    }
                }
            }
        }
        violations
    }
}

// ── Geometry helpers (deterministic, no HashMap) ──────────────────────────────

type Pt = (f64, f64);
type Seg = (Pt, Pt);

/// Extract pipe center-line polylines from a network solution.
///
/// Mirrors oracle `_solution_lines` (constraints.py line 236).
fn solution_pipe_lines(network: &PipeNetwork) -> Vec<Vec<Pt>> {
    let mut lines = Vec::new();
    for pipe in &network.pipes {
        let start = match network.get_node(&pipe.start_node_id) {
            Some(n) => n,
            None => continue,
        };
        let end = match network.get_node(&pipe.end_node_id) {
            Some(n) => n,
            None => continue,
        };
        let mut coords: Vec<Pt> = Vec::with_capacity(2 + pipe.waypoints.len());
        coords.push((start.x, start.y));
        for wp in &pipe.waypoints {
            coords.push((wp[0], wp[1]));
        }
        coords.push((end.x, end.y));
        lines.push(coords);
    }
    lines
}

/// Check whether two polylines intersect (segment-pair test).
fn polylines_intersect(a: &[Pt], b: &[Pt]) -> bool {
    for i in 0..a.len().saturating_sub(1) {
        for j in 0..b.len().saturating_sub(1) {
            if seg_intersect((a[i], a[i + 1]), (b[j], b[j + 1])) {
                return true;
            }
        }
    }
    false
}

/// Check whether a polyline intersects a polygon (closed ring).
///
/// Tests each polyline segment against each polygon edge.
/// Also tests whether any polyline point is inside the polygon.
fn polyline_intersects_polygon(line: &[Pt], polygon: &[Pt]) -> bool {
    if polygon.len() < 3 {
        return false;
    }
    // Build closed ring: polygon[0..n] + polygon[0]
    let n = polygon.len();
    for i in 0..line.len().saturating_sub(1) {
        let seg_ab = (line[i], line[i + 1]);
        for j in 0..n {
            let seg_cd = (polygon[j], polygon[(j + 1) % n]);
            if seg_intersect(seg_ab, seg_cd) {
                return true;
            }
        }
    }
    // Check if any line point is inside the polygon (ray casting)
    for &pt in line {
        if point_in_polygon(pt, polygon) {
            return true;
        }
    }
    false
}

/// Segment-segment intersection test (cross-product formulation).
fn seg_intersect(ab: Seg, cd: Seg) -> bool {
    let ((ax, ay), (bx, by)) = ab;
    let ((cx, cy), (dx, dy)) = cd;
    let d1x = bx - ax;
    let d1y = by - ay;
    let d2x = dx - cx;
    let d2y = dy - cy;
    let cross = d1x * d2y - d1y * d2x;
    if cross.abs() < 1e-12 {
        return false; // parallel or collinear
    }
    let t = ((cx - ax) * d2y - (cy - ay) * d2x) / cross;
    let u = ((cx - ax) * d1y - (cy - ay) * d1x) / cross;
    (0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u)
}

/// Point-in-polygon test using ray casting algorithm.
fn point_in_polygon(pt: Pt, polygon: &[Pt]) -> bool {
    let (px, py) = pt;
    let n = polygon.len();
    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        let (xi, yi) = polygon[i];
        let (xj, yj) = polygon[j];
        if ((yi > py) != (yj > py)) && (px < (xj - xi) * (py - yi) / (yj - yi) + xi) {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Polygon area via shoelace formula (for reporting only).
fn polygon_area(polygon: &[Pt]) -> f64 {
    let n = polygon.len();
    let mut area = 0.0_f64;
    let mut j = n - 1;
    for i in 0..n {
        area += (polygon[j].0 + polygon[i].0) * (polygon[j].1 - polygon[i].1);
        j = i;
    }
    area.abs() / 2.0
}

/// Minimum distance between two polylines.
fn polyline_min_distance(a: &[Pt], b: &[Pt]) -> f64 {
    let mut min_dist = f64::INFINITY;
    for i in 0..a.len().saturating_sub(1) {
        for j in 0..b.len().saturating_sub(1) {
            let d = seg_to_seg_distance((a[i], a[i + 1]), (b[j], b[j + 1]));
            if d < min_dist {
                min_dist = d;
            }
        }
    }
    min_dist
}

/// Minimum distance between a polyline and a reference polyline (for mandatory route proximity).
pub(crate) fn polyline_distance(a: &[Pt], b: &[Pt]) -> f64 {
    polyline_min_distance(a, b)
}

/// Check whether all points of polyline `a` lie within `buffer` distance of polyline `b`.
fn polyline_within_buffer(a: &[Pt], b: &[Pt], buffer: f64) -> bool {
    for &pt in a {
        let mut min_d = f64::INFINITY;
        for j in 0..b.len().saturating_sub(1) {
            let d = pt_to_seg_distance(pt, (b[j], b[j + 1]));
            if d < min_d {
                min_d = d;
            }
        }
        if min_d > buffer + 1e-6 {
            return false;
        }
    }
    true
}

/// Minimum distance from point `p` to segment `seg`.
fn pt_to_seg_distance(p: Pt, seg: Seg) -> f64 {
    let (px, py) = p;
    let ((ax, ay), (bx, by)) = seg;
    let dx = bx - ax;
    let dy = by - ay;
    let len2 = dx * dx + dy * dy;
    if len2 < 1e-14 {
        return ((px - ax).powi(2) + (py - ay).powi(2)).sqrt();
    }
    let t = ((px - ax) * dx + (py - ay) * dy) / len2;
    let t = t.clamp(0.0, 1.0);
    let cx = ax + t * dx;
    let cy = ay + t * dy;
    ((px - cx).powi(2) + (py - cy).powi(2)).sqrt()
}

/// Minimum distance between two segments.
fn seg_to_seg_distance(ab: Seg, cd: Seg) -> f64 {
    if seg_intersect(ab, cd) {
        return 0.0;
    }
    let (a, b) = ab;
    let (c, d) = cd;
    let d1 = pt_to_seg_distance(a, cd);
    let d2 = pt_to_seg_distance(b, cd);
    let d3 = pt_to_seg_distance(c, ab);
    let d4 = pt_to_seg_distance(d, ab);
    d1.min(d2).min(d3).min(d4)
}

/// Angle (degrees) between two 2D direction vectors.
///
/// Mirrors oracle `_angle_between` (constraints.py line 417).
fn angle_between(d1: Pt, d2: Pt) -> f64 {
    let dot = d1.0 * d2.0 + d1.1 * d2.1;
    let mag1 = (d1.0 * d1.0 + d1.1 * d1.1).sqrt();
    let mag2 = (d2.0 * d2.0 + d2.1 * d2.1).sqrt();
    if mag1 < 1e-12 || mag2 < 1e-12 {
        return 180.0;
    }
    let cos_angle = (dot / (mag1 * mag2)).clamp(-1.0, 1.0);
    cos_angle.acos() * (180.0 / PI)
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests (REQ-004, REQ-018)
// Previously in tests/pr8c_constraints.rs. Moved inline (REQ-014).
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ForbiddenZone, MandatoryRoute};
    use hydro_types::enums::{NodeType, PipeMaterial};
    use hydro_types::network::{NetworkNode, NetworkPipe, NodeId, PipeId, PipeNetwork};
    use hydro_types::response::{Solution, SolutionMetadata, SolutionScore};

    fn make_solution(nodes: Vec<NetworkNode>, pipes: Vec<NetworkPipe>) -> Solution {
        Solution {
            rank: 1,
            network: PipeNetwork::new("test", nodes, pipes),
            score: SolutionScore::default(),
            metadata: SolutionMetadata::default(),
        }
    }

    fn make_pipe(id: &str, s: &str, e: &str, length: f64) -> NetworkPipe {
        NetworkPipe {
            id: PipeId::new(id),
            start_node_id: NodeId::new(s),
            end_node_id: NodeId::new(e),
            length,
            effective_length: length,
            diameter: 0.3,
            material: PipeMaterial::Pvc,
            roughness: 0.009,
            start_invert: 0.0,
            end_invert: 0.0,
            slope: 0.0,
            design_flow: 0.0,
            waypoints: vec![],
            metadata: Default::default(),
        }
    }

    fn make_node(id: &str, x: f64, y: f64, z: f64) -> NetworkNode {
        NetworkNode::new(id, x, y, z, NodeType::Manhole)
    }

    // ── ConstraintReport + spec penalty formula ───────────────────────────────

    #[test]
    fn test_feasible_candidate_no_penalty() {
        let checker = ConstraintChecker::new_bounds_only(vec![0.0, 0.0], vec![10.0, 5.0]);
        let report = checker.check_bounds(&[5.0, 2.5]);
        assert!(report.feasible);
        assert!(report.violations.is_empty());
        assert_eq!(report.penalty, 0.0);
    }

    #[test]
    fn test_single_violation_spec_penalty_formula() {
        let checker = ConstraintChecker::new_bounds_only(vec![0.0, 0.0], vec![10.0, 5.0]);
        let report = checker.check_bounds(&[5.0, 7.0]);
        assert!(!report.feasible);
        assert_eq!(report.violations.len(), 1);
        let expected = 1e12 + 2.0 * 1e9;
        assert!((report.penalty - expected).abs() < 1.0);
    }

    #[test]
    fn test_multiple_violations_penalty_sums() {
        let checker = ConstraintChecker::new_bounds_only(vec![0.0, 0.0, 0.0], vec![5.0, 5.0, 5.0]);
        let report = checker.check_bounds(&[-1.0, 2.5, 8.0]);
        assert!(!report.feasible);
        assert_eq!(report.violations.len(), 2);
        let total: f64 = report
            .violations
            .iter()
            .map(|v| 1e12 + v.magnitude * 1e9)
            .sum();
        assert!((report.penalty - total).abs() < 1.0);
    }

    #[test]
    fn test_violation_magnitude_is_correct() {
        let checker = ConstraintChecker::new_bounds_only(vec![0.0], vec![10.0]);
        let report = checker.check_bounds(&[13.5]);
        assert_eq!(report.violations.len(), 1);
        assert!((report.violations[0].magnitude - 3.5).abs() < 1e-10);
    }

    // ── Gene-level bounds ─────────────────────────────────────────────────────

    #[test]
    fn test_bounds_below_lower_bound_infeasible() {
        let checker = ConstraintChecker::new_bounds_only(vec![1.0], vec![5.0]);
        let report = checker.check_bounds(&[0.0]);
        assert!(!report.feasible);
        assert_eq!(report.violations.len(), 1);
        assert!(report.violations[0].magnitude > 0.0);
    }

    #[test]
    fn test_bounds_above_upper_bound_infeasible() {
        let checker = ConstraintChecker::new_bounds_only(vec![0.0], vec![3.0]);
        assert!(!checker.check_bounds(&[5.0]).feasible);
    }

    #[test]
    fn test_bounds_exact_boundary_feasible() {
        let checker = ConstraintChecker::new_bounds_only(vec![0.0], vec![3.0]);
        assert!(checker.check_bounds(&[3.0]).feasible);
        assert!(checker.check_bounds(&[0.0]).feasible);
    }

    #[test]
    fn test_bounds_no_bounds_always_feasible() {
        let checker = ConstraintChecker::new_no_bounds();
        let report = checker.check_bounds(&[999.0, -999.0]);
        assert!(report.feasible);
        assert_eq!(report.penalty, 0.0);
    }

    // ── Forbidden zones ───────────────────────────────────────────────────────

    #[test]
    fn test_forbidden_zone_pipe_intersects_is_infeasible() {
        let zone = ForbiddenZone {
            vertices: vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]],
            label: None,
        };
        let checker = ConstraintChecker::new_geometric(vec![zone], vec![], None);
        let route = vec![(-5.0_f64, 5.0_f64), (15.0, 5.0)];
        let report = checker.check_route(&route);
        assert!(!report.feasible);
        assert!(report
            .violations
            .iter()
            .any(|v| v.kind == ConstraintViolationKind::ForbiddenZone));
        assert!(report.penalty > 0.0);
    }

    #[test]
    fn test_forbidden_zone_pipe_clear_is_feasible() {
        let zone = ForbiddenZone {
            vertices: vec![[0.0, 0.0], [5.0, 0.0], [5.0, 5.0], [0.0, 5.0]],
            label: None,
        };
        let checker = ConstraintChecker::new_geometric(vec![zone], vec![], None);
        let route = vec![(10.0_f64, 0.0_f64), (20.0, 0.0)];
        assert!(checker.check_route(&route).feasible);
    }

    #[test]
    fn test_no_forbidden_zones_always_feasible() {
        let checker = ConstraintChecker::new_geometric(vec![], vec![], None);
        assert!(
            checker
                .check_route(&[(0.0_f64, 0.0_f64), (100.0, 100.0)])
                .feasible
        );
    }

    // ── Mandatory routes ──────────────────────────────────────────────────────

    #[test]
    fn test_mandatory_route_satisfied_is_feasible() {
        let mandatory = MandatoryRoute {
            waypoints: vec![[0.0, 0.0], [10.0, 0.0]],
            corridor_width: 2.0,
            label: None,
        };
        let checker = ConstraintChecker::new_geometric(vec![], vec![mandatory], None);
        let solution = make_solution(
            vec![
                make_node("n1", 0.0, 0.0, 5.0),
                make_node("n2", 10.0, 0.0, 5.0),
            ],
            vec![make_pipe("p1", "n1", "n2", 10.0)],
        );
        assert!(checker.check_solution(&solution).feasible);
    }

    #[test]
    fn test_mandatory_route_missing_is_infeasible() {
        let mandatory = MandatoryRoute {
            waypoints: vec![[100.0, 100.0], [200.0, 100.0]],
            corridor_width: 2.0,
            label: None,
        };
        let checker = ConstraintChecker::new_geometric(vec![], vec![mandatory], None);
        let solution = make_solution(
            vec![
                make_node("n1", 0.0, 0.0, 5.0),
                make_node("n2", 10.0, 0.0, 5.0),
            ],
            vec![make_pipe("p1", "n1", "n2", 10.0)],
        );
        let report = checker.check_solution(&solution);
        assert!(!report.feasible);
        assert!(report
            .violations
            .iter()
            .any(|v| v.kind == ConstraintViolationKind::MandatoryRoute));
    }

    // ── Bend angle ────────────────────────────────────────────────────────────

    #[test]
    fn test_bend_angle_violation_detected() {
        let checker = ConstraintChecker::new_geometric(vec![], vec![], Some(120.0));
        let solution = make_solution(
            vec![
                make_node("n1", 0.0, 0.0, 5.0),
                make_node("n2", 10.0, 0.0, 5.0),
                make_node("n3", 10.0, 10.0, 5.0),
            ],
            vec![
                make_pipe("p1", "n1", "n2", 10.0),
                make_pipe("p2", "n2", "n3", 10.0),
            ],
        );
        let report = checker.check_solution(&solution);
        assert!(!report.feasible);
        assert!(report
            .violations
            .iter()
            .any(|v| v.kind == ConstraintViolationKind::BendAngle));
    }

    #[test]
    fn test_bend_angle_straight_pipe_feasible() {
        let checker = ConstraintChecker::new_geometric(vec![], vec![], Some(45.0));
        let solution = make_solution(
            vec![
                make_node("n1", 0.0, 0.0, 5.0),
                make_node("n2", 10.0, 0.0, 5.0),
                make_node("n3", 20.0, 0.0, 5.0),
            ],
            vec![
                make_pipe("p1", "n1", "n2", 10.0),
                make_pipe("p2", "n2", "n3", 10.0),
            ],
        );
        assert!(checker.check_solution(&solution).feasible);
    }

    // ── Cover depth ───────────────────────────────────────────────────────────

    #[test]
    fn test_cover_depth_below_min_is_infeasible() {
        let checker = ConstraintChecker::new_with_cover(1.5, false, 10.0);
        let mut p = make_pipe("p1", "n1", "n2", 10.0);
        p.start_invert = 4.5;
        p.end_invert = 4.5;
        let mut n1 = make_node("n1", 0.0, 0.0, 5.0);
        n1.z = 5.0;
        let mut n2 = make_node("n2", 10.0, 0.0, 5.0);
        n2.z = 5.0;
        let report = checker.check_solution(&make_solution(vec![n1, n2], vec![p]));
        assert!(!report.feasible);
        assert!(report
            .violations
            .iter()
            .any(|v| v.kind == ConstraintViolationKind::CoverDepth));
    }

    #[test]
    fn test_cover_depth_above_min_is_feasible() {
        let checker = ConstraintChecker::new_with_cover(1.5, false, 10.0);
        let mut p = make_pipe("p1", "n1", "n2", 10.0);
        p.start_invert = 2.0;
        p.end_invert = 2.0;
        let mut n1 = make_node("n1", 0.0, 0.0, 5.0);
        n1.z = 5.0;
        let mut n2 = make_node("n2", 10.0, 0.0, 5.0);
        n2.z = 5.0;
        assert!(
            checker
                .check_solution(&make_solution(vec![n1, n2], vec![p]))
                .feasible
        );
    }

    // ── Existing clearance ────────────────────────────────────────────────────

    #[test]
    fn test_existing_clearance_crossing_insufficient_vertical_infeasible() {
        let existing = ExistingNetworkEntry {
            coords: vec![(-5.0, 5.0), (15.0, 5.0)],
            depth: 1.5,
            setback: None,
        };
        let checker = ConstraintChecker::new_with_clearance(vec![existing], 0.3, 1.0, true);
        let mut p = make_pipe("p1", "n1", "n2", 20.0);
        p.start_invert = 3.5;
        p.end_invert = 3.5;
        let mut n1 = make_node("n1", 5.0, -5.0, 5.0);
        n1.z = 5.0;
        let mut n2 = make_node("n2", 5.0, 15.0, 5.0);
        n2.z = 5.0;
        let report = checker.check_solution(&make_solution(vec![n1, n2], vec![p]));
        assert!(!report.feasible);
        assert!(report
            .violations
            .iter()
            .any(|v| v.kind == ConstraintViolationKind::ExistingClearance));
    }

    #[test]
    fn test_existing_clearance_parallel_sufficient_horizontal_feasible() {
        let existing = ExistingNetworkEntry {
            coords: vec![(0.0, 0.0), (10.0, 0.0)],
            depth: 1.5,
            setback: None,
        };
        let checker = ConstraintChecker::new_with_clearance(vec![existing], 0.3, 1.0, true);
        let solution = make_solution(
            vec![
                make_node("n1", 0.0, 5.0, 5.0),
                make_node("n2", 10.0, 5.0, 5.0),
            ],
            vec![make_pipe("p1", "n1", "n2", 10.0)],
        );
        assert!(checker.check_solution(&solution).feasible);
    }

    // ── Slope bounds ──────────────────────────────────────────────────────────

    #[test]
    fn test_slope_below_min_is_infeasible() {
        let checker = ConstraintChecker::new_with_slopes(0.005, 0.1, true);
        let mut p = make_pipe("p1", "n1", "n2", 100.0);
        p.start_invert = 5.0;
        p.end_invert = 4.9;
        p.slope = 0.001;
        let report = checker.check_solution(&make_solution(
            vec![
                make_node("n1", 0.0, 0.0, 6.0),
                make_node("n2", 100.0, 0.0, 5.9),
            ],
            vec![p],
        ));
        assert!(!report.feasible);
        assert!(report
            .violations
            .iter()
            .any(|v| v.kind == ConstraintViolationKind::Slope));
    }

    #[test]
    fn test_slope_above_max_is_infeasible() {
        let checker = ConstraintChecker::new_with_slopes(0.001, 0.05, true);
        let mut p = make_pipe("p1", "n1", "n2", 100.0);
        p.start_invert = 5.0;
        p.end_invert = -5.0;
        p.slope = 0.1;
        let report = checker.check_solution(&make_solution(
            vec![
                make_node("n1", 0.0, 0.0, 6.0),
                make_node("n2", 100.0, 0.0, -4.0),
            ],
            vec![p],
        ));
        assert!(!report.feasible);
    }

    #[test]
    fn test_adverse_slope_not_rejected_by_slope_check() {
        let checker = ConstraintChecker::new_with_slopes(0.005, 0.1, true);
        let mut p = make_pipe("p1", "n1", "n2", 100.0);
        p.start_invert = 3.0;
        p.end_invert = 4.0;
        p.slope = -0.01;
        let report = checker.check_solution(&make_solution(
            vec![
                make_node("n1", 0.0, 0.0, 5.0),
                make_node("n2", 100.0, 0.0, 5.0),
            ],
            vec![p],
        ));
        assert!(report.feasible);
    }

    // ── Aggregate check + REQ-018 ─────────────────────────────────────────────

    #[test]
    fn test_aggregate_check_feasible_candidate() {
        let checker =
            ConstraintChecker::new_full(vec![0.0, 0.0], vec![10.0, 5.0], vec![], vec![], None);
        let solution = make_solution(
            vec![
                make_node("n1", 0.0, 0.0, 5.0),
                make_node("n2", 10.0, 0.0, 5.0),
            ],
            vec![make_pipe("p1", "n1", "n2", 10.0)],
        );
        let report = checker.check_full(&[5.0, 2.5], &solution);
        assert!(report.feasible);
        assert_eq!(report.penalty, 0.0);
    }

    #[test]
    fn test_aggregate_check_bounds_infeasible_skips_geometric() {
        let checker = ConstraintChecker::new_full(vec![0.0], vec![10.0], vec![], vec![], None);
        let solution = make_solution(
            vec![
                make_node("n1", 0.0, 0.0, 5.0),
                make_node("n2", 10.0, 0.0, 5.0),
            ],
            vec![make_pipe("p1", "n1", "n2", 10.0)],
        );
        let report = checker.check_full(&[15.0], &solution);
        assert!(!report.feasible);
        let expected = 1e12 + 5.0 * 1e9;
        assert!((report.penalty - expected).abs() < 1.0);
    }

    #[test]
    fn test_req018_feasibility_rate_at_least_10_percent() {
        let lower = vec![0.0, 0.5, 1.0, 0.0, 60.0];
        let upper = vec![9.0, 2.0, 1.5, 3.0, 120.0];
        let checker = ConstraintChecker::new_bounds_only(lower.clone(), upper.clone());
        let mut individuals: Vec<Vec<f64>> = Vec::new();
        for _ in 0..45 {
            let mid: Vec<f64> = lower
                .iter()
                .zip(upper.iter())
                .map(|(lo, hi)| (lo + hi) / 2.0)
                .collect();
            individuals.push(mid);
        }
        for _ in 0..5 {
            individuals.push(vec![-1.0, 1.0, 1.2, 1.0, 90.0]);
        }
        let feasible = individuals
            .iter()
            .filter(|ind| checker.check_bounds(ind).feasible)
            .count();
        assert!(feasible as f64 / 50.0 >= 0.10);
    }
}
