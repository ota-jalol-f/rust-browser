//! Built-in Rust plugins registered into the shell's [`PluginRegistry`], and the
//! factory that assembles them. Hardware plugins (fingerprint, ID reader,
//! signature pad, camera) land here over the coming phases; the streaming
//! `Asbt.Reader` and modal `Asbt.Confirm` demos prove the `window.share` bridge
//! (streaming and modal UI) end to end alongside the real ported plugins.

use sentinel_plugin::{
    Plugin, PluginDescriptor, PluginKind, PluginRegistry, PluginResult, PluginStream, PluginUi,
    StreamStep, UiStep,
};

/// A demo STREAMING plugin, shaped like the idbox chip read: it emits a few
/// status chunks, then a final result. Proves `window.share.RunPluginStream`
/// end to end. Real hardware plugins replace this with device status / preview.
pub struct ReaderPlugin;

/// The running operation behind [`ReaderPlugin`]. One step per poll (one frame).
struct ReaderStream {
    step: u32,
}

impl PluginStream for ReaderStream {
    fn poll(&mut self) -> StreamStep {
        self.step += 1;
        match self.step {
            1 => StreamStep::Chunk(r#"{"status":"connecting"}"#.to_string()),
            2 => StreamStep::Chunk(r#"{"status":"reading"}"#.to_string()),
            3 => StreamStep::Chunk(r#"{"status":"complete"}"#.to_string()),
            _ => StreamStep::Done(PluginResult::ok(r#"{"read":true}"#)),
        }
    }
}

impl Plugin for ReaderPlugin {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor {
            id: "Asbt.Reader".to_string(),
            name: "reader".to_string(),
            version: "1.0.0".to_string(),
            kind: PluginKind::Ui,
        }
    }

    fn execute(&mut self, _request: &str) -> PluginResult {
        PluginResult::fail("Asbt.Reader is a streaming plugin; use RunPluginStream")
    }

    fn execute_stream(&mut self, _request: &str) -> Option<Box<dyn PluginStream>> {
        Some(Box::new(ReaderStream { step: 0 }))
    }
}

/// A demo MODAL UI plugin: shows a confirm dialog in a full-window overlay and
/// resolves with `confirmed` / cancel. Proves `window.share.RunPluginUi` end to
/// end. Real UI plugins (signature pad, ID reader, camera) replace this.
pub struct ConfirmPlugin;

/// The overlay's HTML. Opaque full window (no backdrop blending needed), a
/// centred card, a `#msg` filled from the host via `window.ui.onUpdate`
/// (`textContent`, never HTML), and OK / Cancel buttons that `window.ui.send`.
const CONFIRM_HTML: &str = r##"<!DOCTYPE html>
<html><head><meta charset="utf-8"><style>
  html, body { margin: 0; height: 100%; background: rgba(13,17,23,0.001);
    font-family: "Segoe UI", Arial, sans-serif; }
  #scrim { position: fixed; inset: 0; background: #0d1117e6;
    display: flex; align-items: center; justify-content: center; }
  #card { width: 360px; background: #20242b; color: #e6edf3; border: 1px solid #444c56;
    border-radius: 10px; padding: 20px; box-shadow: 0 10px 40px #000a; }
  #msg { font-size: 15px; line-height: 1.5; margin: 0 0 18px; min-height: 22px; }
  .row { display: flex; gap: 10px; justify-content: flex-end; }
  button { height: 34px; padding: 0 16px; border-radius: 6px; cursor: pointer; font-size: 13px;
    border: 1px solid #444c56; background: #2d333b; color: #cdd9e5; }
  button:hover { background: #373e47; }
  #ok { background: #d9772b; border-color: #d9772b; color: #fff; }
</style></head><body>
  <div id="scrim"><div id="card">
    <p id="msg"></p>
    <div class="row">
      <button id="cancel" onclick="window.ui && window.ui.send('cancel')">Bekor</button>
      <button id="ok" onclick="window.ui && window.ui.send('ok')">OK</button>
    </div>
  </div></div>
<script>
  window.ui = window.ui || {};
  window.ui.onUpdate = function (j) {
    try { document.getElementById('msg').textContent = JSON.parse(j).message || ''; } catch (e) {}
  };
</script></body></html>
"##;

struct ConfirmUi {
    answer: Option<PluginResult>,
    pushed: bool,
}

impl PluginUi for ConfirmUi {
    fn html(&self) -> String {
        CONFIRM_HTML.to_string()
    }
    fn on_message(&mut self, message: &str) {
        self.answer = Some(match message {
            "ok" => PluginResult::ok("confirmed"),
            _ => PluginResult::cancel(),
        });
    }
    fn poll(&mut self) -> UiStep {
        if let Some(r) = self.answer.take() {
            return UiStep::Done(r);
        }
        if !self.pushed {
            self.pushed = true;
            // Data reaches the overlay only through this channel, never embedded
            // in html() — so it can't inject markup.
            return UiStep::Update(r#"{"message":"Davom etilsinmi?"}"#.to_string());
        }
        UiStep::Pending
    }
}

impl Plugin for ConfirmPlugin {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor {
            id: "Asbt.Confirm".to_string(),
            name: "confirm".to_string(),
            version: "1.0.0".to_string(),
            kind: PluginKind::Ui,
        }
    }
    fn execute(&mut self, _request: &str) -> PluginResult {
        PluginResult::fail("Asbt.Confirm is a UI plugin; use RunPluginUi")
    }
    fn execute_ui(&mut self, _request: &str) -> Option<Box<dyn PluginUi>> {
        Some(Box::new(ConfirmUi { answer: None, pushed: false }))
    }
}

/// Build the registry of built-in plugins the shell ships with.
pub fn builtin_registry() -> PluginRegistry {
    let mut registry = PluginRegistry::new();
    // Distinct ids — registration cannot clash; ignore the Result.
    // system: the reference `Asbt.System` host object. Direct methods (ping,
    // isDebug, GetGUID, Usb*, Get/SetAppSettings) are `raw` — the pv-next `system`
    // service uses the return value with no envelope unwrap; `mbox` is the one
    // deferred/modal method. GetAppSettings backs the workstation settings
    // (RecordCountOnPage, UseReader) pv-next reads.
    let _ = registry.register(Box::new(sentinel_system::SystemPlugin::new()));
    let _ = registry.register(Box::new(ReaderPlugin));
    let _ = registry.register(Box::new(ConfirmPlugin));
    // idbox runs in-process: ReadChipUid is x64 PC/SC (no device SDK); ReadDocument
    // (serial MRZ + x86 chip helper) is a documented stub until that flow lands.
    // pv-next reaches it through its `idbox` JS host object (host_methods), the real
    // pv-next contract — not only `share.RunPlugin`.
    let _ = registry.register(Box::new(sentinel_idbox::IdBoxPlugin::new()));
    // photo: TakePhoto (1.0.0 stub) + TakeDocumentPhoto (1.1.0) via the `photo` host
    // object. TakeDocumentPhoto opens the camera overlay (the UI path); the real
    // Canon EDSDK is linked only in a `device` x64 build, else a stub drives the
    // overlay to the reference's "Canon files missing" box.
    let _ = registry.register(Box::new(sentinel_photo::PhotoPlugin::new()));
    // signpad: Capture() via the `signpad` host object opens the signature overlay
    // (an HTML canvas, since SigPlusNET is managed .NET and no native Topaz SDK is
    // available) and returns a 495x285 base64 JPEG.
    let _ = registry.register(Box::new(sentinel_signpad::SignPadPlugin::new()));
    // fingerprints: Scan (1.0.0 stub) + Enroll (1.1.0) via the `fingerprints` host
    // object. Enroll opens the 10-finger enrollment overlay (the UI path); the real
    // Suprema RealScan device is behind a `device` x64 build, else a stub drives the
    // overlay to the reference's no-scanner state.
    let _ = registry.register(Box::new(sentinel_fingerprints::FingerprintsPlugin::new()));
    // ekalit: EKalitId + SignStringV1/V2 + GetCertificateInfo via the `ekalit` host
    // object (E-IMZO / E-Kalit USB token). The signing methods open a PIN overlay;
    // the token backend needs the vendor native SDK (managed libesigner only here),
    // so a stub reports "E-Kalit уланмаган".
    let _ = registry.register(Box::new(sentinel_ekalit::EKalitPlugin::new()));
    // widget: the reference `Asbt.Widget` data plugin (IBrowserExtension, no host
    // object). Its Oracle-backed per-region protocol counts are disabled in the
    // reference and server-side anyway, so it returns an empty row array `[]`.
    // pv-next does not call it; registered for parity so GetPlugins() lists it.
    let _ = registry.register(Box::new(sentinel_widget::WidgetPlugin::new()));
    registry
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_is_registered_with_its_raw_host_methods_and_mbox_ui() {
        let reg = builtin_registry();
        assert!(reg.contains("Asbt.System"));
        let spec = reg.host_object_specs();
        let system = spec.iter().find(|s| s.name == "system").expect("system host object");
        assert_eq!(system.id, "Asbt.System");
        // Every direct method is raw (the pv-next service uses the return verbatim);
        // mbox is the one deferred/modal (ui) method.
        let methods: Vec<(&str, bool, bool)> =
            system.methods.iter().map(|m| (m.js.as_str(), m.ui, m.raw)).collect();
        assert_eq!(
            methods,
            [
                ("ping", false, true),
                ("isDebug", false, true),
                ("GetGUID", false, true),
                ("mbox", true, false),
                ("UsbEnable", false, true),
                ("UsbDisable", false, true),
                ("GetAppSettings", false, true),
                ("SetAppSettings", false, true),
            ]
        );
    }

    #[test]
    fn system_is_debug_dispatches_to_a_raw_json_true() {
        let mut reg = builtin_registry();
        let r = reg.dispatch("Asbt.System", r#"{"method":"isDebug","args":[]}"#);
        assert!(r.success && r.error_message.is_empty());
        // Raw: data JSON-decodes to a JS boolean true.
        assert_eq!(serde_json::from_str::<serde_json::Value>(&r.data).unwrap(), serde_json::Value::Bool(true));
    }

    #[test]
    fn reader_plugin_streams_status_then_final() {
        let mut reg = builtin_registry();
        let mut stream = reg.dispatch_stream("Asbt.Reader", "go").expect("should stream");
        let mut chunks = Vec::new();
        let final_result = loop {
            match stream.poll() {
                StreamStep::Chunk(c) => chunks.push(c),
                StreamStep::Pending => {}
                StreamStep::Done(r) => break r,
            }
        };
        assert_eq!(chunks.len(), 3);
        assert!(chunks[0].contains("connecting"));
        assert_eq!(final_result, PluginResult::ok(r#"{"read":true}"#));
    }

    #[test]
    fn idbox_is_registered_in_process_with_its_host_methods() {
        let reg = builtin_registry();
        assert!(reg.contains("Asbt.IdBox"));
        // pv-next looks the host object up by name "idbox" and calls ReadChipUid /
        // ReadDocument on it — the spec the bridge builds must carry exactly that.
        let spec = reg.host_object_specs();
        let idbox = spec.iter().find(|s| s.name == "idbox").expect("idbox host object");
        assert_eq!(idbox.id, "Asbt.IdBox");
        let methods: Vec<&str> = idbox.methods.iter().map(|m| m.js.as_str()).collect();
        assert_eq!(methods, ["ReadChipUid", "ReadDocument"]);
    }

    #[test]
    fn idbox_read_chip_uid_dispatches_to_real_pcsc() {
        // The host-object method sends {"method":"ReadChipUid",...}; the real plugin
        // runs PC/SC. No reader here → "" with success (never an error), matching the
        // reference. With a card present it would be 14 hex chars.
        let mut reg = builtin_registry();
        let r = reg.dispatch("Asbt.IdBox", r#"{"method":"ReadChipUid","args":[]}"#);
        assert!(r.success);
        assert!(r.error_message.is_empty());
        assert!(r.data.is_empty() || r.data.len() == 14);
    }

    #[test]
    fn photo_is_registered_with_its_host_methods() {
        let reg = builtin_registry();
        assert!(reg.contains("Asbt.Photo"));
        let spec = reg.host_object_specs();
        let photo = spec.iter().find(|s| s.name == "photo").expect("photo host object");
        assert_eq!(photo.id, "Asbt.Photo");
        // pv-next calls photo.TakePhoto() / .TakeDocumentPhoto(opts); the latter is a
        // UI (overlay) method, the former a plain call.
        let methods: Vec<(&str, bool)> =
            photo.methods.iter().map(|m| (m.js.as_str(), m.ui)).collect();
        assert_eq!(methods, [("TakePhoto", false), ("TakeDocumentPhoto", true)]);
    }

    #[test]
    fn signpad_is_registered_with_its_capture_host_method() {
        let reg = builtin_registry();
        assert!(reg.contains("Asbt.SignPad"));
        let spec = reg.host_object_specs();
        let signpad = spec.iter().find(|s| s.name == "signpad").expect("signpad host object");
        assert_eq!(signpad.id, "Asbt.SignPad");
        let methods: Vec<(&str, bool)> =
            signpad.methods.iter().map(|m| (m.js.as_str(), m.ui)).collect();
        assert_eq!(methods, [("Capture", true)]);
    }

    #[test]
    fn fingerprints_is_registered_with_its_host_methods() {
        let reg = builtin_registry();
        assert!(reg.contains("Asbt.Fingerprints"));
        let spec = reg.host_object_specs();
        let fp = spec.iter().find(|s| s.name == "fingerprints").expect("fingerprints host object");
        assert_eq!(fp.id, "Asbt.Fingerprints");
        let methods: Vec<(&str, bool)> = fp.methods.iter().map(|m| (m.js.as_str(), m.ui)).collect();
        assert_eq!(methods, [("Scan", false), ("Enroll", true)]);
    }

    #[test]
    fn ekalit_is_registered_with_its_host_methods() {
        let reg = builtin_registry();
        assert!(reg.contains("Asbt.eKalit"));
        let spec = reg.host_object_specs();
        let ek = spec.iter().find(|s| s.name == "ekalit").expect("ekalit host object");
        assert_eq!(ek.id, "Asbt.eKalit");
        let methods: Vec<(&str, bool)> = ek.methods.iter().map(|m| (m.js.as_str(), m.ui)).collect();
        assert_eq!(
            methods,
            [("EKalitId", false), ("SignStringV1", true), ("SignStringV2", true), ("GetCertificateInfo", true)]
        );
    }

    #[test]
    fn widget_is_registered_headless_and_returns_empty_rows() {
        let mut reg = builtin_registry();
        assert!(reg.contains("Asbt.Widget"));
        // IBrowserExtension with an empty Init → no host object (RunPlugin-only).
        let spec = reg.host_object_specs();
        assert!(spec.iter().all(|s| s.name != "widget"), "widget exposes no host object");
        // share.RunPlugin("Asbt.Widget", …) → the empty widget-row array.
        let r = reg.dispatch("Asbt.Widget", r#"{"method":"run","args":[]}"#);
        assert!(r.success);
        assert!(r.error_message.is_empty());
        assert_eq!(r.data, "[]");
    }

    #[test]
    fn confirm_plugin_is_a_ui_op_that_resolves_on_ok() {
        let mut reg = builtin_registry();
        let mut ui = reg.dispatch_ui("Asbt.Confirm", "{}").expect("should be a UI op");
        assert!(ui.html().contains("window.ui"));
        // plugin -> overlay: one update is pushed first.
        assert!(matches!(ui.poll(), UiStep::Update(_)));
        // overlay -> plugin: OK resolves it with "confirmed".
        ui.on_message("ok");
        match ui.poll() {
            UiStep::Done(r) => assert_eq!(r, PluginResult::ok("confirmed")),
            _ => panic!("expected completion after ok"),
        }
    }
}
