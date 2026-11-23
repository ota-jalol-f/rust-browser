//! The `Answere` JSON envelope, matching the reference `Answere<T>.ToString`
//! (Newtonsoft, `NullValueHandling.Ignore`, PascalCase, order `AnswereId`,
//! `AnswereMessage`, `AnswereComment`, `Data`). Pinned to real output in the tests.

use serde_json::Value;

/// Build the envelope: `{"AnswereId":..,"AnswereMessage":..,"AnswereComment":..,"Data":..}`
/// with `Data` omitted when `None` (as `NullValueHandling.Ignore`).
pub fn answere_json(id: i64, message: &str, comment: &str, data: Option<Value>) -> String {
    let mut parts = vec![
        format!("\"AnswereId\":{id}"),
        format!("\"AnswereMessage\":{}", json_string(message)),
        format!("\"AnswereComment\":{}", json_string(comment)),
    ];
    if let Some(d) = data {
        parts.push(format!("\"Data\":{d}"));
    }
    format!("{{{}}}", parts.join(","))
}

/// A JSON string literal via serde (correct escaping).
fn json_string(s: &str) -> String {
    Value::String(s.to_string()).to_string()
}
