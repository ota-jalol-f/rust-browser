//! `sentinel-ekalit`: the `Asbt.eKalit` plugin (x64) — the in-process Rust
//! equivalent of the reference `Asbt.EKalitWebV6.Plugin`'s `ekalit` host object
//! (E-IMZO / E-Kalit USB-token signing).
//!
//! pv-next calls `chrome.webview.hostObjects.ekalit`:
//! - `EKalitId()` → the token UID (no PIN; a plain call).
//! - `SignStringV1(value)` / `SignStringV2(data)` → sign; open the PIN overlay.
//! - `GetCertificateInfo()` → the signer certificate; open the PIN overlay.
//!
//! The device-free logic is in `sentinel-ekalit-core`. The token backend needs the
//! vendor NATIVE SDK (the source only has the managed `libesigner.net` wrapper), so
//! a [`signer_stub::StubSigner`] drives every build; signing returns the reference's
//! "E-Kalit уланмаган" Answere.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod overlay;
pub mod signer_stub;

use std::cell::Cell;
use std::rc::Rc;

use serde_json::Value;
use sentinel_ekalit_core::answere::answere_json;
use sentinel_ekalit_core::signer::Signer;
use sentinel_ekalit_core::texts;
use sentinel_plugin::{HostMethod, Plugin, PluginDescriptor, PluginKind, PluginResult, PluginUi};

use crate::overlay::{BusyGuard, PinOp, PinUi};
use crate::signer_stub::StubSigner;

/// Which `ekalit` method a request resolves to.
#[derive(Debug, Clone, PartialEq, Eq)]
enum EKalitCall {
    EKalitId,
    SignV1(String),
    SignV2(String),
    Cert,
}

/// The eKalit plugin.
pub struct EKalitPlugin {
    busy: Rc<Cell<bool>>,
}

impl EKalitPlugin {
    /// Create the eKalit plugin.
    pub fn new() -> Self {
        EKalitPlugin { busy: Rc::new(Cell::new(false)) }
    }
}

impl Default for EKalitPlugin {
    fn default() -> Self {
        Self::new()
    }
}

/// The signer for one call: the stub until a native libesigner backend exists.
fn make_signer() -> Box<dyn Signer> {
    Box::new(StubSigner)
}

/// `EKalitId()`: the token UID as an `Answere<string>` JSON (no PIN needed).
fn ekalit_id() -> String {
    let mut signer = make_signer();
    match signer.get_uid() {
        Ok(uid) => answere_json(1, texts::OK, "", Some(Value::String(uid))),
        Err(e) => answere_json(0, texts::EKALIT_ID_ERROR, &e, None),
    }
}

/// Classify a request into an [`EKalitCall`]. Unknown / empty / `share.RunPlugin`
/// defaults to `EKalitId` (the reference `ExecuteAsync` always returns it).
fn classify(request: &str) -> EKalitCall {
    let value: Value = match serde_json::from_str(request) {
        Ok(v) => v,
        Err(_) => return EKalitCall::EKalitId,
    };
    let object = match value.as_object() {
        Some(o) => o,
        None => return EKalitCall::EKalitId,
    };
    let method = object
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("method"))
        .and_then(|(_, v)| v.as_str())
        .unwrap_or("");
    let arg0 = object
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("args"))
        .and_then(|(_, v)| v.as_array())
        .and_then(|a| a.first());

    if method.eq_ignore_ascii_case("signStringV1") {
        EKalitCall::SignV1(arg0.and_then(Value::as_str).unwrap_or("").to_string())
    } else if method.eq_ignore_ascii_case("signStringV2") {
        // The data may be a JSON string or an object; keep its JSON text.
        let raw = match arg0 {
            Some(Value::String(s)) => s.clone(),
            Some(v) => v.to_string(),
            None => String::new(),
        };
        EKalitCall::SignV2(raw)
    } else if method.eq_ignore_ascii_case("getCertificateInfo") {
        EKalitCall::Cert
    } else {
        EKalitCall::EKalitId
    }
}

