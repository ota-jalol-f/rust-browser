//! `SignPadSession`: `FormSignature`'s Load / Clear / Save / FormClosing logic,
//! independent of the window and the device, over the [`Tablet`] seam.

use crate::acceptance::{is_too_small, serialized_length};
use crate::encoder::to_jpeg;
use crate::image::Image;

/// The outcome of a save attempt (`FormSignature.sbSave_Click`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveOutcome {
    /// The guard failed; the window shows the warning and stays open.
    TooSmall,
    /// Accepted; [`SignPadSession::jpeg`] holds the encoded signature.
    Accepted,
}

/// The signature pad as `FormSignature` drives it. The real reference control was a
/// Topaz pad via the managed SigPlusNET; the Rust overlay implements this with an
/// HTML canvas, and tests use a fake.
pub trait Tablet {
    /// `FormSignature_Load`: `SetTabletState(1)`, image size 495×285, justify 5.
    fn open(&mut self);
    /// `sbClear_Click`: `ClearTablet()`.
    fn clear(&mut self);
    /// `GetSigImage()`: the current signature image (a blank pad when nothing drawn).
    fn get_image(&mut self) -> Image;
    /// `SetTabletState(0)` — on a successful save and on closing.
    fn close(&mut self);
}

/// `FormSignature`'s logic.
pub struct SignPadSession {
    tablet: Box<dyn Tablet>,
    jpeg: Option<Vec<u8>>,
}

impl SignPadSession {
    /// Build the session over `tablet`.
    pub fn new(tablet: Box<dyn Tablet>) -> Self {
        SignPadSession { tablet, jpeg: None }
    }

    /// The accepted signature JPEG, or `None` until a save is accepted.
    pub fn jpeg(&self) -> Option<&[u8]> {
        self.jpeg.as_deref()
    }

    /// `FormSignature_Load`.
    pub fn open(&mut self) {
        self.tablet.open();
    }

    /// `sbClear_Click`.
    pub fn clear(&mut self) {
        self.tablet.clear();
    }

    /// `sbSave_Click` (`FormSignature.cs:26-41`): reject a too-small signature
    /// (the window stays open), else encode it and turn the tablet off.
    pub fn save(&mut self) -> SaveOutcome {
        let image = self.tablet.get_image();
        if is_too_small(serialized_length(&image)) {
            return SaveOutcome::TooSmall;
        }
        self.jpeg = to_jpeg(&image);
        self.tablet.close();
        SaveOutcome::Accepted
    }

    /// `FormSignature_FormClosing`.
    pub fn close(&mut self) {
        self.tablet.close();
    }
}
