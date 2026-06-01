//! NSGA-III — multi-objective selection based on reference points (Deb & Jain 2014).
//!
//! PR-8d1 scope: fast nondominated sort + Das-Dennis reference-point generation.
//! PR-8d2 will add: normalize, niching, selection wrapper.
//!
//! # Module layout
//! - `nondom_sort`      — Deb 2002 fast nondominated sort, O(M·N²).
//! - `reference_points` — Das-Dennis uniform simplex lattice.
//!
//! Dead-code suppression: PR-8d1 introduces these items; PR-8d2 will wire them
//! into the selection loop. Until then, `#[allow(dead_code)]` prevents the
//! `-D warnings` gate from failing on legitimately unused-for-now functions.
#![allow(dead_code)]

pub(crate) mod nondom_sort;
pub(crate) mod reference_points;
