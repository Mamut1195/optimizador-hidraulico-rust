//! REQ-016 parity test skeleton — awaiting contracts/examples/sewer_basic fixture.
//!
//! When `contracts/examples/sewer_basic/` fixture lands, remove `#[ignore]` from
//! `parity_hv_igdplus_sewer_basic` to gate REQ-016. Until then, the test is
//! `#[ignore = "fixture-missing: oracle JSON not yet checked into contracts/examples/sewer_basic"]`
//! so it shows as `ignored` in CI (not failed) and provides a named test target
//! for the requirement.
//!
//! ## To activate (checklist when fixture arrives)
//! 1. Add `contracts/examples/sewer_basic/oracle_golden.json` to the repo.
//! 2. Remove the `#[ignore]` attribute from `parity_hv_igdplus_sewer_basic`.
//! 3. Replace `placeholder_front` with the actual Pareto front from the optimizer.
//! 4. Load oracle golden HV/IGD+ from `oracle_golden.json` instead of placeholders.

// ── Local metric helpers (mirrors metrics.rs — avoids pub surface exposure) ──

/// 2-D hypervolume by sorted sweep. Returns 0 for M != 2.
fn hv_2d(front: &[Vec<f64>], reference_point: &[f64]) -> f64 {
    if front.is_empty() || reference_point.len() != 2 {
        return 0.0;
    }
    let mut pts: Vec<(f64, f64)> = front
        .iter()
        .filter_map(|v| {
            if v.len() >= 2 {
                Some((v[0], v[1]))
            } else {
                None
            }
        })
        .collect();
    if pts.is_empty() {
        return 0.0;
    }
    pts.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let (ref_x, ref_y) = (reference_point[0], reference_point[1]);
    let mut hv = 0.0_f64;
    let mut prev_x = ref_x;
    for (x, y) in pts.iter().rev() {
        let w = prev_x - x;
        let h = ref_y - y;
        if w > 0.0 && h > 0.0 {
            hv += w * h;
        }
        prev_x = *x;
    }
    hv
}

/// IGD+ per Ishibuchi 2015.
fn igdplus(front: &[Vec<f64>], reference_front: &[Vec<f64>]) -> f64 {
    if front.is_empty() || reference_front.is_empty() {
        return f64::INFINITY;
    }
    let sum: f64 = reference_front
        .iter()
        .map(|r| {
            front
                .iter()
                .map(|a| {
                    r.iter()
                        .zip(a.iter())
                        .map(|(&rj, &aj)| {
                            let d = rj - aj;
                            if d > 0.0 {
                                d * d
                            } else {
                                0.0
                            }
                        })
                        .sum::<f64>()
                        .sqrt()
                })
                .fold(f64::INFINITY, f64::min)
        })
        .sum();
    sum / reference_front.len() as f64
}

// ── Parity test ───────────────────────────────────────────────────────────────

/// REQ-016 parity gate: HV and IGD+ of the Rust optimizer on the sewer_basic
/// fixture must be within ±10% / ±15% of the Python oracle golden values.
///
/// The test body panics with a clear message if the fixture is absent.
#[test]
#[ignore = "fixture-missing: oracle JSON not yet checked into contracts/examples/sewer_basic"]
fn parity_hv_igdplus_sewer_basic() {
    // ── Fixture guard ─────────────────────────────────────────────────────────
    let fixture_dir =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../contracts/examples/sewer_basic");

    if !fixture_dir.exists() {
        panic!(
            "Fixture directory not found: {}. \
             Check out contracts/examples/sewer_basic/ before un-ignoring this test.",
            fixture_dir.display()
        );
    }

    // ── Pareto front from optimizer ───────────────────────────────────────────
    // TODO: run GeneticOptimizer::optimize(seed=44_001) with the fixture terrain
    // and capture results.comparison_table() as `front`.
    // Placeholder two-point front used so the test body compiles cleanly.
    let front: Vec<Vec<f64>> = vec![
        vec![1.0e6, 200.0, 50_000.0, 2.0, 0.3],
        vec![1.5e6, 100.0, 30_000.0, 1.0, 0.1],
    ];

    // ── Load oracle golden values ─────────────────────────────────────────────
    // TODO: deserialize oracle_hv, oracle_igdp, reference_point, and oracle_front
    // from fixture_dir/oracle_golden.json.
    let oracle_hv = 1.0_f64; // placeholder
    let oracle_igdp = 1.0_f64; // placeholder
    let reference_point: Vec<f64> = vec![1e13, 1e13]; // placeholder (2-D for now)
    let oracle_front: Vec<Vec<f64>> = front.clone(); // placeholder (perfect parity)

    // ── Compute metrics ───────────────────────────────────────────────────────
    let hv_rust = hv_2d(&front, &reference_point);
    let igdp_rust = igdplus(&front, &oracle_front);

    // ── Assert parity (REQ-016 tolerances) ───────────────────────────────────
    assert!(
        (hv_rust - oracle_hv).abs() / oracle_hv.max(1e-12) <= 0.10,
        "HV parity failed: rust={hv_rust:.4e}, oracle={oracle_hv:.4e} (>10% diff)"
    );

    assert!(
        (igdp_rust - oracle_igdp).abs() / oracle_igdp.max(1e-12) <= 0.15,
        "IGD+ parity failed: rust={igdp_rust:.4e}, oracle={oracle_igdp:.4e} (>15% diff)"
    );
}

// ── REQ-012 PathSmoother path-length parity ──────────────────────────────────

/// REQ-012 parity gate: total length of the Rust PathSmoother's output on the
/// sewer_basic fixture must be within ±2% of the Python `path_smoother.py`
/// oracle's output on the same input.
///
/// The test body panics with a clear message if the fixture is absent.
///
/// ## To activate
/// 1. Add `contracts/examples/sewer_basic/oracle_path.json` with fields:
///    `waypoints` (Vec<[f64; 2]>), `obstacles` (Vec<Polygon>), `oracle_length`
///    (f64), `clearance_distance` (f64), `rdp_epsilon` (f64).
/// 2. Remove the `#[ignore]` attribute from `parity_path_length_sewer_basic`.
/// 3. Replace the placeholder front with a real call to `PathSmoother::smooth`.
#[test]
#[ignore = "fixture-missing: oracle path JSON not yet checked into contracts/examples/sewer_basic"]
fn parity_path_length_sewer_basic() {
    let fixture_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../contracts/examples/sewer_basic/oracle_path.json");

    if !fixture_path.exists() {
        panic!(
            "Fixture not found: {}. \
             Generate from Python `path_smoother.py` on the sewer_basic case \
             before un-ignoring this test.",
            fixture_path.display()
        );
    }

    // TODO: deserialize waypoints, obstacles, clearance, rdp_epsilon, oracle_length
    // from oracle_path.json. Call PathSmoother::new(clearance, rdp_epsilon)
    // then .smooth(&waypoints, &obstacles). Compute total polyline length.
    let oracle_length = 1.0_f64; // placeholder
    let rust_length = 1.0_f64; // placeholder

    assert!(
        (rust_length - oracle_length).abs() / oracle_length.max(1e-12) <= 0.02,
        "Path length parity failed: rust={rust_length:.6}, oracle={oracle_length:.6} (>2% diff)"
    );
}
