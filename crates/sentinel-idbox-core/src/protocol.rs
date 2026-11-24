//! The wire protocol between the x64 plugin (pipe server) and the x86/i686 chip helper
//! (pipe client): one JSON object per line, UTF-8 without BOM, `'\n'`-terminated.
//!
//! The plugin sends [`HelperRequest`] lines (`readChip` / `closeComm` / `quit`); the
//! helper answers each with zero or more `status`/`alert` lines and exactly one
//! `result` line carrying the request id, after an initial `hello`. Ported from the
//! reference `Asbt.IdBox.Shared.HelperProtocol` + `HelperRequest` + `HelperMessage`
//! (the JSON property names are kept identical so a Rust end can talk to the C# end).

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

/// Protocol version.
pub const VERSION: i32 = 1;
/// Pipe-name prefix.
pub const PIPE_PREFIX: &str = "Asbt.IdBox.ChipHelper.";

/// `type` values (helper → plugin) and the parallel helper identity.
pub struct MessageType;

impl MessageType {
    /// First line: the helper is up (`ok` says whether the SDK was created).
    pub const HELLO: &'static str = "hello";
    /// A status-line update.
    pub const STATUS: &'static str = "status";
    /// A toast (caption + text).
    pub const ALERT: &'static str = "alert";
    /// The single terminal message of a request.
    pub const RESULT: &'static str = "result";
}

/// `cmd` values (plugin → helper).
pub struct Command;

impl Command {
    /// `ElyRDRControl.RDR_CloseComm()` after the antenna scan.
    pub const CLOSE_COMM: &'static str = "closeComm";
    /// Read the chip (`reader` + `mrzInfo`).
    pub const READ_CHIP: &'static str = "readChip";
    /// Shut the helper down.
    pub const QUIT: &'static str = "quit";
}

/// plugin → helper.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HelperRequest {
    /// The request id, echoed on the matching `result`.
    #[serde(default)]
    pub id: i32,
    /// [`Command`] value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cmd: Option<String>,
    /// `readChip`: the PC/SC reader name the antenna scan matched.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reader: Option<String>,
    /// `readChip`: `MRZ.MRZ_info` (the BAC key seed).
    #[serde(default, rename = "mrzInfo", skip_serializing_if = "Option::is_none")]
    pub mrz_info: Option<String>,
}

/// helper → plugin. One shape for every line; [`HelperMessage::type_`] tells which
/// fields are set. Null fields are omitted on the wire (Newtonsoft `NullValueHandling.Ignore`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct HelperMessage {
    /// [`MessageType`] value.
    #[serde(rename = "type")]
    pub type_: String,
    /// status/alert/result: the request id (0 for hello).
    #[serde(default)]
    pub id: i32,

    // ---- hello ----
    /// hello: protocol version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protocol: Option<i32>,
    /// hello: `false` when the SDK could not be created; closeComm: success flag.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ok: Option<bool>,
    /// hello: why the SDK failed; closeComm: the exception text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// hello: the SCard manager / antenna selector failed.
    #[serde(default, rename = "readerInitError", skip_serializing_if = "Option::is_none")]
    pub reader_init_error: Option<String>,

    // ---- status / alert ----
    /// status/alert: the text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// status: `true` = red error line.
    #[serde(default, rename = "isError", skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
    /// alert: the toast caption.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caption: Option<String>,

    // ---- result ----
    /// result: `None` = e.Result never set (connect -1 / unknown applet), 0 = read,
    /// -1 = outer exception, -2 = exception during BAC/DG reading.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<i32>,
    /// result: the exception text for -1/-2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// result: `APPLET_TYPES` (0 ICAO, 1 IDl).
    #[serde(default, rename = "appletType", skip_serializing_if = "Option::is_none")]
    pub applet_type: Option<i32>,
    /// result: the DG1 strings (ICAO 10, IDl 8), empty when DG1 was not read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dg1: Option<Vec<String>>,
    /// result (IDl): `false` when the category list stayed null.
    #[serde(default, rename = "categoriesRead", skip_serializing_if = "Option::is_none")]
    pub categories_read: Option<bool>,
    /// result: the DG2/DG6 face image as base64 JPEG.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub photo: Option<String>,
    /// result: the decoded photo could not be re-encoded.
    #[serde(default, rename = "photoError", skip_serializing_if = "Option::is_none")]
    pub photo_error: Option<bool>,
    /// result: an exception escaped `DoWork` (the helper state is then suspect).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fault: Option<String>,
}

impl HelperMessage {
    /// A `status` line.
    pub fn status(id: i32, text: &str, is_error: bool) -> Self {
        HelperMessage {
            type_: MessageType::STATUS.to_string(),
            id,
            text: Some(text.to_string()),
            is_error: Some(is_error),
            ..Default::default()
        }
    }

    /// An `alert` line.
    pub fn alert(id: i32, caption: &str, text: &str) -> Self {
        HelperMessage {
            type_: MessageType::ALERT.to_string(),
            id,
            caption: Some(caption.to_string()),
            text: Some(text.to_string()),
            ..Default::default()
        }
    }

    /// A bare `result` line for request `id`.
    pub fn result(id: i32) -> Self {
        HelperMessage {
            type_: MessageType::RESULT.to_string(),
            id,
            ..Default::default()
        }
    }
}

/// `HelperProtocol.Serialize`: one compact JSON line (no trailing newline).
pub fn serialize<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value).expect("protocol messages serialize")
}

/// `HelperProtocol.Deserialize`.
pub fn deserialize<T: DeserializeOwned>(line: &str) -> serde_json::Result<T> {
    serde_json::from_str(line)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_round_trips_and_omits_nulls() {
        let m = HelperMessage::status(7, "DG1 ўқилди", false);
        let line = serialize(&m);
        assert!(line.contains("\"type\":\"status\""));
        assert!(line.contains("\"isError\":false"));
        assert!(!line.contains("code"));
        let back: HelperMessage = deserialize(&line).unwrap();
        assert_eq!(back, m);
    }

    #[test]
    fn request_round_trips() {
        let r = HelperRequest {
            id: 3,
            cmd: Some(Command::READ_CHIP.to_string()),
            reader: Some("eader 0123456789ABCDEF 0".to_string()),
            mrz_info: Some("AA1234567890010113001019".to_string()),
        };
        let back: HelperRequest = deserialize(&serialize(&r)).unwrap();
        assert_eq!(back, r);
    }

    #[test]
    fn quit_omits_the_empty_fields() {
        let r = HelperRequest { cmd: Some(Command::QUIT.to_string()), ..Default::default() };
        let line = serialize(&r);
        assert_eq!(line, "{\"id\":0,\"cmd\":\"quit\"}");
    }
}
