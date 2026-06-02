//! NSGA-III — multi-objective selection based on reference points (Deb & Jain 2014).
//!
//! # Module layout
//! - `nondom_sort`      — Deb 2002 fast nondominated sort, O(M·N²).
//! - `reference_points` — Das-Dennis uniform simplex lattice.
//! - `normalize`        — extreme-point hyperplane intercept normalization (PR-8d2).
//! - `niching`          — association + niche-count selection (PR-8d2).
//! - `selection`        — NSGA-III environmental selection wrapper (PR-8d2).
//!
//! # Dead-code note
//! `sel_nsga3` and its dependencies are `pub(crate)` items consumed by `optimizer.rs`
//! (PR-8f). Until that module exists, suppress the warning here so the `-D warnings`
//! gate stays green across all slices.
#![allow(dead_code)]

pub(crate) mod niching;
pub(crate) mod nondom_sort;
pub(crate) mod normalize;
pub(crate) mod reference_points;
pub(crate) mod selection;

// NOTE: sel_nsga3 and its dependencies are consumed by optimizer.rs (PR-8f).
// The re-export is reserved for that consumer; suppress the unused-import lint
// until the optimizer module exists.
#[allow(unused_imports)]
pub(crate) use selection::sel_nsga3;
