//! NormRegistry — stub for T-3.1 RED phase.
//! Full implementation is in T-3.2.

use crate::error::NormError;
use crate::types::NormProfile;

/// Registry for known norm profiles.
pub struct NormRegistry;

impl NormRegistry {
    /// Resolve a norm profile by canonical code or alias.
    pub fn get(_code: &str) -> Result<NormProfile, NormError> {
        unimplemented!("NormRegistry::get — implement in T-3.2")
    }
}
