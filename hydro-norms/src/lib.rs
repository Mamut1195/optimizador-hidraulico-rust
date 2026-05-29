//! hydro-norms — Norm profiles and validator.
//!
//! Deserializes the 15 norm JSON profiles (CONAGUA_MX, EPA_US, ABNT_BR, …)
//! and validates pipe networks against them. Violation counts are EXACT-match
//! tested against the Python oracle.

pub mod error;
pub mod registry;
pub mod types;
pub mod validator;

pub use error::NormError;
pub use registry::NormRegistry;
pub use types::{
    ElementType, NormProfile, NormRule, NormSource, NormValidationResult, NormViolation,
    ValueBasis,
};
pub use validator::NormValidator;
