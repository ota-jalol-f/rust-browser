//! The window seam (`IEnrollView`): what [`EnrollmentSession`](crate::session::EnrollmentSession)
//! asks the enrollment window to show. The overlay is the real implementation;
//! tests use a recorder.

/// `Asbt.CommonControlsV2 AlertType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlertKind {
    /// "Маълумот".
    Info,
    /// "Диққат".
    Warning,
    /// "Ҳато".
    Error,
    /// "Савол".
    Question,
}

/// What `EnrollmentSession` needs from the window.
///
/// `start_capture` records that a capture was requested; the driver (the overlay's
/// poll loop, or a test) then runs
/// [`EnrollmentSession::do_auto_capture`](crate::session::EnrollmentSession::do_auto_capture)
/// off the UI thread and feeds the outcome to
/// [`EnrollmentSession::capture_process`](crate::session::EnrollmentSession::capture_process).
pub trait EnrollView {
    /// `AlertMessage.ShowMessage`: a modal box, then the corner toast.
    fn show_message(&mut self, message: &str, kind: AlertKind);
    /// `AlertMessage.ShowAlert`: the bottom-right toast only.
    fn show_alert(&mut self, caption: &str, text: &str, kind: AlertKind);
    /// `AlertMessage.ShowQuestion`: toast, then a Yes/No box; `true` on Yes.
    fn show_question(&mut self, message: &str) -> bool;
    /// `WSQProcesses`: a plain `MessageBox.Show(text)` (no caption).
    fn show_plain_message_box(&mut self, text: &str);
    /// Record that a capture worker should run (C#'s `StartCapture`).
    fn start_capture(&mut self);
    /// The native preview window the SDK draws into (`PreviewWindow.Handle`).
    fn preview_handle(&self) -> usize;
    /// `GetClientRect(PreviewWindow)`: (right, bottom).
    fn preview_client_size(&self) -> (i32, i32);
}
