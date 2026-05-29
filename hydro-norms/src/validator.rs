//! NormValidator — stub for T-3.1 RED phase.
//! Full implementation is in T-3.3.

use crate::types::{NormProfile, NormValidationResult};
use hydro_types::{PipeNetwork, ProjectType};

/// Validates a pipe network against a norm profile.
pub struct NormValidator {
    pub profile: NormProfile,
}

impl NormValidator {
    pub fn new(profile: NormProfile) -> Self {
        NormValidator { profile }
    }

    pub fn validate(
        &self,
        _network: &PipeNetwork,
        _project_type: ProjectType,
    ) -> NormValidationResult {
        unimplemented!("NormValidator::validate — implement in T-3.3")
    }
}
