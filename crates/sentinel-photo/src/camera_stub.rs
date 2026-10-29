//! The camera used when the real Canon EDSDK is not linked (every build without the
//! `device` feature, and any non-Windows host).
//!
//! Its `initialize` reports the Canon files missing, so the overlay shows the
//! reference's "Canon фотокамера файллари топилмади" box + toast and stays open —
//! exactly the no-SDK path of `FormCaptureDocument`. The document flow is still
//! fully exercised: the user closes the window, which cancels the call.

use sentinel_photo_core::camera::{Camera, CameraError, CameraEvent};

/// A camera with no SDK behind it.
pub struct StubCamera;

impl Camera for StubCamera {
    fn initialize(&mut self) -> Result<(), CameraError> {
        Err(CameraError::DllNotFound(
            "EDSDK.dll yo'q — Asbt.Photo `device` feature bilan qurilishi kerak".into(),
        ))
    }
    fn open_session(&mut self) -> Result<bool, CameraError> {
        Ok(false)
    }
    fn close_session(&mut self) -> Result<(), CameraError> {
        Ok(())
    }
    fn prepare_host_capture(&mut self) -> Result<(), CameraError> {
        Ok(())
    }
    fn start_live_view(&mut self) -> Result<(), CameraError> {
        Ok(())
    }
    fn stop_live_view(&mut self) -> Result<(), CameraError> {
        Ok(())
    }
    fn get_live_view_frame(&mut self) -> Result<Option<Vec<u8>>, CameraError> {
        Ok(None)
    }
    fn take_photo(&mut self) -> Result<(), CameraError> {
        Err(CameraError::Other("Фотокамера уланмаган".into()))
    }
    fn release_camera(&mut self) -> Result<(), CameraError> {
        Ok(())
    }
    fn dispose(&mut self) -> Result<(), CameraError> {
        Ok(())
    }
    fn is_session_open(&self) -> bool {
        false
    }
    fn is_live_view_on(&self) -> bool {
        false
    }
    fn poll_event(&mut self) -> Option<CameraEvent> {
        None
    }
}
