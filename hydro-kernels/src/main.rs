//! hydro-kernels — JSON in / JSON out shell over the `hydro-hydraulics` kernels.
//!
//! `hydro-cli` runs the NSGA-III optimizer: it takes a `DesignRequest` and returns a
//! `DesignResult`. The verification kernels of `hydro-hydraulics` — Manning on a
//! rectangular open channel, Darcy-Weisbach, Hazen-Williams — are **library functions**,
//! not subcommands, so nothing outside this workspace could reach them. This binary is
//! the shell that makes them reachable, and nothing else.
//!
//! **It contains no engineering.** Every number it emits is the return value of a
//! `pub fn` of `hydro-hydraulics`, verbatim. There is no arithmetic in this file: no
//! formula, no unit conversion, no default standing in for a physical value. If a
//! quantity is not what one of those functions returns, this binary does not report it —
//! Reynolds is the deliberate example. `head_loss_dw` derives it internally from the
//! velocity and never surfaces it, and re-deriving it here would put the same physics in
//! a second place, where it could drift from the first.
//!
//! # Protocol
//!
//! One JSON object on stdin, one JSON object on stdout:
//!
//! ```text
//! {"kernel": "<name>", "<arg>": <value>, ...}
//! ```
//!
//! Every kernel is a 1:1 wrapper over one `pub fn`, so its argument names are that
//! function's parameter names and the response carries the function's own result.
//!
//! # Exit codes
//!
//! Mirrors `hydro-cli`, so the adapter's existing exit-code table keeps its meaning.
//!
//! | Code | Meaning                                                                     |
//! |------|-----------------------------------------------------------------------------|
//! | 0    | Success                                                                     |
//! | 1    | Validation error (bad JSON, unknown kernel, missing or wrong-typed argument) |
//! | 4    | Internal / IO error                                                         |

use std::io::{self, Read, Write};
use std::process;

use hydro_hydraulics::darcy_weisbach::{friction_factor, head_loss_dw};
use hydro_hydraulics::hazen_williams::{
    head_loss as hw_head_loss, hw_coefficient, required_diameter_pressure, velocity as hw_velocity,
    HW_COEFFICIENTS,
};
use hydro_hydraulics::open_channel::rectangular_channel_flow_full;
use serde_json::{json, Map, Value};

/// Every kernel this binary publishes, with the arguments it requires.
///
/// Kept as data so a rejection can name what IS valid, which the MAMUT contract asks of
/// every error: "los errores nombran lo válido, no solo lo que falló".
const KERNELS: &[(&str, &[&str])] = &[
    (
        "rectangular_channel_flow",
        &["width_m", "depth_m", "slope", "roughness_n"],
    ),
    (
        "darcy_head_loss",
        &["velocity_m_s", "diameter_m", "length_m", "roughness_mm"],
    ),
    (
        "darcy_friction_factor",
        &["reynolds", "roughness_mm", "diameter_m"],
    ),
    (
        "hazen_williams_head_loss",
        &["flow_m3s", "diameter_m", "length_m", "c"],
    ),
    ("hazen_williams_velocity", &["flow_m3s", "diameter_m"]),
    ("hazen_williams_coefficient", &["material"]),
    (
        "hazen_williams_required_diameter",
        &[
            "flow_m3s",
            "length_m",
            "available_head_m",
            "c",
            "available_diameters_m",
        ],
    ),
];

fn kernel_names() -> Vec<&'static str> {
    KERNELS.iter().map(|(name, _)| *name).collect()
}

// ── argument access ───────────────────────────────────────────────────────────

/// A required finite `f64` argument, or a rejection that names the argument.
///
/// Non-finite input is refused rather than passed through: a NaN travelling into a
/// kernel comes back as a NaN result that serialises to JSON `null`, which reads
/// downstream as "no value" instead of "bad input".
fn number(params: &Map<String, Value>, key: &str) -> Result<f64, String> {
    match params.get(key) {
        None => Err(format!("falta el argumento obligatorio '{key}'")),
        Some(Value::Number(n)) => match n.as_f64() {
            Some(v) if v.is_finite() => Ok(v),
            _ => Err(format!("'{key}' no es un numero finito")),
        },
        Some(other) => Err(format!(
            "'{key}' debe ser un numero, llego {}",
            type_name(other)
        )),
    }
}

fn text<'a>(params: &'a Map<String, Value>, key: &str) -> Result<&'a str, String> {
    match params.get(key) {
        None => Err(format!("falta el argumento obligatorio '{key}'")),
        Some(Value::String(s)) => Ok(s.as_str()),
        Some(other) => Err(format!(
            "'{key}' debe ser una cadena, llego {}",
            type_name(other)
        )),
    }
}

fn number_list(params: &Map<String, Value>, key: &str) -> Result<Vec<f64>, String> {
    let items = match params.get(key) {
        None => return Err(format!("falta el argumento obligatorio '{key}'")),
        Some(Value::Array(items)) => items,
        Some(other) => {
            return Err(format!(
                "'{key}' debe ser una lista de numeros, llego {}",
                type_name(other)
            ))
        }
    };
    if items.is_empty() {
        return Err(format!("'{key}' no puede ser una lista vacia"));
    }
    let mut out = Vec::with_capacity(items.len());
    for (index, item) in items.iter().enumerate() {
        match item.as_f64() {
            Some(v) if v.is_finite() => out.push(v),
            _ => return Err(format!("'{key}'[{index}] no es un numero finito")),
        }
    }
    Ok(out)
}

