//! The Canon camera seam (`ICanonCamera`) and its asynchronous events.
//!
//! [`DocumentPhotoSession`](crate::session::DocumentPhotoSession) drives a camera
//! through this trait step by step, so each reference error message ties to the
//! step that failed. The real EDSDK implementation lives in the plugin crate
//! (behind its `device` feature); tests and the stub use a fake.
//!
//! The reference's camera raised three events from inside EDSDK callbacks, which
//! `FormCaptureDocument` marshalled onto the UI thread with `Control.BeginInvoke`.
//! Rust has no such pump here, so the camera *queues* its events and the session
//! drains them in [`DocumentPhotoSession::pump`](crate::session::DocumentPhotoSession::pump)
//! — the exact point the C# `view.Post` action would have run.

/// An asynchronous camera event (an EDSDK callback), delivered via
/// [`Camera::poll_event`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CameraEvent {
    /// A captured photo was downloaded (the camera file bytes, normally JPEG).
    PhotoDownloaded(Vec<u8>),
    /// The download of a captured photo failed (the failure message).
    DownloadFailed(String),
    /// The camera reported `StateEvent_Shutdown` (switched off / unplugged).
    Shutdown,
}

/// A camera step that failed. `DllNotFound` is the missing-`EDSDK.dll` case the
/// reference's ctor catches specially (→ "Canon фотокамера файллари топилмади");
/// everything else is `Other`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CameraError {
    /// `EDSDK.dll` (or a dependency) is missing — `DllNotFoundException`.
    DllNotFound(String),
    /// Any other SDK/step failure.
    Other(String),
}

impl CameraError {
    /// The failure message (the body of `GetAllMessages`).
    pub fn message(&self) -> &str {
        match self {
            CameraError::DllNotFound(m) | CameraError::Other(m) => m,
        }
    }

    /// Whether this is the missing-DLL case.
    pub fn is_dll_not_found(&self) -> bool {
        matches!(self, CameraError::DllNotFound(_))
    }
}

impl std::fmt::Display for CameraError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.message())
    }
}

impl std::error::Error for CameraError {}

/// The Canon camera as `FormCaptureDocument` drives it. Every method runs on the UI
/// thread (EDSDK is not thread safe). Must not panic; surface problems as
/// [`CameraError`].
pub trait Camera {
    /// `new CanonAPI()` → `EdsInitializeSDK`. A missing `EDSDK.dll` is
    /// [`CameraError::DllNotFound`].
    fn initialize(&mut self) -> Result<(), CameraError>;

    /// Open a session on the FIRST camera. `Ok(false)` when none is connected;
    /// `Err` on any other failure (also when not initialized).
    fn open_session(&mut self) -> Result<bool, CameraError>;

    /// Close the session; the camera object stays for a later [`Camera::open_session`].
    fn close_session(&mut self) -> Result<(), CameraError>;

    /// `SetSetting(SaveTo, Host)` + `SetCapacity`; `Err` without a session.
    fn prepare_host_capture(&mut self) -> Result<(), CameraError>;

    /// Start the live view.
    fn start_live_view(&mut self) -> Result<(), CameraError>;

    /// Stop the live view.
    fn stop_live_view(&mut self) -> Result<(), CameraError>;

    /// One live-view JPEG frame, or `None` when the camera has none ready.
    fn get_live_view_frame(&mut self) -> Result<Option<Vec<u8>>, CameraError>;

    /// Fire the shutter; the file arrives later via a [`CameraEvent::PhotoDownloaded`].
    /// `Err` without a session.
    fn take_photo(&mut self) -> Result<(), CameraError>;

    /// `MainCamera.Dispose()`: close the session and release the camera, SDK stays
    /// initialized.
    fn release_camera(&mut self) -> Result<(), CameraError>;

    /// Full teardown (session, release, `EdsTerminateSDK`).
    fn dispose(&mut self) -> Result<(), CameraError>;

    /// Whether a session is open.
    fn is_session_open(&self) -> bool;

    /// Whether the live view is on.
    fn is_live_view_on(&self) -> bool;

    /// The next queued asynchronous event, or `None`. Drained by
    /// [`DocumentPhotoSession::pump`](crate::session::DocumentPhotoSession::pump).
    fn poll_event(&mut self) -> Option<CameraEvent>;
}
