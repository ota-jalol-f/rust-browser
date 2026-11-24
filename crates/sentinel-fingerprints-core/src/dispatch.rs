//! Classify a plugin request into [`FingerprintsCall`], mirroring the reference
//! `FingerprintsPlugin.ExecuteAsync` / `IsEnrollRequest`, and handling both request
//! shapes:
//! - host object: `hostObjects.fingerprints.Enroll(optionsJson)` →
//!   `{"method":"Enroll","args":["<optionsJson>"]}` (options = `args[0]`);
//! - `share.RunPlugin`: `{"method":"enroll", ...options}` (the whole request is the
//!   options).
//!
//! `method` is matched case-insensitively. Anything else — `Scan`, empty, non-JSON —
//! is [`FingerprintsCall::Scan`], as 1.0.0 behaved.

use serde_json::Value;

/// Which `fingerprints` method a request resolves to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FingerprintsCall {
    /// 1.0.0 `Scan()` — one flat finger (a documented stub in the plugin).
    Scan,
    /// 1.1.0 `Enroll(optionsJson)` with the extracted options JSON.
    Enroll(String),
}

/// Classify `request`.
pub fn classify(request: &str) -> FingerprintsCall {
    let value: Value = match serde_json::from_str(request) {
        Ok(v) => v,
        Err(_) => return FingerprintsCall::Scan,
    };
    let object = match value.as_object() {
        Some(o) => o,
        None => return FingerprintsCall::Scan,
    };
    let is_enroll = get_ignore_ascii_case(object, "method")
        .and_then(Value::as_str)
        .is_some_and(|m| m.eq_ignore_ascii_case("enroll"));
    if !is_enroll {
        return FingerprintsCall::Scan;
    }
    let from_args = get_ignore_ascii_case(object, "args")
        .and_then(Value::as_array)
        .and_then(|a| a.first())
        .and_then(Value::as_str)
        .map(str::to_string);
    FingerprintsCall::Enroll(from_args.unwrap_or_else(|| request.to_string()))
}

fn get_ignore_ascii_case<'a>(
    object: &'a serde_json::Map<String, Value>,
    name: &str,
) -> Option<&'a Value> {
    object.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_object_shape_takes_args_zero() {
        let r = r#"{"method":"Enroll","args":["{\"secondScreen\":true}"]}"#;
        assert_eq!(classify(r), FingerprintsCall::Enroll(r#"{"secondScreen":true}"#.to_string()));
    }

    #[test]
    fn run_plugin_inline_uses_whole_request() {
        let r = r#"{"method":"enroll","secondScreen":true}"#;
        assert_eq!(classify(r), FingerprintsCall::Enroll(r.to_string()));
    }

    #[test]
    fn method_case_insensitive() {
        assert!(matches!(classify(r#"{"method":"ENROLL"}"#), FingerprintsCall::Enroll(_)));
    }

    #[test]
    fn scan_cases() {
        assert_eq!(classify(""), FingerprintsCall::Scan);
        assert_eq!(classify("not json"), FingerprintsCall::Scan);
        assert_eq!(classify("{}"), FingerprintsCall::Scan);
        assert_eq!(classify(r#"{"method":"scan"}"#), FingerprintsCall::Scan);
        assert_eq!(classify("[1,2]"), FingerprintsCall::Scan);
    }
}
