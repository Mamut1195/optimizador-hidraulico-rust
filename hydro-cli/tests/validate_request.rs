//! Integration tests for `validate_request` — REQ-002.
//!
//! Tests cover per-solver presence checks and cross-field validations as defined
//! in design §2 and spec REQ-002.
//!
//! RED phase: these tests reference `hydro_cli::validate_request` whose body is
//! `todo!()`. Every test that reaches the function will panic with "not yet
//! implemented". Tests that construct invalid requests at the serde level will
//! panic even earlier (deserialization). The RED commit is expected to fail
//! `cargo test` on every case below.

use hydro_cli::{validate_request, CliError};
use hydro_types::request::DesignRequest;
use serde_json::json;

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Minimal valid terrain (3 points) as a JSON array — required for all requests.
fn terrain() -> serde_json::Value {
    json!([
        {"x": 0.0, "y": 0.0,  "z": 10.0},
        {"x": 50.0, "y": 0.0,  "z": 9.0},
        {"x": 50.0, "y": 50.0, "z": 8.5}
    ])
}

fn parse(v: serde_json::Value) -> DesignRequest {
    serde_json::from_value(v).expect("fixture JSON must deserialize cleanly")
}

// ── Happy-path tests ──────────────────────────────────────────────────────────

/// A fully-valid sewer request must return Ok(()) — REQ-002 happy path.
#[test]
fn test_validate_accepts_valid_sewer_request() {
    let req = parse(json!({
        "project_type": "sewer",
        "terrain_points": terrain(),
        "outlet":         {"x": 0.0, "y": 0.0},
        "service_points": [
            {"x": 10.0, "y": 10.0},
            {"x": 20.0, "y": 20.0}
        ]
    }));
    assert!(validate_request(&req).is_ok());
}

/// A fully-valid water_supply request must return Ok(()).
#[test]
fn test_validate_accepts_valid_water_supply_request() {
    let req = parse(json!({
        "project_type": "water_supply",
        "terrain_points": terrain(),
        "source":         {"x": 0.0, "y": 0.0},
        "service_points": [
            {"x": 10.0, "y": 10.0},
            {"x": 20.0, "y": 20.0}
        ]
    }));
    assert!(validate_request(&req).is_ok());
}

/// A fully-valid conveyance request must return Ok(()).
/// Conveyance uses `source` + `outlet` (outlet is treated as destination).
#[test]
fn test_validate_accepts_valid_conveyance_request() {
    let req = parse(json!({
        "project_type": "conveyance",
        "terrain_points": terrain(),
        "source": {"x": 0.0, "y": 0.0},
        "outlet": {"x": 50.0, "y": 50.0}
    }));
    assert!(validate_request(&req).is_ok());
}

/// A fully-valid distribution request (>= 2 demand points) must return Ok(()).
#[test]
fn test_validate_accepts_valid_distribution_request() {
    let req = parse(json!({
        "project_type": "distribution",
        "terrain_points": terrain(),
        "source":         {"x": 0.0, "y": 0.0},
        "service_points": [
            {"x": 10.0, "y": 10.0},
            {"x": 30.0, "y": 30.0}
        ]
    }));
    assert!(validate_request(&req).is_ok());
}

/// A fully-valid pump_station request must return Ok(()).
#[test]
fn test_validate_accepts_valid_pump_station_request() {
    let req = parse(json!({
        "project_type": "pump_station",
        "terrain_points": terrain(),
        "source": {"x": 0.0,  "y": 0.0},
        "outlet": {"x": 50.0, "y": 50.0}
    }));
    assert!(validate_request(&req).is_ok());
}

/// A fully-valid intake request must return Ok(()).
#[test]
fn test_validate_accepts_valid_intake_request() {
    let req = parse(json!({
        "project_type": "intake",
        "terrain_points": terrain(),
        "source": {"x": 0.0, "y": 0.0}
    }));
    assert!(validate_request(&req).is_ok());
}

// ── Error-path: sewer ─────────────────────────────────────────────────────────

/// Sewer without outlet → ValidationError (exit 1) — REQ-002 §sewer.
#[test]
fn test_validate_rejects_sewer_without_outlet() {
    let req = parse(json!({
        "project_type": "sewer",
        "terrain_points": terrain(),
        "service_points": [{"x": 10.0, "y": 10.0}, {"x": 20.0, "y": 20.0}]
        // outlet intentionally absent
    }));
    let err = validate_request(&req).expect_err("should fail — outlet missing");
    assert_eq!(err.exit_code(), 1, "missing field must exit 1");
}

/// Sewer without service_points → ValidationError (exit 1).
#[test]
fn test_validate_rejects_sewer_without_service_points() {
    let req = parse(json!({
        "project_type": "sewer",
        "terrain_points": terrain(),
        "outlet": {"x": 0.0, "y": 0.0}
        // service_points intentionally absent
    }));
    let err = validate_request(&req).expect_err("should fail — service_points missing");
    assert_eq!(err.exit_code(), 1);
}

/// Sewer with an empty service_points list → ValidationError (exit 1).
#[test]
fn test_validate_rejects_sewer_with_empty_service_points() {
    let req = parse(json!({
        "project_type": "sewer",
        "terrain_points": terrain(),
        "outlet":         {"x": 0.0, "y": 0.0},
        "service_points": []
    }));
    let err = validate_request(&req).expect_err("should fail — service_points empty");
    assert_eq!(err.exit_code(), 1);
}

// ── Error-path: water_supply ──────────────────────────────────────────────────

