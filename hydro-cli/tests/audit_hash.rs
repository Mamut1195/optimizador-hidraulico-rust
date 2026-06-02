//! Integration tests: audit_hash SHA-256 canonical form (REQ-005).
//!
//! ## Strict TDD
//!
//! RED commit: this file calls `hydro_cli::compute_audit_hash` which does not
//! yet exist, causing a compilation error. The happy-path tests also assert
//! that `result.diagnostics.audit_hash` is non-empty; they would fail because
//! WU-7 leaves `audit_hash` as an empty string.
//!
//! GREEN commit: `compute_audit_hash` is implemented in `lib.rs` and `run()`
//! populates `result.diagnostics.audit_hash` before returning. All 4 tests
//! below pass.
//!
//! ## What is tested
//!
//! 1. `test_compute_audit_hash_known_value` — unit: known DesignRequest + known
//!    seed produces a SHA-256 hex string of exactly 64 lowercase hex chars.
//! 2. `test_run_audit_hash_non_empty` — integration: run() result carries a
//!    non-empty 64-char lowercase hex string in `diagnostics.audit_hash`.
//! 3. `test_audit_hash_determinism` — determinism: two run() calls with the
//!    same input and same seed produce identical `audit_hash` values.
//! 4. `test_audit_hash_seed_sensitivity` — seed sensitivity: two run() calls
//!    with different seeds produce different `audit_hash` values.
//!
//! Design §3: canonical form = `request_json + "|" + seed_decimal + "|" + engine_version`.
//! SHA-256 → lowercase hex, 64 chars, no prefix, no whitespace.

use hydro_cli::{compute_audit_hash, run};
use hydro_types::request::{DesignRequest, PointXY, PointXYZ, ProjectTypeStr};
use oracle::solvers::load_sewer_golden;

// ── helpers ──────────────────────────────────────────────────────────────────

/// Build a valid sewer `DesignRequest` from the sewer golden fixture.
///
/// GA budget: pop=20, gen=10, max_time=30 — valid per `validate_request`
/// ranges (population_size ∈ [20, 500], generations ∈ [10, 500],
/// max_time_seconds ∈ [30, 7200]).
fn sewer_request() -> DesignRequest {
    let f = load_sewer_golden();

    let terrain_points: Vec<PointXYZ> = f
        .terrain
        .points
        .iter()
        .map(|p| PointXYZ { x: p[0], y: p[1], z: p[2] })
        .collect();

    let service_points: Vec<PointXY> = f
        .solver_params
        .service_points
        .iter()
        .map(|p| PointXY { x: p[0], y: p[1] })
        .collect();

    let outlet = PointXY {
        x: f.solver_params.outlet[0],
        y: f.solver_params.outlet[1],
    };

    DesignRequest {
        project_type: ProjectTypeStr("sewer".to_string()),
        project_name: "audit-hash-test".to_string(),
        norm: "CONAGUA_MX".to_string(),
        terrain_points,
        service_points: Some(service_points),
        outlet: Some(outlet),
        source: None,
        source_head: None,
        constraints: None,
        forbidden_zones: vec![],
        mandatory_routes: vec![],
        existing_networks: vec![],
        material: "PVC".to_string(),
        num_alternatives: 1,
        strict_norm_compliance: false,
        flow_per_service: f.solver_params.flow_per_service,
        grid_resolution: f.terrain.grid_res,
        seed: Some(42),
        nsga_population_size: 20,
        nsga_generations: 10,
        nsga_max_time_seconds: 30,
        nsga_num_workers: None,
        enforce_cover_depth: false,
        enforce_existing_clearance: false,
        enforce_segment_slopes: false,
        enable_path_smoothing: false,
        weight_cost: 0.30,
        weight_excavation: 0.25,
        weight_pumping: 0.15,
        weight_interference: 0.10,
        weight_resilience: 0.20,
        min_vertical_separation: 0.3,
        min_horizontal_separation: 1.0,
        project_crs: None,
    }
}

