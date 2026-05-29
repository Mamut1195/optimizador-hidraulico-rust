//! hydro-types — Domain types and contracts for the hydraulic optimization engine.
//!
//! Foundation crate. No algorithm logic lives here — only structs, enums,
//! newtypes, serde contracts, and domain-level validation that every other
//! crate in the workspace depends on.

pub mod constraints;
pub mod enums;
pub mod error;
pub mod network;
pub mod request;
pub mod response;

pub use constraints::DesignConstraints;
pub use enums::{FlowType, NodeType, PipeMaterial, ProjectType, Severity};
pub use error::HydroTypesError;
pub use network::{NetworkNode, NetworkPipe, NodeId, PipeId, PipeNetwork};
pub use request::DesignRequest;
pub use response::{DesignResult, Diagnostics, Solution, SolutionScore};

/// Re-export commonly-used types so downstream crates can use `hydro_types::prelude::*`.
pub mod prelude {
    pub use crate::constraints::DesignConstraints;
    pub use crate::enums::{FlowType, NodeType, PipeMaterial, ProjectType, Severity};
    pub use crate::error::HydroTypesError;
    pub use crate::network::{NetworkNode, NetworkPipe, NodeId, PipeId, PipeNetwork};
    pub use crate::request::DesignRequest;
    pub use crate::response::{DesignResult, Diagnostics, Solution, SolutionScore};
}
