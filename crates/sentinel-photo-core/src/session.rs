//! `DocumentPhotoSession`: the logic of pv-winform `FormCaptureDocument`, ported
//! 1:1 apart from the deviations in `docs/photo-document-plugin.md`.
//!
//! It owns a [`Camera`] and a [`DocumentView`] and runs on the UI thread. The
//! direct user actions ([`Self::start`], [`Self::capture`], [`Self::save`],
//! [`Self::init_camera`], [`Self::replace_pane`], [`Self::live_view_failed`],
//! [`Self::close`]) are synchronous; the three camera-origin events are queued by
//! the camera and processed in [`Self::pump`] — the point C#'s `view.Post` action
//! ran.

use crate::base64;
use crate::camera::{Camera, CameraError, CameraEvent};
use crate::errortext::get_all_messages;
use crate::pipeline::{self, Rgb};
use crate::texts;
use crate::view::DocumentView;

/// What `pCameraLive` paints (`FormCaptureDocument.pCameraEnroll_Paint`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveViewState {
    /// No camera or no open session: the NullText shows.
    NoSession,
    /// Session open, live view off: cleared to the background.
    Off,
    /// Session open, live view on: the last frame plus the green guides.
    On,
}

/// The reference `FormCaptureDocument` state machine.
pub struct DocumentPhotoSession {
    view: Box<dyn DocumentView>,
    camera: Box<dyn Camera>,
    scale_percent: i32,
    log: Box<dyn FnMut(&str)>,

    /// The caller's base64 (only kept when it decoded), returned unchanged by Save
    /// when the pane still holds it.
    current_photo_base64: Option<String>,
    current_photo: Option<Rgb>,

    pane: Option<Rgb>,
    /// Whether `pane` is still the caller's `current_photo` (C#'s `ReferenceEquals`).
    pane_is_current: bool,
    is_capturing: bool,
    closed: bool,
    result_base64: Option<String>,
}

impl DocumentPhotoSession {
    /// Build the session. `current_photo_base64` is the caller's `currentPhoto`
    /// (already validated base64); it is decoded here and, only if it decodes to an
    /// image, shown in the pane and eligible to be returned unchanged by Save.
    pub fn new(
        view: Box<dyn DocumentView>,
        camera: Box<dyn Camera>,
        scale_percent: i32,
        current_photo_base64: Option<String>,
        log: Box<dyn FnMut(&str)>,
    ) -> Self {
        let mut current_photo = None;
        let mut kept_base64 = None;
        if let Some(b64) = &current_photo_base64 {
            if let Some(bytes) = base64::decode_dotnet(b64) {
                if let Some(rgb) = pipeline::decode_jpeg(&bytes) {
                    current_photo = Some(rgb);
                    kept_base64 = Some(b64.clone());
                }
            }
        }
        DocumentPhotoSession {
            view,
            camera,
            scale_percent,
            log,
            current_photo_base64: kept_base64,
            current_photo,
            pane: None,
            pane_is_current: false,
            is_capturing: false,
            closed: false,
            result_base64: None,
        }
    }

    /// base64 of the saved photo (set by [`Self::save`]); `None` while cancelled.
    pub fn result_base64(&self) -> Option<&str> {
        self.result_base64.as_deref()
    }

    /// C#'s `isCapturing` (for tests / the overlay's busy state).
    pub fn is_capturing(&self) -> bool {
        self.is_capturing
    }

    /// The live-view state (for painting).
    pub fn live_view_state(&self) -> LiveViewState {
        if !self.camera.is_session_open() {
            LiveViewState::NoSession
        } else if self.camera.is_live_view_on() {
            LiveViewState::On
        } else {
            LiveViewState::Off
        }
    }

