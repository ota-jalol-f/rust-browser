//! [`PluginResult`]: the `{data, success, error_message}` envelope every plugin
//! call returns.

use serde::{Deserialize, Serialize};

/// The result envelope every plugin call returns, matching the reference
/// browser's `MyBrowser.Abstractions.PluginResult` byte-for-byte: fields
/// serialize in the order `data`, `success`, `error_message`.
///
/// Conventions (as in the reference plugins):
/// - [`PluginResult::ok`] — succeeded, `data` carries the payload.
/// - [`PluginResult::cancel`] — the user dismissed a modal window; `success`
///   is `false` and `error_message` is empty.
/// - [`PluginResult::fail`] — an error; `error_message` describes it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginResult {
    /// The payload on success (often itself a JSON string), else empty.
    pub data: String,
    /// Whether the call succeeded.
    pub success: bool,
    /// A human-readable error, or empty on success / cancel.
    pub error_message: String,
}

impl PluginResult {
    /// A successful result carrying `data`.
    pub fn ok(data: impl Into<String>) -> Self {
        Self { data: data.into(), success: true, error_message: String::new() }
    }

    /// A user cancellation (e.g. the modal window was closed): not a success,
    /// but not an error either — empty `error_message`.
    pub fn cancel() -> Self {
        Self { data: String::new(), success: false, error_message: String::new() }
    }

    /// A failure carrying an error message.
    pub fn fail(message: impl Into<String>) -> Self {
        Self { data: String::new(), success: false, error_message: message.into() }
    }

    /// Serialize to the JSON envelope string sent back to page JavaScript.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| {
            r#"{"data":"","success":false,"error_message":"serialize failed"}"#.to_string()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ok_envelope_shape() {
        assert_eq!(
            PluginResult::ok("hi").to_json(),
            r#"{"data":"hi","success":true,"error_message":""}"#
        );
    }

    #[test]
    fn cancel_envelope_shape() {
        assert_eq!(
            PluginResult::cancel().to_json(),
            r#"{"data":"","success":false,"error_message":""}"#
        );
    }

    #[test]
    fn fail_envelope_shape() {
        assert_eq!(
            PluginResult::fail("boom").to_json(),
            r#"{"data":"","success":false,"error_message":"boom"}"#
        );
    }

    #[test]
    fn data_is_escaped() {
        // A JSON-string payload is escaped correctly (the reference's `data` is
        // frequently itself a JSON string).
        let r = PluginResult::ok(r#"{"a":1}"#);
        assert_eq!(r.to_json(), r#"{"data":"{\"a\":1}","success":true,"error_message":""}"#);
    }
}
