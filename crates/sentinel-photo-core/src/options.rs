//! `DocumentPhotoOptions`: the options of `photo.TakeDocumentPhoto(optionsJson)`,
//! parsed and validated exactly as the reference
//! `Asbt.Photo.Plugin/Document/DocumentPhotoOptions.cs`.
//!
//! ```json
//! { "scalePercent": 30, "currentPhoto": "<base64 JPEG>" | null }
//! ```
//! - `scalePercent` — **required**, a JSON integer 1..=100. `20.5`, `"20"`, `0`,
//!   `101` are rejected. It is C#'s `AppSettings.DocumentPhotoQuality`, the
//!   `ScaleByPercent` argument (the Vue side sends the constant 30).
//! - `currentPhoto` — optional; `null` and `""` both mean none. Must be valid
//!   base64 (otherwise the call is rejected). It is C#'s `FormIDCard.DocumentImage`,
//!   shown in the right pane and returned unchanged by Enter.
//!
//! Member names are matched case-insensitively; unknown members (e.g. the `method`
//! of a `share.RunPlugin` request) are ignored.

use serde_json::Value;

use crate::base64;

/// A rejected options JSON. The message is surfaced verbatim to the page as the
/// `PluginResult.error_message`, matching the reference's thrown `ArgumentException`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptionsError(pub String);

impl OptionsError {
    /// The validation message (identical to the C# `ArgumentException.Message`).
    pub fn message(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for OptionsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for OptionsError {}

/// The smallest / largest accepted `scalePercent`.
pub const MIN_SCALE_PERCENT: i64 = 1;
/// See [`MIN_SCALE_PERCENT`].
pub const MAX_SCALE_PERCENT: i64 = 100;

/// The validated options of one `TakeDocumentPhoto` call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentPhotoOptions {
    /// 1..=100, the `ScaleByPercent` argument.
    pub scale_percent: i32,
    /// The caller's base64 text exactly as passed, or `None` (`null`/`""`).
    pub current_photo: Option<String>,
}

impl DocumentPhotoOptions {
    /// Parse + validate, mirroring `DocumentPhotoOptions.Parse` including every
    /// error message.
    pub fn parse(options_json: &str) -> Result<Self, OptionsError> {
        if options_json.trim().is_empty() {
            return Err(err(
                "optionsJson is empty; expected {\"scalePercent\": 1..100, \"currentPhoto\": base64|null}",
            ));
        }

        let value: Value = serde_json::from_str(options_json)
            .map_err(|e| err(&format!("optionsJson is not valid JSON: {e}")))?;
        let object = match &value {
            Value::Object(map) => map,
            _ => return Err(err("optionsJson must be a JSON object")),
        };

        // scalePercent — required integer 1..=100.
        let scale = get_ignore_ascii_case(object, "scalePercent");
        let scale = match scale {
            None | Some(Value::Null) => {
                return Err(err("scalePercent is required (integer 1..100)"))
            }
            Some(v) => v,
        };
        let percent = match scale.as_i64() {
            // `as_i64` is Some only for a JSON integer in i64 range — a float
            // (20.5, 20.0) or a string is None, exactly like `Type != Integer`.
            Some(n) => n,
            None => {
                return Err(err(&format!(
                    "scalePercent must be an integer 1..100, got {}",
                    compact(scale)
                )))
            }
        };
        if !(MIN_SCALE_PERCENT..=MAX_SCALE_PERCENT).contains(&percent) {
            return Err(err(&format!(
                "scalePercent must be an integer 1..100, got {}",
                compact(scale)
            )));
        }

        // currentPhoto — optional base64 string.
        let mut current_photo = None;
        if let Some(photo) = get_ignore_ascii_case(object, "currentPhoto") {
            if !photo.is_null() {
                let text = match photo {
                    Value::String(s) => s,
                    _ => return Err(err("currentPhoto must be a base64 string or null")),
                };
                if text.is_empty() {
                    current_photo = None;
                } else if base64::decode_dotnet(text).is_none() {
                    return Err(err("currentPhoto is not valid base64"));
                } else {
                    current_photo = Some(text.clone());
                }
            }
        }

        Ok(DocumentPhotoOptions { scale_percent: percent as i32, current_photo })
    }
}

/// Build an error with the reference's `"TakeDocumentPhoto: "` prefix.
fn err(rest: &str) -> OptionsError {
    OptionsError(format!("TakeDocumentPhoto: {rest}"))
}

/// Newtonsoft `JObject.GetValue(name, StringComparison.OrdinalIgnoreCase)`: the
/// first member whose name matches `name` ignoring ASCII case.
fn get_ignore_ascii_case<'a>(
    object: &'a serde_json::Map<String, Value>,
    name: &str,
) -> Option<&'a Value> {
    object
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v)
}

