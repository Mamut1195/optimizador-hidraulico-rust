//! Error types for the hydro-hydraulics crate.

use thiserror::Error;

/// Errors produced by hydraulic computation functions.
#[derive(Debug, Error, PartialEq)]
pub enum HydraulicsError {
    /// A required input value is non-physical (e.g. negative diameter).
    #[error("invalid input: {msg}")]
    InvalidInput { msg: String },

    /// A pump curve determinant is too close to zero (degenerate geometry).
    #[error("degenerate pump curve: determinant {det} is below threshold")]
    DegenerateCurve { det: f64 },
}
