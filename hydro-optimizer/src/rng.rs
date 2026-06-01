//! RNG seeding helpers for deterministic multi-threaded optimization.
//!
//! Design §5: One root `ChaCha20Rng` seeded from `config.seed`.
//! Per-rayon-task child RNGs are derived deterministically via splitmix64
//! keyed by `(generation, individual_index)`.
//!
//! Guarantees bit-identical Pareto fronts across re-runs with the same seed.

use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;

/// Construct the root RNG seeded from a u64.
///
/// Used to initialize the optimizer's main-thread RNG.
pub fn root_rng(seed: u64) -> ChaCha20Rng {
    ChaCha20Rng::seed_from_u64(seed)
}

/// Derive a deterministic child RNG keyed by `(generation, individual_idx)`.
///
/// Uses splitmix64 mixing so adjacent `(gen, idx)` values diverge rapidly.
/// This ensures rayon workers get independent, reproducible streams.
///
/// Design §5 exact formula.
pub fn child_rng(root_seed: u64, generation: u32, individual_idx: u32) -> ChaCha20Rng {
    let mut s = root_seed
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add((generation as u64) << 32)
        .wrapping_add(individual_idx as u64);
    // splitmix64
    s = (s ^ (s >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    s = (s ^ (s >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    s ^= s >> 31;
    ChaCha20Rng::seed_from_u64(s)
}
