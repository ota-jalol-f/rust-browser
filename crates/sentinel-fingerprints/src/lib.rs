//! `sentinel-fingerprints`: the `Asbt.Fingerprints` plugin (x64) — the in-process
//! Rust equivalent of the reference `Asbt.Fingerprints.Plugin`'s `fingerprints`
//! host object.
//!
//! pv-next drives it through `chrome.webview.hostObjects.fingerprints`:
//! - `Enroll(optionsJson)` (1.1.0) → the 10-finger enrollment overlay
//!   ([`overlay::EnrollUi`]); routed through the shell's UI path.
//! - `Scan()` (1.0.0 single flat finger) → a documented stub (not ported).
//!
//! The device-free logic is in `sentinel-fingerprints-core`; the Suprema RealScan
//! device (RS_SDK FFI + WSQ) is the next increment, with [`device_stub`] everywhere
//! until then.

// `deny` (not `forbid`) so a future RealScan FFI module can opt into `unsafe`.
#![deny(unsafe_code)]
#![warn(missing_docs)]

pub mod device_stub;
pub mod overlay;

use sentinel_fingerprints_core::sdk::RealScanDevice;
use sentinel_fingerprints_core::{classify, EnrollOptions, FingerprintsCall};
use sentinel_plugin::{HostMethod, Plugin, PluginDescriptor, PluginKind, PluginResult, PluginUi};

use crate::device_stub::StubDevice;
use crate::overlay::EnrollUi;

/// The Fingerprints plugin.
pub struct FingerprintsPlugin;

impl FingerprintsPlugin {
    /// Create the Fingerprints plugin.
    pub fn new() -> Self {
        FingerprintsPlugin
    }
}

impl Default for FingerprintsPlugin {
    fn default() -> Self {
        Self::new()
    }
}

/// The 1.0.0 `Scan()` single-flat-finger path is not ported.
fn scan_stub() -> PluginResult {
    PluginResult::fail("Scan (1.0.0) hali ko'chirilmagan; Enroll'dan foydalaning")
}

/// The RealScan device for one overlay: the stub until the RS_SDK FFI lands.
fn make_device() -> Box<dyn RealScanDevice> {
    Box::new(StubDevice)
}

impl Plugin for FingerprintsPlugin {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor {
            id: "Asbt.Fingerprints".to_string(),
            name: "fingerprints".to_string(),
            // 1.1.0: the page feature-detects Enroll.
            version: "1.1.0".to_string(),
            kind: PluginKind::Ui,
        }
    }

    fn host_methods(&self) -> Vec<HostMethod> {
        // pv-next calls hostObjects.fingerprints.Enroll(opts) / .Scan().
        vec![HostMethod::run("Scan"), HostMethod::ui("Enroll")]
    }

    fn execute(&mut self, request: &str) -> PluginResult {
        match classify(request) {
            FingerprintsCall::Scan => scan_stub(),
            FingerprintsCall::Enroll(options_json) => {
                // Reached only when execute_ui returned None (bad options) or on the
                // non-UI RunPlugin path.
                match EnrollOptions::parse(&options_json) {
                    Err(e) => PluginResult::fail(e.message()),
                    Ok(_) => PluginResult::fail(
                        "Enroll UI oynasi orqali chaqirilishi kerak (hostObjects.fingerprints.Enroll)",
                    ),
                }
            }
        }
    }

    fn execute_ui(&mut self, request: &str) -> Option<Box<dyn PluginUi>> {
        match classify(request) {
            FingerprintsCall::Enroll(options_json) => {
                // Validate the options (second screen is deferred → otherwise ignored).
                EnrollOptions::parse(&options_json).ok()?;
                Some(Box::new(EnrollUi::new(make_device())))
            }
            FingerprintsCall::Scan => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sentinel_plugin::UiStep;

    #[test]
    fn descriptor_is_fingerprints_1_1_0_ui() {
        let d = FingerprintsPlugin::new().descriptor();
        assert_eq!(d.id, "Asbt.Fingerprints");
        assert_eq!(d.name, "fingerprints");
        assert_eq!(d.version, "1.1.0");
        assert_eq!(d.kind, PluginKind::Ui);
    }

    #[test]
    fn host_methods_match_pv_next() {
        let m: Vec<(String, bool)> =
            FingerprintsPlugin::new().host_methods().into_iter().map(|h| (h.js, h.ui)).collect();
        assert_eq!(m, [("Scan".to_string(), false), ("Enroll".to_string(), true)]);
    }

    #[test]
    fn scan_is_a_stub() {
        let mut p = FingerprintsPlugin::new();
        let r = p.execute(r#"{"method":"Scan","args":[]}"#);
        assert!(!r.success);
        assert!(r.error_message.contains("Scan"));
    }

    #[test]
    fn bad_options_fail_with_message() {
        let mut p = FingerprintsPlugin::new();
        let req = r#"{"method":"Enroll","args":["[1,2]"]}"#;
        assert!(p.execute_ui(req).is_none());
        assert_eq!(p.execute(req).error_message, "optionsJson must be a JSON object.");
    }

    #[test]
    fn enroll_opens_overlay_and_no_scanner_lets_the_user_cancel() {
        let mut p = FingerprintsPlugin::new();
        let mut ui = p.execute_ui(r#"{"method":"Enroll","args":["{}"]}"#).expect("overlay");
        assert!(ui.html().contains("window.ui"));
        // First poll pushes a snapshot; with the stub device the status is a
        // no-scanner dico message (not a crash).
        match ui.poll() {
            UiStep::Update(j) => {
                assert!(j.contains("\"status\""));
                assert!(j.contains("fingerprints") || j.contains("labels"));
            }
            _ => panic!("expected an initial snapshot"),
        }
        // Exit → cancel.
        ui.on_message("cancel");
        match ui.poll() {
            UiStep::Done(r) => {
                assert!(!r.success);
                assert!(r.error_message.is_empty());
            }
            _ => panic!("expected cancel"),
        }
    }
}
