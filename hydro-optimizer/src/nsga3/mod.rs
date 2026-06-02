//! NSGA-III — multi-objective selection based on reference points (Deb & Jain 2014).
//!
//! # Module layout
//! - `nondom_sort`      — Deb 2002 fast nondominated sort, O(M·N²).
//! - `reference_points` — Das-Dennis uniform simplex lattice.
//! - `normalize`        — extreme-point hyperplane intercept normalization (PR-8d2).
//! - `niching`          — association + niche-count selection (PR-8d2).
//! - `selection`        — NSGA-III environmental selection wrapper (PR-8d2).

pub(crate) mod niching;
pub(crate) mod nondom_sort;
pub(crate) mod normalize;
pub(crate) mod reference_points;
pub(crate) mod selection;
