//! Classify a plugin request into a [`PhotoCall`], mirroring the reference
//! `PhotoPlugin.ExecuteAsync` / `IsTakeDocumentPhotoRequest`, and handling the two
//! shapes a request can arrive in:
//!
//! - **Host object** — the `sentinel-plugin` bridge turns
//!   `chrome.webview.hostObjects.photo.TakeDocumentPhoto(optionsJson)` into
//!   `{"method":"TakeDocumentPhoto","args":["<optionsJson>"]}`. The options are
//!   `args[0]`.
//! - **`share.RunPlugin`** — `RunPlugin("Asbt.Photo", json)` passes `json`
//!   verbatim: `{"method":"takeDocumentPhoto", ...options}`, so the whole request
//!   doubles as the options (the `method` member is ignored by the parser).
//!
//! `method` is matched case-insensitively. Anything that is not a
//! `takeDocumentPhoto` request — including a `TakePhoto` call, an empty request, or
//! non-JSON — is [`PhotoCall::TakePhoto`], exactly as 1.0.0 behaved.

use serde_json::Value;

/// Which `photo` method a request resolves to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PhotoCall {
    /// 1.0.0 `TakePhoto()` — the Canon full-frame PNG path.
    TakePhoto,
    /// 1.1.0 `TakeDocumentPhoto(optionsJson)` with the extracted options JSON.
    TakeDocumentPhoto(String),
}

/// Classify `request`. See the module docs for the two request shapes.
pub fn classify(request: &str) -> PhotoCall {
    let value: Value = match serde_json::from_str(request) {
        Ok(v) => v,
        Err(_) => return PhotoCall::TakePhoto,
    };
    let object = match value.as_object() {
        Some(o) => o,
        None => return PhotoCall::TakePhoto,
    };

    let is_doc = get_ignore_ascii_case(object, "method")
        .and_then(Value::as_str)
        .is_some_and(|m| m.eq_ignore_ascii_case("takeDocumentPhoto"));
    if !is_doc {
        return PhotoCall::TakePhoto;
    }

    // Host-object shape: the options JSON is the first (string) arg. Otherwise the
    // whole request is the options (the inline `share.RunPlugin` shape).
    let from_args = get_ignore_ascii_case(object, "args")
        .and_then(Value::as_array)
        .and_then(|a| a.first())
        .and_then(Value::as_str)
        .map(str::to_string);
    PhotoCall::TakeDocumentPhoto(from_args.unwrap_or_else(|| request.to_string()))
}

fn get_ignore_ascii_case<'a>(
    object: &'a serde_json::Map<String, Value>,
    name: &str,
) -> Option<&'a Value> {
    object
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_object_shape_takes_args_zero_as_options() {
        let r = r#"{"method":"TakeDocumentPhoto","args":["{\"scalePercent\":30}"]}"#;
        assert_eq!(classify(r), PhotoCall::TakeDocumentPhoto(r#"{"scalePercent":30}"#.to_string()));
    }

    #[test]
    fn run_plugin_inline_shape_uses_whole_request() {
        let r = r#"{"method":"takeDocumentPhoto","scalePercent":30}"#;
        assert_eq!(classify(r), PhotoCall::TakeDocumentPhoto(r.to_string()));
    }

    #[test]
    fn method_is_case_insensitive() {
        let r = r#"{"method":"TAKEDOCUMENTPHOTO","scalePercent":30}"#;
        assert!(matches!(classify(r), PhotoCall::TakeDocumentPhoto(_)));
    }

    #[test]
    fn take_photo_host_object_shape() {
        assert_eq!(classify(r#"{"method":"TakePhoto","args":[]}"#), PhotoCall::TakePhoto);
    }

    #[test]
    fn empty_or_non_json_or_non_object_is_take_photo() {
        assert_eq!(classify(""), PhotoCall::TakePhoto);
        assert_eq!(classify("not json"), PhotoCall::TakePhoto);
        assert_eq!(classify("\"a string\""), PhotoCall::TakePhoto);
        assert_eq!(classify("[1,2]"), PhotoCall::TakePhoto);
    }

    #[test]
    fn no_method_is_take_photo() {
        assert_eq!(classify(r#"{"scalePercent":30}"#), PhotoCall::TakePhoto);
    }
}
