//! What [`DocumentPhotoSession`](crate::session::DocumentPhotoSession) does to the
//! window (`IDocumentPhotoView`). The overlay (the plugin crate) is the real
//! implementation; tests use a recording fake.
//!
//! The reference's `IDocumentPhotoView.Post` (marshal a camera event onto the UI
//! thread) is not here: the session drains camera events itself in `pump`, so there
//! is nothing to post.

use crate::pipeline::Rgb;

/// The window operations the session invokes. Every call is on the UI thread.
pub trait DocumentView {
    /// `AlertMessage.ShowMessage(..., Error)`: a modal `XtraMessageBox(text, "Ҳато")`.
    fn show_error_message(&mut self, text: &str);

    /// `AlertMessage.ShowAlert`: a 2 s toast at the bottom right. `text` is the
    /// detail body (may be empty).
    fn show_toast(&mut self, caption: &str, text: &str);

    /// `sbCapture.Visible`.
    fn set_capture_visible(&mut self, visible: bool);

    /// `sbCapture.Enabled`.
    fn set_capture_enabled(&mut self, enabled: bool);

    /// `sbSave.Enabled`.
    fn set_save_enabled(&mut self, enabled: bool);

    /// `pCameraCaptured.Image` (`None` shows the NullText). The view copies what it
    /// needs to display.
    fn show_captured_image(&mut self, image: Option<&Rgb>);

    /// `DialogResult = OK; Close()`.
    fn close_ok(&mut self);
}
