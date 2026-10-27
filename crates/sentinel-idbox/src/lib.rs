//! `sentinel-idbox`: the Elyctis ID BOX reader plugin (`Asbt.IdBox`), the in-process
//! Rust equivalent of the reference `Asbt.IdBox.Plugin`'s `IdBoxObj` host object.
//!
//! pv-next drives it through the JS host object `chrome.webview.hostObjects.idbox`
//! (see `sentinel-plugin`'s host-method bridge), calling:
//! - `ReadChipUid()` → the contactless card UID over **x64 PC/SC** (standard
//!   `winscard`, no vendor SDK): APDU `FF CA 00 00 00`, the 10-byte response hex
//!   lower-cased and cut to 14 chars (`data`), or `""` on any error — exactly the
//!   reference `PcScChipUidReader` + `UidFormatter`. `success` stays `true`.
//! - `ReadDocument()` → the MRZ + chip read. The reference opened a modal window;
//!   by deployment requirement we do NOT — idbox works purely through the plugin
//!   API, and the front-end owns any UI. The read (MRZ over serial, then the chip
//!   in an **x86 helper** because `ElyMRTD.dll` is x86-only) will stream status
//!   over the plugin API and resolve with the document envelope. It needs the
//!   device + serial scanner + vendor SDK, so it is a documented stub here
//!   (device-missing failure) until that flow lands.
//!
//! The generic `share.RunPlugin("Asbt.IdBox", {"method":"readChipUid"})` request
//! shape is honoured too (case-insensitive `method`), matching the reference's
//! `ExecuteAsync` dispatch.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod serial;
pub mod session;

#[cfg(feature = "device")]
mod device;

use sentinel_plugin::{HostMethod, Plugin, PluginDescriptor, PluginKind, PluginResult};

/// The idbox reader plugin.
pub struct IdBoxPlugin;

impl IdBoxPlugin {
    /// Create the idbox plugin.
    pub fn new() -> Self {
        IdBoxPlugin
    }

    /// `ReadChipUid`: the card UID as 14 lowercase hex chars, or `""` when there is
    /// no reader/card/service. Never fails on a PC/SC error (matches the reference:
    /// the error is swallowed to `""`, `success` stays `true`).
    fn read_chip_uid(&self) -> PluginResult {
        PluginResult::ok(uid_via_pcsc().unwrap_or_default())
    }

    /// `ReadDocument`: MRZ (serial) + chip (x86 helper) read, returned through the
    /// plugin API with NO modal window — the front-end owns the UI, and the read runs
    /// start-to-finish inside this call ([`session::read_document`]), streaming status
    /// to the host log. Success → the Answere envelope in `data`; no data → `success`
    /// false with an empty message (the front-end may retry).
    ///
    /// Needs the real devices, so the flow is compiled under the `device` feature; a
    /// default build answers with a clear "not compiled" message (the host method is
    /// always present so pv-next still sees `idbox.ReadDocument`).
    fn read_document(&self) -> PluginResult {
        read_document_impl()
    }
}

/// The real `device`-feature flow, or the not-compiled message otherwise.
#[cfg(feature = "device")]
fn read_document_impl() -> PluginResult {
    use sentinel_idbox_core::reader_result::ReaderResultBuilder;
    use session::ReadOutcome;

    let devices = device::RealDevices::new();
    let mut view = session::CollectingView::default();
    let builder = ReaderResultBuilder::new(current_year());
    let outcome = session::read_document(&devices, &mut view, &builder, &mut |m| {
        eprintln!("[idbox] {m}");
    });
    for status in &view.statuses {
        eprintln!("[idbox] status: {status}");
    }
    match outcome {
        ReadOutcome::Data(json) => PluginResult::ok(json),
        ReadOutcome::Cancel => PluginResult::fail(""),
    }
}

/// Without the `device` feature the vendor SDK, serial scanner and PC/SC are not linked.
#[cfg(not(feature = "device"))]
fn read_document_impl() -> PluginResult {
    PluginResult::fail("ID BOX hujjat o'qish `device` feature bilan qurilishi va qurilma ulanishi kerak")
}

/// `DateTime.Today.Year` for the `CMRZ` century heuristic, computed from the system
/// clock without a date crate (Howard Hinnant's days-from-civil, inverted).
#[cfg(feature = "device")]
fn current_year() -> i32 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) as i64;
    let z = secs / 86_400 + 719_468; // days since 0000-03-01
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    (y + i64::from(month <= 2)) as i32
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

    fn host_methods(&self) -> Vec<HostMethod> {
        // pv-next calls hostObjects.idbox.ReadChipUid() / .ReadDocument() directly.
        // Both are plain plugin-API calls (`run`, never a modal overlay — by
        // deployment requirement): ReadDocument will stream status over the plugin
        // API while the serial MRZ + x86 chip read runs, then resolve the envelope.
        vec![HostMethod::run("ReadChipUid"), HostMethod::run("ReadDocument")]
    }

    fn execute(&mut self, request: &str) -> PluginResult {
        // `{"method":"readChipUid"}` / `{"method":"ReadChipUid",…}` (case-insensitive)
        // → ReadChipUid; anything else (incl. ReadDocument, empty, non-JSON) → ReadDocument.
        let method = serde_json::from_str::<serde_json::Value>(request)
            .ok()
            .and_then(|v| v.get("method").and_then(|m| m.as_str()).map(str::to_string));
        match method.as_deref() {
            Some(m) if m.eq_ignore_ascii_case("readChipUid") => self.read_chip_uid(),
            _ => self.read_document(),
        }
    }
}

