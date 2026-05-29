//! Error types for hydro-types.

use thiserror::Error;

/// Errors produced during domain type construction or validation.
#[derive(Debug, Error, PartialEq)]
pub enum HydroTypesError {
    /// A required field was missing or null when it must be present.
    #[error("Missing required field: {field}")]
    MissingRequired { field: &'static str },

    /// A numeric field value was outside its valid range.
    #[error("Field '{field}' value {value} is outside valid range [{min}, {max}]")]
    OutOfRange {
        field: &'static str,
        value: f64,
        min: f64,
        max: f64,
    },

    /// A cross-field constraint was violated (e.g. min > max).
    #[error("Cross-field validation failed: {message}")]
    CrossFieldViolation { message: String },

    /// An unknown or unsupported enum variant was supplied.
    #[error("Unknown variant '{value}' for field '{field}'")]
    UnknownVariant { field: &'static str, value: String },

    /// available_diameters contained a non-positive value.
    #[error("available_diameters must contain only positive values")]
    InvalidDiameter,

    /// A polygon constraint had fewer than the required number of coordinates.
    #[error("Polygon constraint requires at least 3 coordinates, got {got}")]
    TooFewCoords { got: usize },
}
