//! NSGA-III — multi-objective selection based on reference points (Deb & Jain 2014).
//!
//! PR-8d1 scope: fast nondominated sort + Das-Dennis reference-point generation.
//! PR-8d2 will add: normalize, niching, selection wrapper.
//!
//! # Module layout
//! - `nondom_sort`      — Deb 2002 fast nondominated sort, O(M·N²).
//! - `reference_points` — Das-Dennis uniform simplex lattice.

pub mod nondom_sort;
pub mod reference_points;