/// water_supply without source → ValidationError (exit 1) — REQ-002 §water_supply.
#[test]
fn test_validate_rejects_water_supply_without_source() {
    let req = parse(json!({
        "project_type": "water_supply",
        "terrain_points": terrain(),
        "service_points": [{"x": 10.0, "y": 10.0}, {"x": 20.0, "y": 20.0}]
        // source intentionally absent
    }));
    let err = validate_request(&req).expect_err("should fail — source missing");
    assert_eq!(err.exit_code(), 1);
}

/// water_supply without service_points → ValidationError (exit 1).
#[test]
fn test_validate_rejects_water_supply_without_service_points() {
    let req = parse(json!({
        "project_type": "water_supply",
        "terrain_points": terrain(),
        "source": {"x": 0.0, "y": 0.0}
        // service_points intentionally absent
    }));
    let err = validate_request(&req).expect_err("should fail — service_points missing");
    assert_eq!(err.exit_code(), 1);
}

// ── Error-path: conveyance ────────────────────────────────────────────────────

/// Conveyance without source → ValidationError (exit 1).
#[test]
fn test_validate_rejects_conveyance_without_source() {
    let req = parse(json!({
        "project_type": "conveyance",
        "terrain_points": terrain(),
        "outlet": {"x": 50.0, "y": 50.0}
        // source intentionally absent
    }));
    let err = validate_request(&req).expect_err("should fail — source missing");
    assert_eq!(err.exit_code(), 1);
}

/// Conveyance without outlet (destination) → ValidationError (exit 1).
#[test]
fn test_validate_rejects_conveyance_without_destination() {
    let req = parse(json!({
        "project_type": "conveyance",
        "terrain_points": terrain(),
        "source": {"x": 0.0, "y": 0.0}
        // outlet intentionally absent — outlet is used as destination for conveyance
    }));
    let err = validate_request(&req).expect_err("should fail — outlet/destination missing");
    assert_eq!(err.exit_code(), 1);
}

// ── Error-path: distribution ──────────────────────────────────────────────────

/// Distribution with exactly one demand point → ValidationError (exit 1).
/// Requires >= 2 demand points; one point is explicitly rejected — design §2 rule 3.
#[test]
fn test_validate_rejects_distribution_with_one_point() {
    let req = parse(json!({
        "project_type": "distribution",
        "terrain_points": terrain(),
        "source":         {"x": 0.0, "y": 0.0},
        "service_points": [{"x": 10.0, "y": 10.0}]
    }));
    let err = validate_request(&req).expect_err("should fail — only 1 demand point");
    assert_eq!(err.exit_code(), 1);
}

/// Distribution with zero demand points (empty list) → ValidationError (exit 1).
#[test]
fn test_validate_rejects_distribution_with_zero_demand_points() {
    let req = parse(json!({
        "project_type": "distribution",
        "terrain_points": terrain(),
        "source":         {"x": 0.0, "y": 0.0},
        "service_points": []
    }));
    let err = validate_request(&req).expect_err("should fail — 0 demand points");
    assert_eq!(err.exit_code(), 1);
}

/// Distribution without source → ValidationError (exit 1).
#[test]
fn test_validate_rejects_distribution_without_source() {
    let req = parse(json!({
        "project_type": "distribution",
        "terrain_points": terrain(),
        "service_points": [
            {"x": 10.0, "y": 10.0},
            {"x": 20.0, "y": 20.0}
        ]
        // source intentionally absent
    }));
    let err = validate_request(&req).expect_err("should fail — source missing");
    assert_eq!(err.exit_code(), 1);
}

// ── Error-path: pump_station ──────────────────────────────────────────────────

/// pump_station without source → ValidationError (exit 1).
#[test]
fn test_validate_rejects_pump_station_without_source() {
    let req = parse(json!({
        "project_type": "pump_station",
        "terrain_points": terrain(),
        "outlet": {"x": 50.0, "y": 50.0}
        // source intentionally absent
    }));
    let err = validate_request(&req).expect_err("should fail — source missing");
    assert_eq!(err.exit_code(), 1);
}

/// pump_station without outlet → ValidationError (exit 1).
#[test]
fn test_validate_rejects_pump_station_without_outlet() {
    let req = parse(json!({
        "project_type": "pump_station",
        "terrain_points": terrain(),
        "source": {"x": 0.0, "y": 0.0}
        // outlet intentionally absent
    }));
    let err = validate_request(&req).expect_err("should fail — outlet missing");
    assert_eq!(err.exit_code(), 1);
}

// ── Error-path: intake ────────────────────────────────────────────────────────

/// intake without source → ValidationError (exit 1).
#[test]
fn test_validate_rejects_intake_without_source() {
    let req = parse(json!({
        "project_type": "intake",
        "terrain_points": terrain()
        // source intentionally absent
    }));
    let err = validate_request(&req).expect_err("should fail — source missing");
    assert_eq!(err.exit_code(), 1);
}

// ── Error-path: range check (req.validate()) ─────────────────────────────────

/// A request with nsga_population_size below the minimum [20, 500] range must
/// be rejected by req.validate() — REQ-002 rule 1.
#[test]
fn test_validate_rejects_out_of_range_population_size() {
    // nsga_population_size = 0 is below the minimum of 20.
    let req = parse(json!({
        "project_type": "sewer",
        "terrain_points": terrain(),
        "outlet":         {"x": 0.0, "y": 0.0},
        "service_points": [{"x": 10.0, "y": 10.0}],
        "nsga_population_size": 0
    }));
    let err = validate_request(&req).expect_err("should fail — population_size out of range");
    assert_eq!(err.exit_code(), 1);
}
