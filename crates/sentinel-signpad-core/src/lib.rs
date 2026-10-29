//! `sentinel-signpad-core`: the device-free heart of the `Asbt.SignPad` plugin
//! (`signpad.Capture()`), ported from the reference `Asbt.SignPad.Plugin`
//! (pv-winform `FormSignature`, Topaz SigPlusNET).
//!
//! - [`image`] — a 24bpp image + PNG decode (the canvas / the fixtures).
//! - [`acceptance`] — `FormSignature`'s "ToByteArray().Length < 1500" guard,
//!   re-derived as `png_len + 162 < 1500`.
//! - [`encoder`] — `CImageWorks.ImageToByte`: the signature JPEG (495×285).
//! - [`session`] — `SignPadSession` over the [`session::Tablet`] seam
//!   (`Open`/`Clear`/`Save`/`Close`), the window- and device-independent logic.
//! - [`texts`] — the six user-visible strings, byte-for-byte.
//!
//! The reference's capture device was a Topaz pad reached through the **managed**
//! `SigPlusNET.dll`; the shipped Rust browser forbids .NET, and there is no native
//! Topaz SDK here, so the plugin captures the signature on an HTML `<canvas>` in its
//! overlay instead (same 495×285 JPEG contract). This crate is the shared logic;
//! the overlay + plugin are in `sentinel-signpad`.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod acceptance;
pub mod base64;
pub mod encoder;
pub mod image;
pub mod session;
pub mod texts;

pub use acceptance::{is_too_small, png_len, serialized_length};
pub use encoder::to_jpeg;
pub use image::{decode_png, Image};
pub use session::{SaveOutcome, SignPadSession, Tablet};
