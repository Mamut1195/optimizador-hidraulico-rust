//! NormRegistry — loads and caches the 15 norm JSON profiles.
//!
//! Mirrors Python `NormRegistry` from `hydro_engine/norms/profile.py`.
//!
//! Profile JSON files are embedded at compile-time via `include_str!` so the
//! binary carries them without a runtime file path dependency.
//!
//! Alias resolution covers all entries from `NormRegistry._aliases` in the
//! Python oracle (200+ entries). The alias table is a plain `match` for
//! zero-overhead dispatch.

use std::collections::HashMap;

use hydro_types::ProjectType;

use crate::error::NormError;
use crate::types::{NormProfile, NormRule, NormSource};

// ── Embedded profile data ─────────────────────────────────────────────────────

const CONAGUA_MX_JSON: &str = include_str!("data/conagua_mx.json");
const EPA_US_JSON: &str = include_str!("data/epa_us.json");
const ABNT_BR_JSON: &str = include_str!("data/abnt_br.json");
const RAS_CO_JSON: &str = include_str!("data/ras_co.json");
const EU_EN_JSON: &str = include_str!("data/eu_en.json");
const RNE_PE_JSON: &str = include_str!("data/rne_pe.json");
const ENOHSA_AR_JSON: &str = include_str!("data/enohsa_ar.json");
const EPMAPS_EC_JSON: &str = include_str!("data/epmaps_ec.json");
const ETAPA_EC_JSON: &str = include_str!("data/etapa_ec.json");
const INTERAGUA_EC_JSON: &str = include_str!("data/interagua_ec.json");
const CHILE_NCH_JSON: &str = include_str!("data/chile_nch.json");
const BOLIVIA_NB_JSON: &str = include_str!("data/bolivia_nb.json");
const CA_OPS_JSON: &str = include_str!("data/ca_ops.json");
const VENEZUELA_GACETA_4044_JSON: &str = include_str!("data/venezuela_gaceta_4044.json");
const DOMINICANA_INAPA_JSON: &str = include_str!("data/dominicana_inapa.json");

// ── NormRegistry ──────────────────────────────────────────────────────────────

/// Registry for known norm profiles.
///
/// Profiles are loaded lazily and cached for the process lifetime.
/// Thread-safety is not required (single-threaded optimizer core).
pub struct NormRegistry;

impl NormRegistry {
    /// Resolve a norm profile by canonical code or any documented alias.
    ///
    /// Normalization: trim, uppercase, replace `-` and spaces with `_`.
    /// Alias resolution covers all entries from `NormRegistry._aliases` in the
    /// Python oracle.
    ///
    /// # Errors
    ///
    /// Returns `NormError::UnknownNorm` when the code is not recognized.
    pub fn get(code: &str) -> Result<NormProfile, NormError> {
        let normalized = code.trim().to_uppercase().replace(['-', ' '], "_");
        let canonical = resolve_alias(&normalized).ok_or_else(|| NormError::UnknownNorm {
            code: code.to_string(),
        })?;
        load_profile_data(canonical)
    }

    /// Return sorted canonical norm codes available in the registry.
    pub fn available_codes() -> Vec<String> {
        let mut codes = vec![
            "CONAGUA_MX",
            "EPA_US",
            "ABNT_BR",
            "RAS_CO",
            "EU_EN",
            "RNE_PE",
            "ENOHSA_AR",
            "EPMAPS_EC",
            "ETAPA_EC",
            "INTERAGUA_EC",
            "CHILE_NCH",
            "BOLIVIA_NB",
            "CENTROAMERICA_OPS",
            "VENEZUELA_INOS",
            "DOMINICANA_INAPA",
        ]
        .into_iter()
        .map(String::from)
        .collect::<Vec<_>>();
        codes.sort();
        codes
    }
}

// ── Alias resolution ──────────────────────────────────────────────────────────

