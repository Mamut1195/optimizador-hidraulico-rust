//! Integration tests for `--validate-only` fast path (REQ-007).
//!
//! ## Strict TDD
//!
//! RED commit: WU-6 already routes `--validate-only` to `validate_request()` and
//! exits with the correct code, but emits **no JSON to stdout**. Tests 1 and 2
//! will fail because they assert a `{"status":"valid",...}` / `{"status":"invalid",...}`
//! JSON object on stdout. Tests 3 and 4 (timing) will PASS on RED since the fast
//! path already exists — this is acceptable because the test file itself is the new
//! artifact (it didn't exist on the parent branch).
//!
//! GREEN commit: `main.rs` `--validate-only` path emits minimal status JSON to
//! stdout; tests 1 and 2 now pass.
//!
//! ## What is tested
//!
//! - Test 1 (status valid): binary exits 0 AND stdout is `{"status":"valid","project_type":"sewer"}`.
//! - Test 2 (status invalid): binary exits 1 AND stdout JSON has `"status":"invalid"` with an `"error"` key.
//! - Test 3 (timing — valid): wall time for `--validate-only` on a valid request is under 100 ms.
//! - Test 4 (timing — invalid): wall time for `--validate-only` on an invalid request is under 100 ms.
//!
//! ## Design note (§7 fast-path target)
//!
//! Design §7 tightens the budget to **50 ms**. Spec REQ-007 states 100 ms. The
//! implementation target is 50 ms; the test hard-fails at 100 ms (the spec ceiling)
//! to avoid platform-flaky failures on CI while still enforcing the spirit of the
//! requirement. Any timing between 50–100 ms should prompt investigation.

use std::io::Write;
use std::time::Instant;

use hydro_types::request::{DesignRequest, PointXY, PointXYZ, ProjectTypeStr};
use oracle::solvers::load_sewer_golden;

// ── Fixture builder ───────────────────────────────────────────────────────────

