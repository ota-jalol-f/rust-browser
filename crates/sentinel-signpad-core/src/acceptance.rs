//! `FormSignature.sbSave_Click`'s guard — `signImage.ToByteArray().Length < 1500` —
//! re-derived without BinaryFormatter (gone in .NET 10).
//!
//! `ToByteArray` was `BinaryFormatter.Serialize(Bitmap)`: a fixed 162-byte header
//! plus the bytes of .NET Framework's `Image.Save(MemoryStream)`, which for the
//! `GetSigImage()` `MemoryBmp` is a PNG. So the reference rejects iff
//! `PNG(sigImage).Length + 162 < 1500`. See `docs/signpad-plugin.md` §4.
//!
//! The RULE below is exact. The PNG *length* comes from a Rust encoder, not GDI+,
//! so it is not byte-identical to the reference's 1282/1572/4127 — the acceptance
//! DECISION on the fixtures is pinned instead (blank rejected, strokes accepted),
//! and this matters only at the near-blank boundary (a real signature is far above
//! it either way).

use crate::image::Image;

/// `FormSignature.cs:29`: the minimum accepted serialized length.
pub const MINIMUM_SERIALIZED_LENGTH: i64 = 1500;

/// BinaryFormatter's fixed bytes around a `Bitmap`'s encoded image.
pub const BINARY_FORMATTER_BITMAP_OVERHEAD: i64 = 162;

/// `IsTooSmall(long)`: the pure rule, `serialized_length < 1500`.
pub fn is_too_small(serialized_length: i64) -> bool {
    serialized_length < MINIMUM_SERIALIZED_LENGTH
}

/// `SerializedLength(Image)`: `PNG(image).len + 162`, what `ToByteArray(sigImage)`
/// returned on .NET Framework.
pub fn serialized_length(image: &Image) -> i64 {
    png_len(image) as i64 + BINARY_FORMATTER_BITMAP_OVERHEAD
}

/// The PNG byte length of `image` (8-bit RGB). Used only for the size rule.
pub fn png_len(image: &Image) -> usize {
    encode_png(image).map(|p| p.len()).unwrap_or(0)
}

/// Encode an image as PNG (8-bit RGB).
fn encode_png(image: &Image) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, image.width as u32, image.height as u32);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_compression(png::Compression::Best);
        encoder.set_filter(png::FilterType::Paeth);
        encoder.set_adaptive_filter(png::AdaptiveFilterType::Adaptive);
        let mut writer = encoder.write_header().ok()?;
        writer.write_image_data(&image.rgb).ok()?;
    }
    Some(out)
}