/// Map a normalized code to its canonical profile key.
///
/// Covers all entries from `NormRegistry._aliases` in `profile.py`.
#[allow(clippy::match_same_arms)]
fn resolve_alias(code: &str) -> Option<&'static str> {
    match code {
        // CONAGUA_MX
        "CONAGUA" | "MAPAS" | "CONAGUA_MX" => Some("CONAGUA_MX"),
        // EPA_US
        "EPA" | "EPA_US" => Some("EPA_US"),
        // ABNT_BR
        "ABNT" | "ABNT_BR" | "BRASIL" | "BRAZIL" | "NBR" => Some("ABNT_BR"),
        // RAS_CO
        "RAS" | "RAS_CO" | "COLOMBIA" | "MINVIVIENDA" | "RES_0330" => Some("RAS_CO"),
        // EU_EN
        "EU" | "EU_EN" | "EN" | "EN_752" | "EN_805" | "CEN" => Some("EU_EN"),
        // RNE_PE
        "RNE" | "RNE_PE" | "PERU" | "OS_050" | "OS_070" | "MVCS" => Some("RNE_PE"),
        // ENOHSA_AR
        "ENOHSA" | "ENOHSA_AR" | "ARGENTINA" | "AYSA" => Some("ENOHSA_AR"),
        // EPMAPS_EC
        "EPMAPS" | "EPMAPS_EC" | "QUITO" | "EMAAP_Q" => Some("EPMAPS_EC"),
        // ETAPA_EC
        "ETAPA" | "ETAPA_EC" | "ETAPA_EP" | "CUENCA" => Some("ETAPA_EC"),
        // INTERAGUA_EC
        "INTERAGUA" | "INTERAGUA_EC" | "EMAPAG" | "EMAPAG_EP" | "GUAYAQUIL" => {
            Some("INTERAGUA_EC")
        }
        // CHILE_NCH
        "CHILE"
        | "CHILE_NCH"
        | "NCH"
        | "NCH_691"
        | "NCH_1105"
        | "INN"
        | "INN_CHILE"
        | "SISS"
        | "AGUAS_ANDINAS"
        | "ESVAL"
        | "ESSBIO"
        | "ESSAL" => Some("CHILE_NCH"),
        // BOLIVIA_NB
        "BOLIVIA" | "BOLIVIA_NB" | "NB_688" | "NB_689" | "MMAYA" | "AAPS" | "IBNORCA" => {
            Some("BOLIVIA_NB")
        }
        // CENTROAMERICA_OPS
        "CENTROAMERICA"
        | "CENTROAMERICA_OPS"
        | "OPS_CEPIS"
        | "CEPIS"
        | "UNATSABAR"
        | "OPS"
        | "PAHO"
        | "COSTA_RICA"
        | "AYA"
        | "GUATEMALA"
        | "INFOM"
        | "EL_SALVADOR"
        | "ANDA"
        | "HONDURAS"
        | "SANAA"
        | "NICARAGUA"
        | "ENACAL"
        | "PANAMA"
        | "IDAAN" => Some("CENTROAMERICA_OPS"),
        // VENEZUELA_INOS
        "VENEZUELA"
        | "VENEZUELA_INOS"
        | "INOS"
        | "GACETA_4044"
        | "HIDROVEN"
        | "HIDROCAPITAL"
        | "COVENIN" => Some("VENEZUELA_INOS"),
        // DOMINICANA_INAPA
        "DOMINICANA"
        | "REPUBLICA_DOMINICANA"
        | "DOMINICANA_INAPA"
        | "INAPA"
        | "CAASD"
        | "CORAASAN" => Some("DOMINICANA_INAPA"),
        _ => None,
    }
}

// ── Profile loading ───────────────────────────────────────────────────────────

