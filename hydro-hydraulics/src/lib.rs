//! hydro-hydraulics — Hydraulic computation kernels.
//!
//! Pure-compute crate: Darcy-Weisbach, Hazen-Williams, Manning (full + partial),
//! open-channel, and pump-curve evaluation. No I/O, no global state.
//!
//! Every public function in this crate is EXACT-match tested against the Python
//! oracle (absolute tolerance ≤ 1e-9) via the `tests/oracle` integration harness.

#[cfg(test)]
mod tests {
    #[test]
    fn workspace_member_compiles() {
        // The test passing confirms compilation succeeded.
    }
}
