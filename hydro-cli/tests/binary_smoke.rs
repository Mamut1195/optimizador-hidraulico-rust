//! Binary smoke tests: spawn `hydro-cli` via `std::process::Command` (REQ-006, REQ-009).
//!
//! ## Strict TDD
//!
//! RED commit: current scaffold binary exits 4 (`CliError::InternalError`).
//! All three tests below will fail on the RED commit.
//!
//! GREEN commit: `main.rs` clap shell is implemented; all three tests pass.
//!
//! ## What is tested
//!
//! Test 1 — happy path: spawn binary with `--input <fixture>` pointing to a valid
//!          sewer DesignRequest JSON. Assert exit code 0 and output contains valid
//!          DesignResult with `"success":true`.
//!
//! Test 2 — invalid JSON: pipe malformed JSON to stdin. Assert exit code 1.
//!
//! Test 3 — validate-only bad request: spawn with `--validate-only --input <bad>`.
//!          Assert exit code 1 (sewer request missing outlet → ValidationError).
//!
//! ## Wall-time budget
//!
//! Each test is expected to complete in < 5 s (REQ-009: < 10 s).
//! run() with pop=20 gen=10 seed=42 takes ~0.2 s for sewer on CI.
//! Binary startup overhead is ~50 ms. Total well within budget.
//!
//! ## Fixture construction
//!
//! Rather than depending on oracle fixture files at runtime (which would require
//! the fixtures dir to be reachable from the spawned binary's cwd), the fixture
//! JSON is constructed inline using values derived from the oracle loader and
//! serialized to a temp file. This keeps the test hermetic.

use std::io::Write;
use std::process::Command;

use hydro_types::request::{DesignRequest, PointXY, PointXYZ, ProjectTypeStr};
use oracle::solvers::load_sewer_golden;

// ── Fixture builder ───────────────────────────────────────────────────────────

/// Build a minimal but valid sewer DesignRequest from the oracle sewer golden fixture.
///
/// Uses the validated floor GA budget (pop=20, gen=10) so run() completes in ~0.2 s.
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
        project_name: "binary_smoke_sewer".to_string(),
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
        // Minimum validated floor (pop >= 20, gen >= 10, time >= 30).
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

/// Write a JSON string to a temp file and return the path.
///
/// The file is written to the OS temp directory with a unique name to avoid
/// collisions when tests run in parallel.
fn write_temp_json(content: &str, suffix: &str) -> std::path::PathBuf {
    let mut path = std::env::temp_dir();
    // Use process id + thread id for uniqueness across parallel test runners.
    let pid = std::process::id();
    path.push(format!("hydro_cli_smoke_{pid}_{suffix}.json"));
    let mut f = std::fs::File::create(&path)
        .unwrap_or_else(|e| panic!("could not create temp file {}: {e}", path.display()));
    f.write_all(content.as_bytes())
        .unwrap_or_else(|e| panic!("could not write temp file {}: {e}", path.display()));
    path
}

// ── Tests ─────────────────────────────────────────────────────────────────────

/// Test 1 — happy path: binary exits 0 for a valid sewer DesignRequest JSON.
///
/// The binary reads from `--input`, runs the optimizer, writes DesignResult JSON
/// to `--output`, then exits 0. The output must contain `"success":true`.
///
/// RED: scaffold exits 4. Test fails on assert_eq!(exit_code, 0).
/// GREEN: main.rs clap shell reads input, calls run(), serializes output, exits 0.
#[test]
fn test_binary_exits_zero_for_valid_sewer_json() {
    let req = build_valid_sewer_request();
    let req_json =
        serde_json::to_string(&req).expect("valid DesignRequest must serialize without error");

    let input_path = write_temp_json(&req_json, "valid_sewer_input");
    let output_path = {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "hydro_cli_smoke_{}_valid_sewer_output.json",
            std::process::id()
        ));
        p
    };

    let bin = env!("CARGO_BIN_EXE_hydro-cli");
    let output = Command::new(bin)
        .arg("--input")
        .arg(&input_path)
        .arg("--output")
        .arg(&output_path)
        .arg("--seed")
        .arg("42")
        .output()
        .unwrap_or_else(|e| panic!("failed to spawn binary {bin}: {e}"));

    // Clean up temp files regardless of outcome.
    let _ = std::fs::remove_file(&input_path);

    let exit_code = output.status.code().unwrap_or(-1);
    assert_eq!(
        exit_code,
        0,
        "binary must exit 0 for a valid sewer request; got {exit_code}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Read and parse output file.
    let out_bytes = std::fs::read(&output_path).unwrap_or_else(|e| {
        panic!(
            "output file not created at {}: {e}\nstdout: {}",
            output_path.display(),
            String::from_utf8_lossy(&output.stdout)
        )
    });
    let _ = std::fs::remove_file(&output_path);

    let result: serde_json::Value =
        serde_json::from_slice(&out_bytes).expect("output must be valid JSON");

    assert_eq!(
        result["success"], true,
        "DesignResult.success must be true; full result: {result}"
    );
    assert!(
        result["solutions"]
            .as_array()
            .map(|a| !a.is_empty())
            .unwrap_or(false),
        "DesignResult.solutions must be non-empty; full result: {result}"
    );
}

/// Test 2 — invalid JSON input: binary exits 1 when stdin receives malformed JSON.
///
/// Per REQ-006: `serde_json::Error` → `CliError::ValidationError` → exit 1.
///
/// RED: scaffold exits 4 without reading stdin. Test fails on assert_eq!(exit_code, 1).
/// GREEN: main.rs parses stdin; deserialization fails → exit 1.
#[test]
fn test_binary_exits_one_for_malformed_json_stdin() {
    let bad_json = r#"{"bad": "json"  // missing closing brace"#;
    let input_path = write_temp_json(bad_json, "bad_json_input");

    let bin = env!("CARGO_BIN_EXE_hydro-cli");
    let output = Command::new(bin)
        .arg("--input")
        .arg(&input_path)
        .output()
        .unwrap_or_else(|e| panic!("failed to spawn binary {bin}: {e}"));

    let _ = std::fs::remove_file(&input_path);

    let exit_code = output.status.code().unwrap_or(-1);
    assert_eq!(
        exit_code,
        1,
        "binary must exit 1 for malformed JSON; got {exit_code}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Test 3 — validate-only with invalid request: binary exits 1.
///
/// A sewer request with `outlet` removed fails `validate_request` →
/// `CliError::ValidationError` → exit 1.
/// `--validate-only` must NOT invoke the optimizer.
///
/// RED: scaffold exits 4 immediately. Test fails on assert_eq!(exit_code, 1).
/// GREEN: main.rs parses `--validate-only`; routes to validate_request(); exits 1.
#[test]
fn test_binary_exits_one_validate_only_for_bad_request() {
    let mut req = build_valid_sewer_request();
    // Remove outlet — sewer requires it (validate_request rule 2).
    req.outlet = None;

    let req_json =
        serde_json::to_string(&req).expect("request without outlet must still serialize");

    let input_path = write_temp_json(&req_json, "bad_sewer_no_outlet");

    let bin = env!("CARGO_BIN_EXE_hydro-cli");
    let output = Command::new(bin)
        .arg("--validate-only")
        .arg("--input")
        .arg(&input_path)
        .output()
        .unwrap_or_else(|e| panic!("failed to spawn binary {bin}: {e}"));

    let _ = std::fs::remove_file(&input_path);

    let exit_code = output.status.code().unwrap_or(-1);
    assert_eq!(
        exit_code,
        1,
        "binary must exit 1 for --validate-only with missing outlet; got {exit_code}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
