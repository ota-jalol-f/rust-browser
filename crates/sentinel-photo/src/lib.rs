//! `sentinel-photo`: the `Asbt.Photo` plugin (x64) — the in-process Rust
//! equivalent of the reference `Asbt.Photo.Plugin`'s `photo` host object.
//!
//! pv-next drives it through `chrome.webview.hostObjects.photo`:
//! - `TakeDocumentPhoto(optionsJson)` (1.1.0) → the document-photo camera overlay
//!   ([`overlay::DocumentPhotoUi`]); routed through the shell's UI path because the
//!   camera needs a live view.
//! - `TakePhoto()` (1.0.0 full-frame PNG) → a documented stub (not ported).
//!
//! The device-free logic is in `sentinel-photo-core`; the Canon EDSDK camera is in
//! [`edsdk`] behind the `device` feature (x64 Windows), with a [`camera_stub`]
//! fallback everywhere else.

// `deny` (not `forbid`) so the EDSDK FFI module can opt in with `allow`; all other
// modules stay unsafe-free.
#![deny(unsafe_code)]
#![warn(missing_docs)]

pub mod camera_stub;
pub mod overlay;

#[cfg(all(feature = "device", windows, target_arch = "x86_64"))]
#[allow(unsafe_code)] // the EDSDK FFI; isolated to this module.
pub mod edsdk;

use std::cell::Cell;
use std::rc::Rc;

use sentinel_photo_core::camera::Camera;
use sentinel_photo_core::{classify, texts, DocumentPhotoOptions, PhotoCall};
use sentinel_plugin::{HostMethod, Plugin, PluginDescriptor, PluginKind, PluginResult, PluginUi};

use crate::camera_stub::StubCamera;
use crate::overlay::{BusyGuard, DocumentPhotoUi};

/// The Photo plugin.
pub struct PhotoPlugin {
    /// Set while a document overlay is open; a second call is refused as busy. An
    /// `Rc<Cell>` shared with the overlay's [`BusyGuard`], cleared on the overlay's
    /// drop (so Esc / a tab navigation never locks Photo).
    busy: Rc<Cell<bool>>,
}

impl PhotoPlugin {
    /// Create the Photo plugin.
    pub fn new() -> Self {
        PhotoPlugin { busy: Rc::new(Cell::new(false)) }
    }
}

impl Default for PhotoPlugin {
    fn default() -> Self {
        Self::new()
    }
}

/// The 1.0.0 `TakePhoto()` full-frame path (`PhotoWindow`) is not ported.
fn take_photo_stub() -> PluginResult {
    PluginResult::fail("TakePhoto (1.0.0) hali ko'chirilmagan; TakeDocumentPhoto'dan foydalaning")
}

/// The camera for one overlay: the real EDSDK on x64 Windows `device` builds, else
/// the stub (which reports the Canon files missing).
fn make_camera() -> Box<dyn Camera> {
    #[cfg(all(feature = "device", windows, target_arch = "x86_64"))]
    {
        match edsdk::RealCanonCamera::load() {
            Ok(camera) => Box::new(camera),
            // No EDSDK.dll next to the exe → fall back to the stub, which drives the
            // overlay to the reference's "Canon files missing" box.
            Err(_) => Box::new(StubCamera),
        }
    }
    #[cfg(not(all(feature = "device", windows, target_arch = "x86_64")))]
    {
        Box::new(StubCamera)
    }
}