/// Format a PC/SC GET-UID response the way the reference `UidFormatter` does: hex-
/// encode the WHOLE receive buffer (response + any zero padding), lowercase, and
/// keep the first 14 chars. A 7-byte UID gives its 14 hex digits; a 4-byte UID
/// gives 8 digits + the status word + padding.
fn format_uid(buf: &[u8]) -> String {
    let hex: String = buf.iter().map(|b| format!("{b:02x}")).collect();
    hex.chars().take(14).collect()
}

/// Read the contactless UID of the card on the first PC/SC reader over the OS
/// `winscard` service. `Err` on any failure (no service, no reader, no card, an
/// over-long response); the caller maps that to `""` like the reference.
#[cfg(windows)]
fn uid_via_pcsc() -> Result<String, String> {
    use pcsc::{Context, Protocols, Scope, ShareMode};

    const GET_UID: [u8; 5] = [0xFF, 0xCA, 0x00, 0x00, 0x00];

    let ctx = Context::establish(Scope::System).map_err(|e| e.to_string())?;
    let mut readers_buf = [0u8; 2048];
    let reader = ctx
        .list_readers(&mut readers_buf)
        .map_err(|e| e.to_string())?
        .next()
        .ok_or_else(|| "no smart card readers".to_string())?;
    let card = ctx
        .connect(reader, ShareMode::Shared, Protocols::T0 | Protocols::T1)
        .map_err(|e| e.to_string())?;
    // A 10-byte receive buffer, zero-padded after the response, matching the
    // reference. A response longer than 10 bytes errors out (→ "").
    let mut buf = [0u8; 10];
    let _ = card.transmit(&GET_UID, &mut buf).map_err(|e| e.to_string())?;
    Ok(format_uid(&buf))
}

/// Non-Windows builds have no `winscard`; the UID read yields `""` (the device
/// target is Windows). Keeps the crate building and testing on any host.
#[cfg(not(windows))]
fn uid_via_pcsc() -> Result<String, String> {
    Ok(String::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Vectors ported from the reference PcScTests (UidFormatter).
    #[test]
    fn uid_seven_byte_is_its_fourteen_hex_digits() {
        let res = [0x04, 0xA1, 0xB2, 0xC3, 0xD4, 0xE5, 0xF6, 0x90, 0x00, 0x00];
        assert_eq!(format_uid(&res), "04a1b2c3d4e5f6");
    }

    #[test]
    fn uid_four_byte_includes_status_word_and_padding() {
        let res = [0xDE, 0xAD, 0xBE, 0xEF, 0x90, 0x00, 0x00, 0x00, 0x00, 0x00];
        assert_eq!(format_uid(&res), "deadbeef900000");
    }

    #[test]
    fn uid_empty_buffer_is_empty() {
        assert_eq!(format_uid(&[]), "");
    }

    #[test]
    fn read_chip_uid_never_fails_and_is_empty_or_14_hex() {
        // No reader on the dev/CI machine → "" (success stays true), exactly the
        // reference's ReadUid_onThisMachine contract. With a reader+card it is 14 hex.
        let mut p = IdBoxPlugin::new();
        let r = p.execute(r#"{"method":"readChipUid"}"#);
        assert!(r.success);
        assert!(r.error_message.is_empty());
        assert!(r.data.is_empty() || (r.data.len() == 14 && r.data.chars().all(|c| c.is_ascii_hexdigit())));
    }

    #[test]
    fn method_is_case_insensitive() {
        let mut p = IdBoxPlugin::new();
        let r = p.execute(r#"{"method":"READCHIPUID"}"#);
        assert!(r.success);
        assert!(r.data.is_empty() || r.data.len() == 14);
    }

    #[test]
    fn read_document_is_a_device_missing_failure_stub() {
        let mut p = IdBoxPlugin::new();
        let r = p.execute("{}");
        assert!(!r.success);
        assert!(!r.error_message.is_empty());
    }

    #[test]
    fn host_methods_match_pv_next() {
        let p = IdBoxPlugin::new();
        let names: Vec<String> = p.host_methods().into_iter().map(|m| m.js).collect();
        assert_eq!(names, ["ReadChipUid", "ReadDocument"]);
    }
}
