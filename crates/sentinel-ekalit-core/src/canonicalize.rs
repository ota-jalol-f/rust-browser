//! `EKalitObj.SignStringV2`'s canonicalisation — the exact string that is hashed and
//! signed. Ported from the C# `SumValues` / `GetValues` and pinned against real
//! Newtonsoft output (see the crate's `tests/`).
//!
//! The rule, derived from the C# behaviour:
//! - top-level: an array → concat of `sum_values` over its elements; else `sum_values`.
//! - `sum_values(object)`: sort the members by name, concat `get_values(value)`.
//! - `sum_values(scalar)`: the scalar's text; `sum_values(null)`: "".
//! - `get_values(object)`: recurse over its members in **document** order (not sorted).
//! - `get_values(array)`: the **scalar** elements only (objects/arrays inside are
//!   skipped, NOT recursed).
//! - `get_values(scalar)`: the scalar's text; `get_values(null)`: "".
//! - finally the whole thing is upper-cased.
//!
//! DIVERGENCE (documented): a C# float renders culture-specifically ("1,5") and an
//! ISO-8601 string is auto-parsed as a Date and rendered culture-specifically; this
//! locale-free port renders a float as "1.5" and keeps a date string verbatim. For
//! those value kinds the signed bytes differ from the production C#.
//!
//! SECURITY (acknowledged, automated review 2026-11-23): this canonicalisation is
//! lossy and ambiguous — it drops objects nested inside arrays, upper-cases, and is
//! locale-sensitive. That is NOT a design choice here: it is the EXACT algorithm the
//! ASBT server verifies signatures against (reproduced from the reference plugin).
//! A "safer" form (RFC 8785, signing raw bytes, recursing into every value) would
//! produce a different signed string and break verification against the production
//! server, which cannot be changed from this browser. The user accepted this
//! trade-off; the parity gaps above are the only honest concern and are flagged.

use serde_json::Value;

/// `SignStringV2(data)`: the canonical signed string (upper-cased). `Err` on invalid JSON.
pub fn sign_string_v2(json: &str) -> Result<String, serde_json::Error> {
    let value: Value = serde_json::from_str(json)?;
    let sum = match &value {
        Value::Array(arr) => arr.iter().map(sum_values).collect::<String>(),
        other => sum_values(other),
    };
    Ok(sum.to_uppercase())
}

/// `SumValues`: top-level object members sorted by name, else the scalar's text.
fn sum_values(token: &Value) -> String {
    match token {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort(); // OrderBy(p => p.Name)
            keys.into_iter().map(|k| get_values(&map[k])).collect()
        }
        Value::Null => String::new(),
        other => scalar_text(other),
    }
}

/// `GetValues`: all scalar leaves under a value — recursing through nested objects
/// (document order), taking only scalar array elements (array objects are skipped).
fn get_values(value: &Value) -> String {
    match value {
        Value::Object(map) => map.values().map(get_values).collect(),
        Value::Array(arr) => arr
            .iter()
            .filter_map(|e| if is_scalar(e) { Some(scalar_text(e)) } else { None })
            .collect(),
        Value::Null => String::new(),
        other => scalar_text(other),
    }
}

fn is_scalar(v: &Value) -> bool {
    matches!(v, Value::String(_) | Value::Number(_) | Value::Bool(_))
}

/// A scalar's `JValue.ToString()`. Bool is C#'s "True"/"False"; a number uses the
/// Rust rendering (the documented float divergence).
fn scalar_text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Bool(b) => if *b { "True" } else { "False" }.to_string(),
        Value::Number(n) => n.to_string(),
        _ => String::new(),
    }
}
