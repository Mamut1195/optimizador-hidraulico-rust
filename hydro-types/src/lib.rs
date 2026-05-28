//! hydro-types — Domain types and contracts for the hydraulic optimization engine.
//!
//! Foundation crate. No algorithm logic lives here — only structs, enums,
//! newtypes, serde contracts, and domain-level validation that every other
//! crate in the workspace depends on.

/// Re-export commonly-used types so downstream crates can use `hydro_types::*`.
pub mod prelude {}

#[cfg(test)]
mod tests {
    #[test]
    fn workspace_member_compiles() {
        // Trivial smoke test: the crate skeleton compiles and links.
        // The test passing confirms compilation succeeded.
    }
}
