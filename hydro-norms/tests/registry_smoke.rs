//! Integration tests for NormRegistry — T-3.2 RED → GREEN.
//!
//! Tests load the committed `norms_smoke.json` oracle fixture and assert that
//! the Rust NormRegistry matches the Python oracle exactly (rule counts,
//! alias resolution, error on unknown code).

use hydro_norms::NormRegistry;
use hydro_types::ProjectType;
use oracle::norms::load_norms_smoke;

fn project_type_from_screaming(s: &str) -> ProjectType {
    match s {
        "SEWER" => ProjectType::Sewer,
        "WATER_SUPPLY" => ProjectType::WaterSupply,
        "CONVEYANCE" => ProjectType::Conveyance,
        "DISTRIBUTION" => ProjectType::Distribution,
        "PUMP_STATION" => ProjectType::PumpStation,
        "INTAKE" => ProjectType::Intake,
        other => panic!("unknown ProjectType key: '{other}'"),
    }
}

// T-3.2 acceptance criterion 1: all 15 profiles load without error
#[test]
fn all_15_profiles_load_without_error() {
    let fixture = load_norms_smoke();
    assert_eq!(
        fixture.profiles.len(),
        15,
        "fixture must have exactly 15 profiles"
    );
    for entry in &fixture.profiles {
        let profile = NormRegistry::get(&entry.code)
            .unwrap_or_else(|e| panic!("failed to load {}: {e}", entry.code));
        assert_eq!(profile.code, entry.code, "code mismatch for {}", entry.code);
    }
}

// T-3.2 acceptance criterion 2: canonical alias CONAGUA → CONAGUA_MX
#[test]
fn conagua_alias_resolves_to_conagua_mx() {
    let profile = NormRegistry::get("CONAGUA").expect("CONAGUA alias must resolve");
    assert_eq!(profile.code, "CONAGUA_MX");
}

// T-3.2 acceptance criterion 3: QUITO → EPMAPS_EC
#[test]
fn quito_alias_resolves_to_epmaps_ec() {
    let profile = NormRegistry::get("QUITO").expect("QUITO alias must resolve");
    assert_eq!(profile.code, "EPMAPS_EC");
}

// T-3.2 acceptance criterion 4: unknown code returns NormError::UnknownNorm
#[test]
fn unknown_code_returns_error() {
    let result = NormRegistry::get("MADE_UP_NORM");
    assert!(
        result.is_err(),
        "expected Err for unknown code, got Ok({:?})",
        result.unwrap().code
    );
}

// T-3.2 acceptance criterion 5: rule counts per project_type match oracle exactly
// 15 profiles × 6 project types = 90 assertions
#[test]
fn rule_counts_match_oracle_for_all_profiles_and_project_types() {
    let fixture = load_norms_smoke();
    for entry in &fixture.profiles {
        let profile = NormRegistry::get(&entry.code)
            .unwrap_or_else(|e| panic!("failed to load {}: {e}", entry.code));
        for (pt_name, expected_count) in &entry.rule_counts {
            let pt = project_type_from_screaming(pt_name);
            let actual_count = profile.rules_for(pt).len();
            assert_eq!(
                actual_count, *expected_count,
                "rule count mismatch for profile={} project_type={}: expected={}, actual={}",
                entry.code, pt_name, expected_count, actual_count
            );
        }
    }
}

// T-3.2 acceptance criterion 6: copy_from expansion (PUMP_STATION copies CONVEYANCE in CONAGUA_MX)
#[test]
fn conagua_pump_station_copies_from_conveyance() {
    let profile = NormRegistry::get("CONAGUA_MX").expect("CONAGUA_MX must load");
    let conveyance_rules = profile.rules_for(ProjectType::Conveyance);
    let pump_station_rules = profile.rules_for(ProjectType::PumpStation);
    assert_eq!(
        pump_station_rules.len(),
        conveyance_rules.len(),
        "PUMP_STATION (copy_from CONVEYANCE) must have same rule count as CONVEYANCE"
    );
    // Verify rule keys match exactly (order must be the same — Python preserves insertion order)
    for (pump_rule, conv_rule) in pump_station_rules.iter().zip(conveyance_rules.iter()) {
        assert_eq!(
            pump_rule.key, conv_rule.key,
            "PUMP_STATION rule key '{}' != CONVEYANCE rule key '{}'",
            pump_rule.key, conv_rule.key
        );
    }
}

// T-3.2: available_codes returns sorted canonical codes
#[test]
fn available_codes_returns_sorted_canonical_codes() {
    let codes = NormRegistry::available_codes();
    assert_eq!(codes.len(), 15, "expected 15 canonical codes");
    // verify sorted
    let mut sorted = codes.clone();
    sorted.sort();
    assert_eq!(codes, sorted, "available_codes must be sorted ascending");
    // verify CONAGUA_MX is present
    assert!(
        codes.contains(&"CONAGUA_MX".to_string()),
        "CONAGUA_MX must be in available_codes"
    );
}
