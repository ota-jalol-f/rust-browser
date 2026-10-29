//! `fingerprints.Enroll(optionsJson)` options: `{"secondScreen"?, "previewOnSecondScreen"?}`.
//! Both flags are parsed, then ignored (the second screen is deferred, decision 3);
//! an invalid options JSON is an error and the window does not open.

use serde_json::Value;

/// A rejected options JSON (the message is surfaced as the `error_message`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptionsError(pub String);

impl OptionsError {
    /// The validation message.
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

/// The parsed enroll options.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EnrollOptions {
    /// `secondScreen` (accepted, ignored).
    pub second_screen: bool,
    /// `previewOnSecondScreen` (accepted, ignored).
    pub preview_on_second_screen: bool,
}

impl EnrollOptions {
    /// Parse + validate. `null` / `""` / whitespace → defaults; anything that is not
    /// a JSON object is an error; unknown keys are ignored.
    pub fn parse(options_json: &str) -> Result<EnrollOptions, OptionsError> {
        if options_json.trim().is_empty() {
            return Ok(EnrollOptions::default());
        }
        let value: Value = serde_json::from_str(options_json)
            .map_err(|e| OptionsError(format!("optionsJson is not valid JSON: {e}")))?;
        let obj = match &value {
            Value::Object(map) => map,
            _ => return Err(OptionsError("optionsJson must be a JSON object.".into())),
        };
        Ok(EnrollOptions {
            second_screen: read_bool(obj, "secondScreen"),
            preview_on_second_screen: read_bool(obj, "previewOnSecondScreen"),
        })
    }
}

/// `obj.GetValue(name, OrdinalIgnoreCase)` restricted to a real JSON boolean.
fn read_bool(obj: &serde_json::Map<String, Value>, name: &str) -> bool {
    obj.iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .and_then(|(_, v)| v.as_bool())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_is_default() {
        assert_eq!(EnrollOptions::parse("").unwrap(), EnrollOptions::default());
        assert_eq!(EnrollOptions::parse("   ").unwrap(), EnrollOptions::default());
        assert_eq!(EnrollOptions::parse("{}").unwrap(), EnrollOptions::default());
    }

    #[test]
    fn flags_parsed_case_insensitive() {
        let o = EnrollOptions::parse(r#"{"SecondScreen":true,"previewOnSecondScreen":true}"#).unwrap();
        assert!(o.second_screen);
        assert!(o.preview_on_second_screen);
    }

    #[test]
    fn non_bool_is_false() {
        let o = EnrollOptions::parse(r#"{"secondScreen":"yes"}"#).unwrap();
        assert!(!o.second_screen);
    }

    #[test]
    fn non_object_and_bad_json_are_errors() {
        assert_eq!(EnrollOptions::parse("[1]").unwrap_err().0, "optionsJson must be a JSON object.");
        assert!(EnrollOptions::parse("{bad").unwrap_err().0.starts_with("optionsJson is not valid JSON: "));
    }
}
