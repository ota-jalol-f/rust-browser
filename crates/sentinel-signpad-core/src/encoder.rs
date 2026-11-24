//! `CImageWorks.ImageToByte(new Bitmap(sigImage))`: the signature JPEG pv-winform
//! sends to the server (`ModelSignature.Signature`), 495×285.
//!
//! The reference used the GDI+ default JPEG encoder; this is `jpeg-encoder`, so the
//! bytes are not identical to the C# fixtures (as the Photo pipeline's are not to
//! its GDI+ goldens). The output is the same size/format and within a measured
//! pixel tolerance (see the crate's `tests/`).

use jpeg_encoder::{ColorType, Density, Encoder, SamplingFactor};

use crate::image::Image;

/// The JPEG default quality that best matched the C# fixtures (swept in the tests);
/// the reference itself passed no quality and took the GDI+ default.
const QUALITY: u8 = 90;

/// Encode the signature image as JPEG (the bytes returned, base64, by `Capture`).
/// `None` for an empty image (C#'s `ImageToByte(null)` returned null).
pub fn to_jpeg(image: &Image) -> Option<Vec<u8>> {
    if image.width == 0 || image.height == 0 {
        return None;
    }
    let w = u16::try_from(image.width).ok()?;
    let h = u16::try_from(image.height).ok()?;
    let mut out = Vec::new();
    let mut encoder = Encoder::new(&mut out, QUALITY);
    // GDI+ wrote a 96-dpi JFIF; match it (invisible to the pixels).
    encoder.set_density(Density::Inch { x: 96, y: 96 });
    encoder.set_sampling_factor(SamplingFactor::F_2_2); // 4:2:0, the GDI+ default
    encoder.encode(&image.rgb, w, h, ColorType::Rgb).ok()?;
    Some(out)
}
