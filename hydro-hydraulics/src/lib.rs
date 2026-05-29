//! hydro-hydraulics — Hydraulic computation kernels.
//!
//! Pure-compute crate: Darcy-Weisbach, Hazen-Williams, Manning (full + partial),
//! open-channel, and pump-curve evaluation. No I/O, no global state.
//!
//! Every public function in this crate is EXACT-match tested against the Python
//! oracle (absolute tolerance ≤ 1e-9) via the `tests/oracle` integration harness.

pub mod darcy_weisbach;
pub mod error;
pub mod hazen_williams;
pub mod manning;
pub mod open_channel;
pub mod pump_curves;

pub use error::HydraulicsError;