    /// The ctor body of `FormCaptureDocument` (:30-53): show the caller's photo,
    /// `Initialize`, then `InitCamera`, all before the window shows so the boxes
    /// appear first.
    pub fn start(&mut self) {
        if self.current_photo.is_some() {
            self.set_pane(self.current_photo.clone(), true);
        }
        match self.camera.initialize() {
            Ok(()) => self.init_camera(),
            Err(e) if e.is_dll_not_found() => {
                (self.log)("Asbt.Photo: EDSDK not found");
                self.message(texts::CANON_FILES_NOT_FOUND, "");
            }
            Err(e) => {
                (self.log)("Asbt.Photo: EDSDK initialization failed");
                self.message(texts::CAMERA_FILES_START_ERROR, &get_all_messages(&[e.message()]));
            }
        }
    }

    /// `InitCamera` (:55-84), also the live-view reload button. The C# quirk is
    /// kept: a second call with the session open closes it and then fails
    /// `PrepareHostCapture` → "Камера ишга туширишда ҳато".
    pub fn init_camera(&mut self) {
        match self.init_camera_inner() {
            Ok(()) => {}
            Err(e) => {
                (self.log)("Asbt.Photo: InitCamera failed");
                self.message(texts::CAMERA_START_ERROR, &get_all_messages(&[e.message()]));
            }
        }
    }

    fn init_camera_inner(&mut self) -> Result<(), CameraError> {
        if self.camera.is_session_open() {
            self.camera.close_session()?;
        } else if self.open_session()? {
            self.view.set_capture_visible(true);
        } else {
            self.view.set_capture_visible(false);
            return Ok(());
        }

        self.camera.prepare_host_capture()?;

        if !self.camera.is_live_view_on() {
            self.camera.start_live_view()?;
        } else {
            self.camera.stop_live_view()?;
        }
        Ok(())
    }

    /// `OpenSession` (:134-151): the empty-list case shows "Фотокамера топилмади".
    fn open_session(&mut self) -> Result<bool, CameraError> {
        if self.camera.open_session()? {
            Ok(true)
        } else {
            self.message(texts::CAMERA_NOT_FOUND, "");
            Ok(false)
        }
    }

    /// `pCameraEnroll_Click` (:388-415): sbCapture and a click on the live view.
    /// Deviation (decision 5): the catch resets `isCapturing` (C# left it set, so a
    /// later click did nothing after one failed shot).
    pub fn capture(&mut self) {
        if self.is_capturing {
            return;
        }
        self.view.set_capture_enabled(false);
        self.is_capturing = true;
        if let Err(e) = self.camera.take_photo() {
            (self.log)("Asbt.Photo: TakePhoto failed");
            self.message(texts::CAPTURE_ERROR, &get_all_messages(&[e.message()]));
            self.re_initiate_camera();
            self.view.set_capture_enabled(true);
            let has_pane = self.pane.is_some();
            self.view.set_save_enabled(has_pane);
            self.is_capturing = false;
        }
    }

    /// `sbSave_Click` (:356-367), also Enter (which bypasses the disabled sbSave):
    /// with the caller's photo still in the pane, returns it unchanged.
    pub fn save(&mut self) {
        if self.pane.is_some() {
            self.result_base64 = if self.pane_is_current {
                self.current_photo_base64.clone()
            } else {
                match pipeline::to_jpeg(self.pane.as_ref().unwrap()) {
                    Ok(bytes) => Some(base64::encode(&bytes)),
                    Err(e) => {
                        // Unreachable in practice (the pane is our own in-range RGB);
                        // if GDI+'s equivalent ever failed C# threw out of the call.
                        // Keep the window open rather than close with a bad result.
                        (self.log)(&format!("Asbt.Photo: encode on save failed: {e}"));
                        return;
                    }
                }
            };
            self.view.close_ok();
        } else {
            self.message(texts::NO_PHOTO, "");
        }
    }

    /// Replace the pane from the captured pane's context menu (Cut/Delete → `None`,
    /// Paste/Load → an image); sbSave's state is untouched.
    pub fn replace_pane(&mut self, image: Option<Rgb>) {
        self.set_pane(image, false);
    }

    /// One live-view frame (JPEG) or `None`. Propagates the camera error so the
    /// overlay can route it to [`Self::live_view_failed`].
    pub fn next_live_view_frame(&mut self) -> Result<Option<Vec<u8>>, CameraError> {
        self.camera.get_live_view_frame()
    }