impl Plugin for EKalitPlugin {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor {
            id: "Asbt.eKalit".to_string(),
            name: "ekalit".to_string(),
            version: "1.0.0".to_string(),
            kind: PluginKind::Ui,
        }
    }

    fn host_methods(&self) -> Vec<HostMethod> {
        vec![
            HostMethod::run("EKalitId"),
            HostMethod::ui("SignStringV1"),
            HostMethod::ui("SignStringV2"),
            HostMethod::ui("GetCertificateInfo"),
        ]
    }

    fn execute(&mut self, request: &str) -> PluginResult {
        match classify(request) {
            EKalitCall::EKalitId => PluginResult::ok(ekalit_id()),
            // The signing methods are UI; reached here only as a fallback.
            EKalitCall::SignV1(_) | EKalitCall::SignV2(_) => {
                PluginResult::ok(answere_json(0, texts::SIGN_ERROR, "UI orqali chaqiring", None))
            }
            EKalitCall::Cert => {
                PluginResult::ok(answere_json(0, texts::CERT_INFO_ERROR, "UI orqali chaqiring", None))
            }
        }
    }

    fn execute_ui(&mut self, request: &str) -> Option<Box<dyn PluginUi>> {
        let op = match classify(request) {
            EKalitCall::EKalitId => return None, // no PIN → the plain call
            EKalitCall::SignV1(v) => PinOp::SignV1(v),
            EKalitCall::SignV2(v) => PinOp::SignV2(v),
            EKalitCall::Cert => PinOp::Cert,
        };
        if self.busy.get() {
            return None;
        }
        let guard = BusyGuard::acquire(&self.busy);
        Some(Box::new(PinUi::new(op, make_signer(), guard)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sentinel_plugin::UiStep;

    #[test]
    fn descriptor_is_ekalit_1_0_0_ui() {
        let d = EKalitPlugin::new().descriptor();
        assert_eq!(d.id, "Asbt.eKalit");
        assert_eq!(d.name, "ekalit");
        assert_eq!(d.version, "1.0.0");
        assert_eq!(d.kind, PluginKind::Ui);
    }

    #[test]
    fn host_methods_match_pv_next() {
        let m: Vec<(String, bool)> =
            EKalitPlugin::new().host_methods().into_iter().map(|h| (h.js, h.ui)).collect();
        assert_eq!(
            m,
            [
                ("EKalitId".to_string(), false),
                ("SignStringV1".to_string(), true),
                ("SignStringV2".to_string(), true),
                ("GetCertificateInfo".to_string(), true),
            ]
        );
    }

    #[test]
    fn ekalit_id_without_token_is_the_error_answere() {
        let mut p = EKalitPlugin::new();
        let r = p.execute(r#"{"method":"EKalitId","args":[]}"#);
        assert!(r.success);
        assert!(r.data.contains("E-Kalit рақамини олишда ҳато"));
        assert!(r.data.contains("E-Kalit уланмаган"));
        assert!(r.data.contains("\"AnswereId\":0"));
    }

    #[test]
    fn run_plugin_default_is_ekalit_id() {
        let mut p = EKalitPlugin::new();
        // share.RunPlugin with no method → EKalitId (as ExecuteAsync always does).
        assert!(p.execute("{}").data.contains("E-Kalit рақамини олишда ҳато"));
    }

    #[test]
    fn sign_v1_opens_pin_overlay_then_stub_returns_sign_error() {
        let mut p = EKalitPlugin::new();
        let req = r#"{"method":"SignStringV1","args":["hello"]}"#;
        let mut ui = p.execute_ui(req).expect("overlay");
        assert!(ui.html().contains("window.ui"));
        // busy now.
        assert!(p.execute_ui(req).is_none());
        // labels first, then the PIN resolves with the stub's "not connected" Answere.
        assert!(matches!(ui.poll(), UiStep::Update(_)));
        ui.on_message("pin:1234");
        match ui.poll() {
            UiStep::Done(r) => {
                assert!(r.success);
                assert!(r.data.contains("Имзолашда ҳато"));
                assert!(r.data.contains("E-Kalit уланмаган"));
            }
            _ => panic!("expected completion"),
        }
        drop(ui);
        assert!(p.execute_ui(req).is_some()); // busy cleared
    }

    #[test]
    fn empty_pin_is_cancel() {
        let mut p = EKalitPlugin::new();
        let mut ui = p.execute_ui(r#"{"method":"SignStringV2","args":["{\"a\":1}"]}"#).expect("overlay");
        let _ = ui.poll();
        ui.on_message("pin:");
        match ui.poll() {
            UiStep::Done(r) => assert!(r.data.contains("Амал бекор қилинди") && r.data.contains("\"AnswereId\":10")),
            _ => panic!("expected cancel"),
        }
    }

    #[test]
    fn cancel_message_is_cancel_answere() {
        let mut p = EKalitPlugin::new();
        let mut ui = p.execute_ui(r#"{"method":"GetCertificateInfo","args":[]}"#).expect("overlay");
        ui.on_message("cancel");
        match ui.poll() {
            UiStep::Done(r) => assert!(r.data.contains("Амал бекор қилинди")),
            _ => panic!("expected cancel"),
        }
    }
}