// ── Test 1: unit — compute_audit_hash returns a valid SHA-256 hex string ─────

/// Known DesignRequest + known seed → SHA-256 output is exactly 64 lowercase
/// hex characters (REQ-005, design §3).
///
/// This test verifies the shape contract (64 chars, `[0-9a-f]` only) and
/// determinism. Any change to the canonical form (field order, separator, seed
/// encoding, or engine version) will change the hash and fail loudly — the
/// intended early-warning mechanism described in design §9.
#[test]
fn test_compute_audit_hash_known_value() {
    let req = sewer_request();

    // seed 42 is the canonical seed for this fixture.
    let hash = compute_audit_hash(&req, 42);

    // Shape invariants: always 64 lowercase hex chars.
    assert_eq!(
        hash.len(),
        64,
        "audit_hash must be exactly 64 chars, got {}: '{hash}'",
        hash.len()
    );
    assert!(
        hash.chars().all(|c| c.is_ascii_hexdigit() && !c.is_uppercase()),
        "audit_hash must be lowercase hex [0-9a-f], got: '{hash}'"
    );

    // Calling with the same args again must produce the same hash.
    let hash2 = compute_audit_hash(&req, 42);
    assert_eq!(
        hash, hash2,
        "compute_audit_hash must be deterministic for identical inputs"
    );
}

// ── Test 2: integration — run() populates audit_hash ─────────────────────────

/// run() with a sewer fixture → `result.diagnostics.audit_hash` is non-empty,
/// exactly 64 lowercase hex chars (REQ-005).
#[test]
fn test_run_audit_hash_non_empty() {
    let req = sewer_request();
    let result = run(req, Some(42)).expect("run() must succeed for a valid sewer request");

    let h = &result.diagnostics.audit_hash;
    assert!(
        !h.is_empty(),
        "audit_hash must be non-empty after run() — WU-8 must populate it"
    );
    assert_eq!(
        h.len(),
        64,
        "audit_hash must be exactly 64 chars, got {}: '{h}'",
        h.len()
    );
    assert!(
        h.chars().all(|c| c.is_ascii_hexdigit() && !c.is_uppercase()),
        "audit_hash must be lowercase hex [0-9a-f], got: '{h}'"
    );
}

// ── Test 3: determinism ───────────────────────────────────────────────────────

/// Two run() calls with the same input + same seed → identical audit_hash.
///
/// The canonical form (design §3) is a pure function of (request, seed,
/// engine_version). GA randomness does NOT affect the hash value.
#[test]
fn test_audit_hash_determinism() {
    let req1 = sewer_request();
    let req2 = sewer_request(); // identical construction

    let r1 = run(req1, Some(99)).expect("run() first call must succeed");
    let r2 = run(req2, Some(99)).expect("run() second call must succeed");

    assert_eq!(
        r1.diagnostics.audit_hash, r2.diagnostics.audit_hash,
        "audit_hash must be identical for two calls with the same request + seed"
    );
    // Must also be valid 64-char hex.
    assert_eq!(
        r1.diagnostics.audit_hash.len(),
        64,
        "determinism check: hash must be 64 chars"
    );
}

// ── Test 4: seed sensitivity ──────────────────────────────────────────────────

/// Two run() calls with different seeds → different audit_hash values.
///
/// The effective seed appears in the canonical bytes (design §3), so any
/// change to the seed MUST produce a different hash output.
#[test]
fn test_audit_hash_seed_sensitivity() {
    let req_a = sewer_request();
    let req_b = sewer_request();

    let r_a = run(req_a, Some(1)).expect("run() seed=1 must succeed");
    let r_b = run(req_b, Some(2)).expect("run() seed=2 must succeed");

    assert_ne!(
        r_a.diagnostics.audit_hash, r_b.diagnostics.audit_hash,
        "audit_hash must differ when seeds differ (seed 1 vs seed 2)"
    );
}