    /// `MainCamera_LiveViewUpdated`'s catch: box only (the overlay stops its frame
    /// timer and shows this once).
    pub fn live_view_failed(&mut self, chain: &[&str]) {
        (self.log)("Asbt.Photo: live view failed");
        self.message(texts::LIVE_VIEW_ERROR, &get_all_messages(chain));
    }

    /// `FormCaptureFace_FormClosing` (:370-386). Idempotent.
    pub fn close(&mut self) {
        if self.closed {
            return;
        }
        self.closed = true;
        if let Err(e) = self.camera.dispose() {
            (self.log)("Asbt.Photo: camera dispose failed");
            self.message(texts::CAMERA_CLOSE_ERROR, &get_all_messages(&[e.message()]));
        }
        self.pane = None;
        self.current_photo = None;
    }

    /// Drain and process the camera's queued asynchronous events — the UI-thread
    /// point C#'s `view.Post` callbacks ran. Called each frame by the overlay (and
    /// by tests after raising a camera event).
    pub fn pump(&mut self) {
        while let Some(event) = self.camera.poll_event() {
            match event {
                CameraEvent::PhotoDownloaded(file) => self.on_photo_downloaded(file),
                CameraEvent::DownloadFailed(message) => self.on_download_failed(&message),
                CameraEvent::Shutdown => self.on_camera_shutdown(),
            }
        }
    }

    /// `MainCamera_DownloadReady` (:264-302).
    fn on_photo_downloaded(&mut self, file: Vec<u8>) {
        if self.closed {
            return;
        }
        match pipeline::capture(&file, self.scale_percent) {
            Ok(scaled) => {
                self.set_pane(Some(scaled), false);
                self.view.set_capture_enabled(true);
                self.view.set_save_enabled(true);
            }
            Err(e) => {
                // C# leaves sbCapture disabled here (only success re-enables it);
                // the live-view click still captures (isCapturing is reset below).
                (self.log)("Asbt.Photo: photo handling failed");
                self.view.show_toast(texts::DOWNLOAD_ERROR, &get_all_messages(&[&e.to_string()]));
            }
        }
        self.is_capturing = false;
    }

    /// The camera file could not be downloaded (C#'s `DownloadFile` threw inside
    /// `DownloadReady`).
    fn on_download_failed(&mut self, message: &str) {
        if self.closed {
            return;
        }
        (self.log)("Asbt.Photo: photo download failed");
        self.view.show_toast(texts::DOWNLOAD_ERROR, &format!("{message}\r\n"));
        self.is_capturing = false;
    }

    /// `MainCamera_StateChanged` (:88-104), `StateEventID.Shutdown`.
    fn on_camera_shutdown(&mut self) {
        if self.closed {
            return;
        }
        if let Err(e) = self.camera.close_session() {
            (self.log)("Asbt.Photo: CloseSession on shutdown failed");
            self.message(texts::CAMERA_STATE_ERROR, &get_all_messages(&[e.message()]));
        }
    }

    /// `ReInitiateCamera` (:421-430): `MainCamera.Dispose()` (release), never
    /// re-opened here (the reload button can). A release error is only logged.
    fn re_initiate_camera(&mut self) {
        if let Err(e) = self.camera.release_camera() {
            (self.log)(&format!("Asbt.Photo: camera release failed: {}", e.message()));
        }
    }

    fn set_pane(&mut self, image: Option<Rgb>, is_current: bool) {
        self.view.show_captured_image(image.as_ref());
        self.pane = image;
        self.pane_is_current = is_current;
    }

    /// `AlertMessage.ShowMessage(..., Error)`: the box shows `text` only; a toast
    /// with `text` follows ONLY when there is no `detail`.
    fn message(&mut self, text: &str, detail: &str) {
        self.view.show_error_message(text);
        if detail.is_empty() {
            self.view.show_toast(text, "");
        }
    }
}
