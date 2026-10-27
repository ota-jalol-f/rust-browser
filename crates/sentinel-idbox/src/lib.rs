//! `sentinel-idbox`: the Elyctis ID BOX chip-reader plugin (`Asbt.IdBox`).
//!
//! API mirrors the reference `idbox` host object:
//! - `{"method":"readChipUid"}` → [`PluginResult`] whose `data` is 14 lowercase
//!   hex chars (the PC/SC card UID), or `""` when no card/reader.
//! - anything else → `ReadDocument`: `data` is the reader-result JSON string
//!   (the consumer uses `Data.DocumentNumber` and `Data.BirthDate`).
//!
//! The real device path FFIs the x86-native `ElyMRTD.dll` /`ElyRdrControl.dll`
//! inside an x86 helper (`connect` / `establishBAC` / `getDG1` / `getDG2` / …) and
//! will live behind a `device` feature (licensed vendor DLLs + i686 + hardware).
//! The default build is a FAKE so the plugin and the host↔helper↔registry path are
//! testable without a device.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use sentinel_plugin::{Plugin, PluginDescriptor, PluginKind, PluginResult};

/// The idbox plugin. The fake device returns canned, correctly-shaped results.
pub struct IdBoxPlugin;

impl IdBoxPlugin {
    /// Create the (fake-device) idbox plugin.
    pub fn new() -> Self {
        IdBoxPlugin
    }

    /// `ReadChipUid`: 14 lowercase hex chars of the card UID (fake), or `""`.
    fn read_chip_uid(&self) -> PluginResult {
        // Fake 7-byte UID → 14 hex chars, as a real Uzbek ID card would give.
        PluginResult::ok("04a1b2c3d4e5f6")
    }

    /// `ReadDocument`: the reader-result JSON envelope (fake MRZ-fallback shape).
    fn read_document(&self) -> PluginResult {
        // Only Data.DocumentNumber and Data.BirthDate are consumed downstream.
        let data = r#"{"AnswereId":1,"AnswereMessage":"Hujjat o'qildi","AnswereComment":"FAKE",
"Data":{"DocumentNumber":"AA1234567","BirthDate":"01.01.1990","Surname":"TOSHMATOV",
"Name":"ALISHER","Nationality":"UZB","Sex":"M","InformationFrom":1,"PersonPhoto":null}}"#;
        PluginResult::ok(data)
    }
}

impl Default for IdBoxPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl Plugin for IdBoxPlugin {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor {
            id: "Asbt.IdBox".to_string(),
            name: "idbox".to_string(),
            version: "1.0.0".to_string(),
            kind: PluginKind::Ui,
        }
    }

    fn execute(&mut self, request: &str) -> PluginResult {
        // `{"method":"readChipUid"}` (case-insensitive) → ReadChipUid; else ReadDocument.
        let method = serde_json::from_str::<serde_json::Value>(request)
            .ok()
            .and_then(|v| v.get("method").and_then(|m| m.as_str()).map(str::to_string));
        match method.as_deref() {
            Some(m) if m.eq_ignore_ascii_case("readChipUid") => self.read_chip_uid(),
            _ => self.read_document(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_chip_uid_returns_14_hex() {
        let mut p = IdBoxPlugin::new();
        let r = p.execute(r#"{"method":"readChipUid"}"#);
        assert!(r.success);
        assert_eq!(r.data.len(), 14);
        assert!(r.data.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn read_document_returns_doc_json() {
        let mut p = IdBoxPlugin::new();
        let r = p.execute("{}");
        assert!(r.success);
        assert!(r.data.contains("DocumentNumber"));
        assert!(r.data.contains("BirthDate"));
        // The `data` is itself valid JSON (parses).
        assert!(serde_json::from_str::<serde_json::Value>(&r.data).is_ok());
    }

    #[test]
    fn method_is_case_insensitive() {
        let mut p = IdBoxPlugin::new();
        assert_eq!(p.execute(r#"{"method":"READCHIPUID"}"#).data.len(), 14);
    }
}