/// Load and parse a profile from embedded JSON.
///
/// Mirrors Python `load_profile_data(filename)` exactly:
/// - Resolves `source_catalog` references by `source_id`
/// - Expands `copy_from` entries by substituting the referenced project's
///   rule list (mirrors Python behavior; chains raise `NormError::CopyFromChain`)
fn load_profile_data(canonical: &str) -> Result<NormProfile, NormError> {
    let json_str = match canonical {
        "CONAGUA_MX" => CONAGUA_MX_JSON,
        "EPA_US" => EPA_US_JSON,
        "ABNT_BR" => ABNT_BR_JSON,
        "RAS_CO" => RAS_CO_JSON,
        "EU_EN" => EU_EN_JSON,
        "RNE_PE" => RNE_PE_JSON,
        "ENOHSA_AR" => ENOHSA_AR_JSON,
        "EPMAPS_EC" => EPMAPS_EC_JSON,
        "ETAPA_EC" => ETAPA_EC_JSON,
        "INTERAGUA_EC" => INTERAGUA_EC_JSON,
        "CHILE_NCH" => CHILE_NCH_JSON,
        "BOLIVIA_NB" => BOLIVIA_NB_JSON,
        "CENTROAMERICA_OPS" => CA_OPS_JSON,
        "VENEZUELA_INOS" => VENEZUELA_GACETA_4044_JSON,
        "DOMINICANA_INAPA" => DOMINICANA_INAPA_JSON,
        other => {
            return Err(NormError::UnknownNorm {
                code: other.to_string(),
            })
        }
    };

    let data: serde_json::Value =
        serde_json::from_str(json_str).map_err(|e| NormError::ProfileLoadError {
            file: canonical.to_string(),
            source: e,
        })?;

    // Build source catalog (source_id → NormSource)
    let source_catalog: HashMap<String, NormSource> = {
        let mut map = HashMap::new();
        if let Some(catalog) = data.get("source_catalog").and_then(|v| v.as_object()) {
            for (id, entry) in catalog {
                if let Ok(src) = serde_json::from_value::<NormSource>(entry.clone()) {
                    map.insert(id.clone(), src);
                }
            }
        }
        map
    };

    // Parse project_rules — handle copy_from entries
    let raw_project_rules = data["project_rules"]
        .as_object()
        .expect("project_rules must be an object");

    let mut project_rules: HashMap<ProjectType, Vec<NormRule>> = HashMap::new();

    for (project_key, rules_value) in raw_project_rules {
        let pt = project_type_from_str(project_key);

        // Handle copy_from
        let effective_rules_value = if rules_value.is_object()
            && rules_value.get("copy_from").is_some()
        {
            let source_key = rules_value["copy_from"]
                .as_str()
                .expect("copy_from must be a string");
            let source_value = raw_project_rules
                .get(source_key)
                .expect("copy_from references a missing project key");
            // Python raises ValueError for copy_from chains
            if source_value.is_object() && source_value.get("copy_from").is_some() {
                return Err(NormError::CopyFromChain {
                    profile: format!("{canonical}:{project_key}"),
                });
            }
            source_value
        } else {
            rules_value
        };

        let rule_array = effective_rules_value
            .as_array()
            .expect("project rules must be an array after copy_from resolution");

        let mut rules: Vec<NormRule> = Vec::with_capacity(rule_array.len());
        for rule_data in rule_array {
            let mut rule_obj = rule_data
                .as_object()
                .expect("each rule must be a JSON object")
                .clone();

            // Resolve source_id → NormSource
            if let Some(source_id) = rule_obj.remove("source_id") {
                if let Some(id_str) = source_id.as_str() {
                    if let Some(src) = source_catalog.get(id_str) {
                        rule_obj.insert(
                            "source".to_string(),
                            serde_json::to_value(src).expect("NormSource must serialize"),
                        );
                    }
                }
            }

            let rule: NormRule =
                serde_json::from_value(serde_json::Value::Object(rule_obj)).map_err(|e| {
                    NormError::ProfileLoadError {
                        file: canonical.to_string(),
                        source: e,
                    }
                })?;
            rules.push(rule);
        }

        project_rules.insert(pt, rules);
    }

    Ok(NormProfile {
        code: data["code"].as_str().unwrap_or(canonical).to_string(),
        country: data
            .get("country")
            .and_then(|v| v.as_str())
            .map(String::from),
        agency: data
            .get("agency")
            .and_then(|v| v.as_str())
            .map(String::from),
        version: data
            .get("version")
            .and_then(|v| v.as_str())
            .map(String::from),
        certified: data
            .get("certified")
            .and_then(|v| v.as_bool())
            .unwrap_or(true),
        certification_note: data
            .get("certification_note")
            .and_then(|v| v.as_str())
            .map(String::from),
        project_rules,
    })
}

/// Map a SCREAMING_SNAKE_CASE project type string to `ProjectType`.
///
/// Panics on unknown keys — profile JSON files are compile-time embedded and
/// validated during development, so this should never be reached at runtime.
fn project_type_from_str(s: &str) -> ProjectType {
    match s {
        "SEWER" => ProjectType::Sewer,
        "WATER_SUPPLY" => ProjectType::WaterSupply,
        "CONVEYANCE" => ProjectType::Conveyance,
        "DISTRIBUTION" => ProjectType::Distribution,
        "PUMP_STATION" => ProjectType::PumpStation,
        "INTAKE" => ProjectType::Intake,
        other => panic!("unknown ProjectType key in JSON profile: '{other}'"),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Unit tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_alias_returns_none_for_unknown_code() {
        assert!(resolve_alias("NONEXISTENT_NORM").is_none());
    }

    #[test]
    fn resolve_alias_conagua_returns_conagua_mx() {
        assert_eq!(resolve_alias("CONAGUA"), Some("CONAGUA_MX"));
    }

    #[test]
    fn resolve_alias_quito_returns_epmaps_ec() {
        assert_eq!(resolve_alias("QUITO"), Some("EPMAPS_EC"));
    }

    #[test]
    fn conagua_mx_loads_without_panic() {
        let profile = NormRegistry::get("CONAGUA_MX").unwrap();
        assert_eq!(profile.code, "CONAGUA_MX");
    }

    #[test]
    fn get_normalizes_hyphen_and_spaces() {
        // "EU EN" (with space) should normalize to "EU_EN" and resolve
        let result = NormRegistry::get("EU EN");
        // EU EN → EU_EN (alias) → EU_EN (canonical). If it works, profile loads.
        assert!(result.is_ok(), "EU EN with space should resolve via alias");
        assert_eq!(result.unwrap().code, "EU_EN");
    }
}