/// Build a minimal but valid sewer `DesignRequest` from the oracle sewer golden fixture.
///
/// GA budget is kept at the minimum validated floor (pop=20, gen=10) so that if
/// someone accidentally routes to `run()`, the test wall time is still bounded.
/// Under `--validate-only`, these fields are ignored.
fn build_valid_sewer_request() -> DesignRequest {
    let f = load_sewer_golden();

    let terrain_points: Vec<PointXYZ> = f
        .terrain
        .points
        .iter()
        .map(|p| PointXYZ {
            x: p[0],
            y: p[1],
            z: p[2],
        })
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
        project_name: "validate_only_sewer".to_string(),
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
        norm: "CONAGUA_MX".to_string(),
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

/// Write a JSON string to a temp file; return the path.
fn write_temp_json(content: &str, suffix: &str) -> std::path::PathBuf {
    let mut path = std::env::temp_dir();
    let pid = std::process::id();
    path.push(format!("hydro_cli_vo_{pid}_{suffix}.json"));
    let mut f = std::fs::File::create(&path)
        .unwrap_or_else(|e| panic!("could not create temp file {}: {e}", path.display()));
    f.write_all(content.as_bytes())
        .unwrap_or_else(|e| panic!("could not write temp file {}: {e}", path.display()));
    path
}

// ── Tests ─────────────────────────────────────────────────────────────────────

/// Test 1 — status JSON on valid request.
///
/// `--validate-only` with a valid sewer fixture MUST:
/// - exit 0
/// - write `{"status":"valid","project_type":"sewer"}` to stdout
///
/// RED: WU-6 exits 0 but emits nothing to stdout → JSON parse fails → RED.
/// GREEN: main.rs emits status JSON before exiting 0.
#[test]
fn test_validate_only_exits_zero_and_emits_valid_status_json() {
    let req = build_valid_sewer_request();
    let req_json =
        serde_json::to_string(&req).expect("valid DesignRequest must serialize without error");
    let input_path = write_temp_json(&req_json, "valid_sewer_vo");

    let bin = env!("CARGO_BIN_EXE_hydro-cli");
    let output = std::process::Command::new(bin)
        .arg("--validate-only")
        .arg("--input")
        .arg(&input_path)
        .output()
        .unwrap_or_else(|e| panic!("failed to spawn binary {bin}: {e}"));

    let _ = std::fs::remove_file(&input_path);

    let exit_code = output.status.code().unwrap_or(-1);
    assert_eq!(
        exit_code, 0,
        "--validate-only must exit 0 for valid request; got {exit_code}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // WU-7 contract: stdout must be parseable JSON with "status": "valid".
    let stdout_str = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(stdout_str.trim()).unwrap_or_else(|e| {
        panic!(
            "--validate-only must emit JSON to stdout; got {:?}; parse error: {e}",
            stdout_str
        )
    });

    assert_eq!(
        json["status"], "valid",
        "status field must be 'valid'; full JSON: {json}"
    );
    assert_eq!(
        json["project_type"], "sewer",
        "project_type field must match the request; full JSON: {json}"
    );
}

/// Test 2 — status JSON on invalid request.
///
/// `--validate-only` with a sewer request missing `outlet` MUST:
/// - exit 1
/// - write JSON to stdout with `"status":"invalid"` and an `"error"` key
///
/// RED: WU-6 exits 1 (correct) but emits nothing to stdout → JSON parse fails → RED.
/// GREEN: main.rs emits `{"status":"invalid","error":"..."}` before exiting 1.
#[test]
fn test_validate_only_exits_one_and_emits_invalid_status_json() {
    let mut req = build_valid_sewer_request();
    // Remove outlet — sewer requires it; validate_request will return Err.
    req.outlet = None;

    let req_json =
        serde_json::to_string(&req).expect("request without outlet must still serialize");
    let input_path = write_temp_json(&req_json, "bad_sewer_no_outlet_vo");

    let bin = env!("CARGO_BIN_EXE_hydro-cli");
    let output = std::process::Command::new(bin)
        .arg("--validate-only")
        .arg("--input")
        .arg(&input_path)
        .output()
        .unwrap_or_else(|e| panic!("failed to spawn binary {bin}: {e}"));

    let _ = std::fs::remove_file(&input_path);

    let exit_code = output.status.code().unwrap_or(-1);
    assert_eq!(
        exit_code, 1,
        "--validate-only must exit 1 for invalid request; got {exit_code}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // WU-7 contract: stdout must be parseable JSON with "status": "invalid".
    let stdout_str = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(stdout_str.trim()).unwrap_or_else(|e| {
        panic!(
            "--validate-only must emit JSON to stdout on failure; got {:?}; parse error: {e}",
            stdout_str
        )
    });

    assert_eq!(
        json["status"], "invalid",
        "status field must be 'invalid'; full JSON: {json}"
    );
    assert!(
        json.get("error").is_some(),
        "JSON must contain an 'error' key; full JSON: {json}"
    );
}

/// Test 3 — timing assertion on valid request.
///
/// `--validate-only` must complete in under 100 ms wall time (spec REQ-007).
/// Design §7 tightens the internal target to 50 ms; this test enforces the
/// spec ceiling at 100 ms to stay CI-safe while still catching optimizer
/// accidentally being invoked.
///
/// Note: this test will pass on RED (timing is fine in WU-6 too), so the
/// genuine RED is supplied by tests 1 and 2 (status JSON assertion).
#[test]
fn test_validate_only_completes_under_100ms_valid() {
    let req = build_valid_sewer_request();
    let req_json =
        serde_json::to_string(&req).expect("valid DesignRequest must serialize without error");
    let input_path = write_temp_json(&req_json, "timing_valid_vo");

    let bin = env!("CARGO_BIN_EXE_hydro-cli");

    let t0 = Instant::now();
    let output = std::process::Command::new(bin)
        .arg("--validate-only")
        .arg("--input")
        .arg(&input_path)
        .output()
        .unwrap_or_else(|e| panic!("failed to spawn binary {bin}: {e}"));
    let elapsed = t0.elapsed();

    let _ = std::fs::remove_file(&input_path);

    let exit_code = output.status.code().unwrap_or(-1);
    assert_eq!(
        exit_code, 0,
        "binary must exit 0 for valid request; got {exit_code}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // REQ-007: < 100 ms ceiling. Design §7 internal target: 50 ms.
    // Measure includes binary startup (~50 ms); pure validate_request is < 1 ms.
    assert!(
        elapsed.as_millis() < 100,
        "--validate-only must complete under 100 ms (REQ-007); elapsed: {} ms\n\
         Hint: if this exceeds 50 ms without optimizer, suspect heavy init code.",
        elapsed.as_millis()
    );
}

/// Test 4 — timing assertion on invalid request.
///
/// `--validate-only` must also complete in under 100 ms when the request is
/// invalid (validation error path). Exit 1 but still fast.
#[test]
fn test_validate_only_completes_under_100ms_invalid() {
    let mut req = build_valid_sewer_request();
    req.outlet = None; // Force validation failure

    let req_json =
        serde_json::to_string(&req).expect("request without outlet must still serialize");
    let input_path = write_temp_json(&req_json, "timing_invalid_vo");

    let bin = env!("CARGO_BIN_EXE_hydro-cli");

    let t0 = Instant::now();
    let output = std::process::Command::new(bin)
        .arg("--validate-only")
        .arg("--input")
        .arg(&input_path)
        .output()
        .unwrap_or_else(|e| panic!("failed to spawn binary {bin}: {e}"));
    let elapsed = t0.elapsed();

    let _ = std::fs::remove_file(&input_path);

    let exit_code = output.status.code().unwrap_or(-1);
    assert_eq!(
        exit_code, 1,
        "binary must exit 1 for invalid request; got {exit_code}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // REQ-007: < 100 ms ceiling.
    assert!(
        elapsed.as_millis() < 100,
        "--validate-only (error path) must complete under 100 ms (REQ-007); elapsed: {} ms",
        elapsed.as_millis()
    );
}