fn type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "un booleano",
        Value::Number(_) => "un numero",
        Value::String(_) => "una cadena",
        Value::Array(_) => "una lista",
        Value::Object(_) => "un objeto",
    }
}

/// Refuse an argument the kernel does not take, instead of ignoring it.
///
/// A silently dropped argument is the failure mode this layer exists to avoid: the
/// caller believes it set a value, the engine never saw it, and the answer looks right.
fn reject_extra(
    params: &Map<String, Value>,
    kernel: &str,
    accepted: &[&str],
) -> Result<(), String> {
    for key in params.keys() {
        if key == "kernel" || accepted.contains(&key.as_str()) {
            continue;
        }
        return Err(format!(
            "'{kernel}' no acepta el argumento '{key}'. Acepta: {}",
            accepted.join(", ")
        ));
    }
    Ok(())
}

// ── dispatch ──────────────────────────────────────────────────────────────────

fn dispatch(params: &Map<String, Value>) -> Result<Value, String> {
    let kernel = text(params, "kernel")?;
    let accepted = KERNELS
        .iter()
        .find(|(name, _)| *name == kernel)
        .map(|(_, args)| *args)
        .ok_or_else(|| {
            format!(
                "kernel desconocido '{kernel}'. Disponibles: {}",
                kernel_names().join(", ")
            )
        })?;
    reject_extra(params, kernel, accepted)?;

    match kernel {
        "rectangular_channel_flow" => {
            let result = rectangular_channel_flow_full(
                number(params, "width_m")?,
                number(params, "depth_m")?,
                number(params, "slope")?,
                number(params, "roughness_n")?,
            );
            Ok(json!({
                "area_m2": result.area_m2,
                "wetted_perimeter_m": result.wetted_perimeter_m,
                "hydraulic_radius_m": result.hydraulic_radius_m,
                "velocity_m_s": result.velocity_m_s,
                "flow_m3_s": result.flow_m3_s,
                "flow_lps": result.flow_lps,
                "froude_number": result.froude_number,
                "regime": result.regime,
            }))
        }
        "darcy_head_loss" => Ok(json!({
            "head_loss_m": head_loss_dw(
                number(params, "velocity_m_s")?,
                number(params, "diameter_m")?,
                number(params, "length_m")?,
                number(params, "roughness_mm")?,
            )
        })),
        "darcy_friction_factor" => Ok(json!({
            "friction_factor": friction_factor(
                number(params, "reynolds")?,
                number(params, "roughness_mm")?,
                number(params, "diameter_m")?,
            )
        })),
        "hazen_williams_head_loss" => Ok(json!({
            "head_loss_m": hw_head_loss(
                number(params, "flow_m3s")?,
                number(params, "diameter_m")?,
                number(params, "length_m")?,
                number(params, "c")?,
            )
        })),
        "hazen_williams_velocity" => Ok(json!({
            "velocity_m_s": hw_velocity(
                number(params, "flow_m3s")?,
                number(params, "diameter_m")?,
            )
        })),
        "hazen_williams_coefficient" => {
            let material = text(params, "material")?;
            Ok(json!({
                "c": hw_coefficient(material),
                // The lookup falls back to 150.0 for an unknown name, so the caller is
                // told which names the table actually holds — otherwise a default is
                // indistinguishable from a hit.
                "known_materials": HW_COEFFICIENTS
                    .iter()
                    .map(|(name, c)| json!({"material": name, "c": c}))
                    .collect::<Vec<_>>(),
            }))
        }
        "hazen_williams_required_diameter" => {
            let available = number_list(params, "available_diameters_m")?;
            Ok(json!({
                "diameter_m": required_diameter_pressure(
                    number(params, "flow_m3s")?,
                    number(params, "length_m")?,
                    number(params, "available_head_m")?,
                    number(params, "c")?,
                    &available,
                )
            }))
        }
        _ => unreachable!("el kernel ya se resolvio contra KERNELS"),
    }
}

// ── main ──────────────────────────────────────────────────────────────────────

fn fail(code: i32, message: String) -> ! {
    let body = json!({ "error": message });
    let rendered = serde_json::to_string(&body).unwrap_or_else(|_| "{}".to_string());
    let _ = io::stdout().write_all(rendered.as_bytes());
    let _ = io::stdout().flush();
    eprintln!("hydro-kernels: {message}");
    process::exit(code);
}

