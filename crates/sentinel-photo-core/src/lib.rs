//! `sentinel-photo-core`: the device-free heart of the `Asbt.Photo` plugin's
//! document-photo path (`photo.TakeDocumentPhoto`), ported from the reference
//! `Asbt.Photo.Plugin` 1.1.0 (pv-winform `FormCaptureDocument`).
//!
//! What lives here is everything that does NOT drive the Canon EDSDK:
//! - [`options`] — `DocumentPhotoOptions`: parse + validate the options JSON with
//!   the reference's exact rules and messages.
//! - [`dispatch`] — classify a plugin request into [`dispatch::PhotoCall`],
//!   handling both the host-object shape (`{"method":"TakeDocumentPhoto",
//!   "args":["<optionsJson>"]}`) and the `share.RunPlugin` inline shape
//!   (`{"method":"takeDocumentPhoto", ...options}`).
//! - [`pipeline`] — the GDI+ image path: [`pipeline::crop_rect`] /
//!   [`pipeline::scaled_size`] (exact integer/`f32` arithmetic) and the
//!   decode → crop → scale → JPEG(96 dpi) capture (`pipeline::capture_to_jpeg`).
//! - [`texts`] — every user-visible string, byte-for-byte (homoglyphs kept).
//! - [`errortext`] — `CError.GetAllMessages`, the box-detail / toast body.
//! - [`base64`] — standard base64 encode + a .NET-`Convert`-shaped validator.
//!
//! The Canon camera, the live-view/capture/save state machine and the overlay
//! window are built on top of this in `sentinel-photo` (the plugin crate).

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod base64;
pub mod dispatch;
pub mod errortext;
pub mod options;
pub mod pipeline;
pub mod texts;

pub use dispatch::{classify, PhotoCall};
pub use options::{DocumentPhotoOptions, OptionsError};
pub use pipeline::{
    capture, capture_to_jpeg, crop_rect, scaled_size, to_jpeg, PipelineError, Rect, Rgb,
};