/// A token rendered as `JToken.ToString(Formatting.None)` is for the error message:
/// compact JSON, strings quoted, numbers bare.
fn compact(value: &Value) -> String {
    value.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(json: &str) -> String {
        DocumentPhotoOptions::parse(json).unwrap_err().0
    }

    #[test]
    fn valid_minimal() {
        let o = DocumentPhotoOptions::parse(r#"{"scalePercent":30}"#).unwrap();
        assert_eq!(o.scale_percent, 30);
        assert_eq!(o.current_photo, None);
    }

    #[test]
    fn scale_bounds() {
        assert_eq!(DocumentPhotoOptions::parse(r#"{"scalePercent":1}"#).unwrap().scale_percent, 1);
        assert_eq!(DocumentPhotoOptions::parse(r#"{"scalePercent":100}"#).unwrap().scale_percent, 100);
        assert_eq!(msg(r#"{"scalePercent":0}"#), "TakeDocumentPhoto: scalePercent must be an integer 1..100, got 0");
        assert_eq!(msg(r#"{"scalePercent":101}"#), "TakeDocumentPhoto: scalePercent must be an integer 1..100, got 101");
    }

    #[test]
    fn scale_must_be_integer() {
        assert_eq!(msg(r#"{"scalePercent":20.5}"#), "TakeDocumentPhoto: scalePercent must be an integer 1..100, got 20.5");
        assert_eq!(msg(r#"{"scalePercent":"20"}"#), "TakeDocumentPhoto: scalePercent must be an integer 1..100, got \"20\"");
    }

    #[test]
    fn scale_required() {
        assert_eq!(msg(r#"{"currentPhoto":null}"#), "TakeDocumentPhoto: scalePercent is required (integer 1..100)");
        assert_eq!(msg(r#"{"scalePercent":null}"#), "TakeDocumentPhoto: scalePercent is required (integer 1..100)");
    }

    #[test]
    fn empty_and_non_object_and_bad_json() {
        assert_eq!(msg("   "), "TakeDocumentPhoto: optionsJson is empty; expected {\"scalePercent\": 1..100, \"currentPhoto\": base64|null}");
        assert_eq!(msg("[1,2]"), "TakeDocumentPhoto: optionsJson must be a JSON object");
        assert_eq!(msg("42"), "TakeDocumentPhoto: optionsJson must be a JSON object");
        assert!(msg("{not json").starts_with("TakeDocumentPhoto: optionsJson is not valid JSON: "));
    }

    #[test]
    fn member_names_case_insensitive_and_unknown_ignored() {
        // The `share.RunPlugin` request doubles as the options; `method` is ignored.
        let o = DocumentPhotoOptions::parse(r#"{"method":"takeDocumentPhoto","SCALEPERCENT":30}"#).unwrap();
        assert_eq!(o.scale_percent, 30);
    }

    #[test]
    fn current_photo_null_and_empty_are_none() {
        assert_eq!(DocumentPhotoOptions::parse(r#"{"scalePercent":30,"currentPhoto":null}"#).unwrap().current_photo, None);
        assert_eq!(DocumentPhotoOptions::parse(r#"{"scalePercent":30,"currentPhoto":""}"#).unwrap().current_photo, None);
    }

    #[test]
    fn current_photo_valid_base64_is_kept_verbatim() {
        let o = DocumentPhotoOptions::parse(r#"{"scalePercent":30,"currentPhoto":"Zm9v"}"#).unwrap();
        assert_eq!(o.current_photo.as_deref(), Some("Zm9v"));
    }

    #[test]
    fn current_photo_bad_type_and_bad_base64() {
        assert_eq!(msg(r#"{"scalePercent":30,"currentPhoto":5}"#), "TakeDocumentPhoto: currentPhoto must be a base64 string or null");
        assert_eq!(msg(r#"{"scalePercent":30,"currentPhoto":"not valid!!"}"#), "TakeDocumentPhoto: currentPhoto is not valid base64");
    }
}