impl Plugin for PhotoPlugin {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor {
            id: "Asbt.Photo".to_string(),
            name: "photo".to_string(),
            // 1.1.0: the Vue side gates TakeDocumentPhoto on photo >= 1.1.0.
            version: "1.1.0".to_string(),
            kind: PluginKind::Ui,
        }
    }

    fn host_methods(&self) -> Vec<HostMethod> {
        // pv-next calls hostObjects.photo.TakePhoto() / .TakeDocumentPhoto(opts).
        // TakeDocumentPhoto opens the camera overlay (UI path); TakePhoto is a plain
        // call (the 1.0.0 stub).
        vec![HostMethod::run("TakePhoto"), HostMethod::ui("TakeDocumentPhoto")]
    }

    fn execute(&mut self, request: &str) -> PluginResult {
        match classify(request) {
            PhotoCall::TakePhoto => take_photo_stub(),
            PhotoCall::TakeDocumentPhoto(options_json) => {
                // Reached only when execute_ui returned None: busy, or bad options,
                // or a valid request on the non-UI RunPlugin path.
                if self.busy.get() {
                    return PluginResult::fail(texts::DOCUMENT_BUSY);
                }
                match DocumentPhotoOptions::parse(&options_json) {
                    Err(e) => PluginResult::fail(e.message()),
                    Ok(_) => PluginResult::fail(
                        "TakeDocumentPhoto UI oynasi orqali chaqirilishi kerak (hostObjects.photo.TakeDocumentPhoto)",
                    ),
                }
            }
        }
    }

    fn execute_ui(&mut self, request: &str) -> Option<Box<dyn PluginUi>> {
        match classify(request) {
            PhotoCall::TakeDocumentPhoto(options_json) => {
                // Busy or invalid options → None, so dispatch_ui falls back to
                // execute(), which returns the busy / validation failure.
                if self.busy.get() {
                    return None;
                }
                let options = DocumentPhotoOptions::parse(&options_json).ok()?;
                let guard = BusyGuard::acquire(&self.busy);
                Some(Box::new(DocumentPhotoUi::new(
                    make_camera(),
                    options.scale_percent,
                    options.current_photo,
                    guard,
                )))
            }
            // TakePhoto has no overlay → fall back to execute (the stub).
            PhotoCall::TakePhoto => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sentinel_plugin::UiStep;

    #[test]
    fn descriptor_is_photo_1_1_0_ui() {
        let d = PhotoPlugin::new().descriptor();
        assert_eq!(d.id, "Asbt.Photo");
        assert_eq!(d.name, "photo");
        assert_eq!(d.version, "1.1.0");
        assert_eq!(d.kind, PluginKind::Ui);
    }

    #[test]
    fn host_methods_match_pv_next() {
        let names: Vec<String> = PhotoPlugin::new().host_methods().into_iter().map(|m| m.js).collect();
        assert_eq!(names, ["TakePhoto", "TakeDocumentPhoto"]);
        // TakeDocumentPhoto routes through the UI path, TakePhoto does not.
        let ui: Vec<bool> = PhotoPlugin::new().host_methods().into_iter().map(|m| m.ui).collect();
        assert_eq!(ui, [false, true]);
    }

    #[test]
    fn take_photo_is_a_stub() {
        let mut p = PhotoPlugin::new();
        let r = p.execute(r#"{"method":"TakePhoto","args":[]}"#);
        assert!(!r.success);
        assert!(r.error_message.contains("TakePhoto"));
    }

    #[test]
    fn bad_options_fail_with_the_reference_message_via_execute() {
        let mut p = PhotoPlugin::new();
        // execute_ui returns None for bad options; dispatch_ui would fall back to execute.
        assert!(p.execute_ui(r#"{"method":"TakeDocumentPhoto","args":["{\"scalePercent\":0}"]}"#).is_none());
        let r = p.execute(r#"{"method":"TakeDocumentPhoto","args":["{\"scalePercent\":0}"]}"#);
        assert_eq!(r.error_message, "TakeDocumentPhoto: scalePercent must be an integer 1..100, got 0");
    }

    #[test]
    fn valid_request_opens_an_overlay_and_marks_busy_until_dropped() {
        let mut p = PhotoPlugin::new();
        let req = r#"{"method":"TakeDocumentPhoto","args":["{\"scalePercent\":30}"]}"#;
        let ui = p.execute_ui(req).expect("overlay opens");
        assert!(ui.html().contains("window.ui"));
        // Busy now: a second call is refused, and execute reports the busy message.
        assert!(p.execute_ui(req).is_none());
        assert_eq!(p.execute(req).error_message, texts::DOCUMENT_BUSY);
        // Dropping the overlay clears busy.
        drop(ui);
        assert!(p.execute_ui(req).is_some());
    }

    #[test]
    fn overlay_with_no_sdk_shows_canon_files_box_then_cancels() {
        let mut p = PhotoPlugin::new();
        let mut ui = p
            .execute_ui(r#"{"method":"TakeDocumentPhoto","args":["{\"scalePercent\":20}"]}"#)
            .expect("overlay");
        // The stub camera's initialize reports the Canon files missing → that box is
        // queued by Start and pushed on the first poll.
        let mut saw_box = false;
        for _ in 0..3 {
            if let UiStep::Update(j) = ui.poll() {
                if j.contains(texts::CANON_FILES_NOT_FOUND) {
                    saw_box = true;
                    break;
                }
            }
        }
        assert!(saw_box, "expected the Canon-files-missing box");
        // Cancel → the call resolves as a cancel (success=false, empty message).
        ui.on_message("cancel");
        match ui.poll() {
            UiStep::Done(r) => {
                assert!(!r.success);
                assert!(r.error_message.is_empty());
            }
            _ => panic!("expected cancel completion"),
        }
    }
}
