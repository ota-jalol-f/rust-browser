//! `sentinel-signpad`: the `Asbt.SignPad` plugin (x64) — the in-process Rust
//! equivalent of the reference `Asbt.SignPad.Plugin`'s `signpad` host object.
//!
//! pv-next calls `chrome.webview.hostObjects.signpad.Capture()`, which opens the
//! signature overlay ([`overlay::SignatureUi`]) and resolves with the 495×285 base64
//! JPEG, or cancel / busy / error — the `PluginResult` envelope the reference
//! `SignPadObj.Capture` returned.
//!
//! The reference captured on a Topaz pad through the managed `SigPlusNET.dll`; this
//! build has no .NET and no native Topaz SDK, so the overlay captures on an HTML
//! `<canvas>` (device-free logic + encoding live in `sentinel-signpad-core`).

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod overlay;

use std::cell::Cell;
use std::rc::Rc;

use sentinel_plugin::{HostMethod, Plugin, PluginDescriptor, PluginKind, PluginResult, PluginUi};
use sentinel_signpad_core::texts;

use crate::overlay::{BusyGuard, SignatureUi};

/// The SignPad plugin.
pub struct SignPadPlugin {
    /// Set while a signature overlay is open; a second `Capture` is refused as busy.
    /// Shared with the overlay's [`BusyGuard`], cleared on the overlay's drop.
    busy: Rc<Cell<bool>>,
}

impl SignPadPlugin {
    /// Create the SignPad plugin.
    pub fn new() -> Self {
        SignPadPlugin { busy: Rc::new(Cell::new(false)) }
    }
}

impl Default for SignPadPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl Plugin for SignPadPlugin {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor {
            id: "Asbt.SignPad".to_string(),
            name: "signpad".to_string(),
            version: "1.0.0".to_string(),
            kind: PluginKind::Ui,
        }
    }

    fn host_methods(&self) -> Vec<HostMethod> {
        // pv-next calls hostObjects.signpad.Capture(); it opens the overlay (UI path).
        vec![HostMethod::ui("Capture")]
    }

    fn execute(&mut self, _request: &str) -> PluginResult {
        // Reached only when execute_ui returned None (busy), or via the non-UI
        // RunPlugin path. The reference's ExecuteAsync ignored the request and
        // returned Capture(); a busy overlay yields the busy message.
        if self.busy.get() {
            PluginResult::fail(texts::BUSY)
        } else {
            PluginResult::fail("Capture UI oynasi orqali chaqirilishi kerak (hostObjects.signpad.Capture)")
        }
    }

    fn execute_ui(&mut self, _request: &str) -> Option<Box<dyn PluginUi>> {
        // Capture ignores the request (as ExecuteAsync did). Busy → None, so
        // dispatch_ui falls back to execute() → the busy message.
        if self.busy.get() {
            return None;
        }
        let guard = BusyGuard::acquire(&self.busy);
        Some(Box::new(SignatureUi::new(guard)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sentinel_plugin::UiStep;

    #[test]
    fn descriptor_is_signpad_1_0_0_ui() {
        let d = SignPadPlugin::new().descriptor();
        assert_eq!(d.id, "Asbt.SignPad");
        assert_eq!(d.name, "signpad");
        assert_eq!(d.version, "1.0.0");
        assert_eq!(d.kind, PluginKind::Ui);
    }

    #[test]
    fn host_method_is_capture_ui() {
        let m = SignPadPlugin::new().host_methods();
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].js, "Capture");
        assert!(m[0].ui);
    }

    #[test]
    fn capture_opens_overlay_and_is_busy_until_dropped() {
        let mut p = SignPadPlugin::new();
        let ui = p.execute_ui("{}").expect("overlay opens");
        assert!(ui.html().contains("window.ui"));
        // Busy: a second Capture is refused, and execute reports the busy message.
        assert!(p.execute_ui("{}").is_none());
        assert_eq!(p.execute("{}").error_message, texts::BUSY);
        drop(ui);
        assert!(p.execute_ui("{}").is_some());
    }

    #[test]
    fn cancel_resolves_as_cancel() {
        let mut p = SignPadPlugin::new();
        let mut ui = p.execute_ui("{}").expect("overlay");
        ui.on_message("cancel");
        match ui.poll() {
            UiStep::Done(r) => {
                assert!(!r.success);
                assert!(r.error_message.is_empty());
            }
            _ => panic!("expected cancel"),
        }
    }

    #[test]
    fn blank_save_warns_and_stays_open_then_signed_save_returns_jpeg() {
        let mut p = SignPadPlugin::new();
        let mut ui = p.execute_ui("{}").expect("overlay");

        // A blank 495x285 white PNG → too small → warning, window stays open.
        let blank = blank_png();
        ui.on_message(&format!("save:{}", sentinel_signpad_core::base64::encode(&blank)));
        let mut warned = false;
        for _ in 0..3 {
            if let UiStep::Update(j) = ui.poll() {
                if j.contains(texts::TOO_SMALL_MESSAGE) {
                    warned = true;
                    break;
                }
            }
        }
        assert!(warned, "expected the too-small warning");

        // A heavily-inked PNG → accepted → Done(ok) with a base64 JPEG.
        let inked = inked_png();
        ui.on_message(&format!("save:{}", sentinel_signpad_core::base64::encode(&inked)));
        match ui.poll() {
            UiStep::Done(r) => {
                assert!(r.success, "signed save should succeed: {}", r.error_message);
                assert!(!r.data.is_empty());
                let jpeg = sentinel_signpad_core::base64::decode(&r.data).unwrap();
                assert_eq!(&jpeg[..3], &[0xFF, 0xD8, 0xFF]); // JPEG SOI
            }
            _ => panic!("expected accepted completion"),
        }
    }

    fn encode_png(width: u32, height: u32, rgb: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut out, width, height);
            enc.set_color(png::ColorType::Rgb);
            enc.set_depth(png::BitDepth::Eight);
            enc.write_header().unwrap().write_image_data(rgb).unwrap();
        }
        out
    }

    fn blank_png() -> Vec<u8> {
        encode_png(495, 285, &vec![0xFF; 495 * 285 * 3])
    }

    fn inked_png() -> Vec<u8> {
        // A noisy scatter (poorly compressible, like a real inked signature) so the
        // PNG is well above the too-small threshold. A solid block would compress to
        // almost nothing and be rejected — the rule keys on PNG size, not ink area.
        let mut rgb = vec![0xFFu8; 495 * 285 * 3];
        for y in 0..285usize {
            for x in 0..495usize {
                if (x * 31 + y * 17 + x * y) % 3 == 0 {
                    let i = (y * 495 + x) * 3;
                    rgb[i] = 0;
                    rgb[i + 1] = 0;
                    rgb[i + 2] = 0;
                }
            }
        }
        encode_png(495, 285, &rgb)
    }
}
