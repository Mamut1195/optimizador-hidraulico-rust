//! Error types for the hydro-norms crate.

use thiserror::Error;

/// Errors produced by the norm registry and validator.
#[derive(Debug, Error)]
pub enum NormError {
    #[error("unknown norm profile code: '{code}'")]
    UnknownNorm { code: String },

    #[error("failed to load profile '{file}': {source}")]
    ProfileLoadError {
        file: String,
        #[source]
        source: serde_json::Error,
    },

    #[error("copy_from chain is not supported: '{profile}'")]
    CopyFromChain { profile: String },
}