fn main() {
    let mut raw = Vec::new();
    if let Err(e) = io::stdin().read_to_end(&mut raw) {
        fail(4, format!("no se pudo leer stdin: {e}"));
    }

    let payload: Value = match serde_json::from_slice(&raw) {
        Ok(v) => v,
        Err(e) => fail(1, format!("stdin no es JSON valido: {e}")),
    };
    let params = match payload {
        Value::Object(map) => map,
        other => fail(
            1,
            format!("se esperaba un objeto JSON, llego {}", type_name(&other)),
        ),
    };

    match dispatch(&params) {
        Ok(result) => {
            let rendered = match serde_json::to_vec(&result) {
                Ok(bytes) => bytes,
                Err(e) => fail(4, format!("no se pudo serializar el resultado: {e}")),
            };
            if io::stdout().write_all(&rendered).is_err() {
                process::exit(4);
            }
            if io::stdout().flush().is_err() {
                process::exit(4);
            }
            process::exit(0);
        }
        Err(message) => fail(1, message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(raw: &str) -> Result<Value, String> {
        let params: Map<String, Value> = serde_json::from_str(raw).expect("fixture json");
        dispatch(&params)
    }

    #[test]
    fn rectangular_channel_matches_the_library_function() {
        let got = call(
            r#"{"kernel":"rectangular_channel_flow","width_m":2.0,"depth_m":0.5,
                "slope":0.002,"roughness_n":0.013}"#,
        )
        .expect("kernel ok");
        let expected = rectangular_channel_flow_full(2.0, 0.5, 0.002, 0.013);
        assert_eq!(got["velocity_m_s"], json!(expected.velocity_m_s));
        assert_eq!(got["flow_m3_s"], json!(expected.flow_m3_s));
        assert_eq!(got["froude_number"], json!(expected.froude_number));
        assert_eq!(got["regime"], json!(expected.regime));
    }

    #[test]
    fn darcy_head_loss_matches_the_library_function() {
        let got = call(
            r#"{"kernel":"darcy_head_loss","velocity_m_s":1.5,"diameter_m":0.2,
                "length_m":100.0,"roughness_mm":0.0015}"#,
        )
        .expect("kernel ok");
        assert_eq!(
            got["head_loss_m"],
            json!(head_loss_dw(1.5, 0.2, 100.0, 0.0015))
        );
    }

    #[test]
    fn hazen_williams_head_loss_matches_the_library_function() {
        let got = call(
            r#"{"kernel":"hazen_williams_head_loss","flow_m3s":0.01,"diameter_m":0.2,
                "length_m":100.0,"c":150.0}"#,
        )
        .expect("kernel ok");
        assert_eq!(
            got["head_loss_m"],
            json!(hw_head_loss(0.01, 0.2, 100.0, 150.0))
        );
    }

    #[test]
    fn an_unknown_kernel_names_the_ones_that_exist() {
        let err = call(r#"{"kernel":"tirante_critico"}"#).expect_err("debe rechazar");
        assert!(err.contains("tirante_critico"), "{err}");
        for name in kernel_names() {
            assert!(err.contains(name), "el error no nombra '{name}': {err}");
        }
    }

    #[test]
    fn a_missing_argument_is_named() {
        let err = call(r#"{"kernel":"hazen_williams_velocity","flow_m3s":0.01}"#)
            .expect_err("debe rechazar");
        assert!(err.contains("diameter_m"), "{err}");
    }

    #[test]
    fn an_extra_argument_is_refused_not_ignored() {
        let err = call(
            r#"{"kernel":"hazen_williams_velocity","flow_m3s":0.01,"diameter_m":0.2,
                "roughness_n":0.013}"#,
        )
        .expect_err("debe rechazar");
        assert!(err.contains("roughness_n"), "{err}");
    }

    #[test]
    fn a_number_outside_f64_never_reaches_a_kernel() {
        // The `is_finite` guard in `number` is a second line: serde_json refuses an
        // out-of-range literal while parsing, so it never becomes a Value at all. The
        // test asserts where the refusal ACTUALLY happens instead of asserting a branch
        // no input can reach -- a test that passed here for the wrong reason would be
        // reporting a guard that never ran.
        let parsed: Result<Map<String, Value>, _> = serde_json::from_str(
            r#"{"kernel":"hazen_williams_velocity","flow_m3s":1e400,"diameter_m":0.2}"#,
        );
        assert!(parsed.is_err(), "serde_json acepto un numero fuera de f64");
    }

    #[test]
    fn a_wrong_typed_argument_is_named_with_its_type() {
        let err =
            call(r#"{"kernel":"hazen_williams_velocity","flow_m3s":"mucha","diameter_m":0.2}"#)
                .expect_err("debe rechazar");
        assert!(err.contains("flow_m3s"), "{err}");
        assert!(err.contains("cadena"), "{err}");
    }

    #[test]
    fn every_kernel_in_the_table_dispatches() {
        // Guards the table against a name that is advertised and unreachable: an entry
        // whose match arm is missing would fall into `unreachable!` and abort the test.
        for (name, _) in KERNELS {
            let params: Map<String, Value> =
                serde_json::from_str(&format!(r#"{{"kernel":"{name}"}}"#)).expect("fixture json");
            let err = dispatch(&params).expect_err("sin argumentos, todo kernel rechaza");
            assert!(
                err.starts_with("falta el argumento obligatorio"),
                "'{name}' no llego a su rama: {err}"
            );
        }
    }
}
