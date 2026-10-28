//! The chip-photo decoder for the helper — FreeImage parity for DG2/DG6.
//!
//! The reference used FreeImage to decode the chip photo (JPEG **or JPEG 2000**) and
//! hand a JPEG to the page. [`FreeImageParityDecoder`] (the `device` build) does the
//! same with pure-Rust crates — a JPEG is passed through unchanged, a JPEG 2000 is
//! decoded (`j2k-native`) and re-encoded as JPEG (`jpeg-encoder`) — so the i686 Win7
//! helper keeps building with no C toolchain. [`PassthroughDecoder`] (every other build)
//! just base64-encodes the bytes; the stub SDK never reads a real photo anyway.

use sentinel_idbox_core::chip::PhotoDecoder;

/// A pass-through photo decoder (non-device builds).
pub struct PassthroughDecoder;

impl PhotoDecoder for PassthroughDecoder {
    fn decode(&self, photo: &[u8]) -> Option<Vec<u8>> {
        if photo.is_empty() {
            None
        } else {
            Some(photo.to_vec())
        }
    }

    fn to_jpeg_base64(&self, decoded: &[u8]) -> Option<String> {
        Some(base64(decoded))
    }
}

/// FreeImage-parity decoder: produces JPEG bytes for the page (JPEG pass-through, JPEG
/// 2000 transcoded). The `decode` step yields the final JPEG, `to_jpeg_base64` just
/// base64-encodes it.
#[cfg(feature = "device")]
#[cfg_attr(not(target_arch = "x86"), allow(dead_code))]
pub struct FreeImageParityDecoder;

#[cfg(feature = "device")]
impl PhotoDecoder for FreeImageParityDecoder {
    fn decode(&self, photo: &[u8]) -> Option<Vec<u8>> {
        if photo.is_empty() {
            return None;
        }
        // Already JPEG (SOI FF D8 FF): keep it as-is — no re-encode, no quality loss.
        if photo.starts_with(&[0xFF, 0xD8, 0xFF]) {
            return Some(photo.to_vec());
        }
        // Otherwise treat it as JPEG 2000 (JP2/JPH wrapper or raw J2C codestream) and
        // transcode to JPEG; anything j2k-native cannot decode fails like FreeImage did.
        transcode_jpeg2000(photo)
    }

    fn to_jpeg_base64(&self, decoded: &[u8]) -> Option<String> {
        Some(base64(decoded))
    }
}

/// Decode a JPEG 2000 image to 8-bit samples (`j2k-native`) and re-encode it as JPEG
/// (`jpeg-encoder`), both pure Rust.
#[cfg(feature = "device")]
#[cfg_attr(not(target_arch = "x86"), allow(dead_code))]
fn transcode_jpeg2000(photo: &[u8]) -> Option<Vec<u8>> {
    use jpeg_encoder::{ColorType, Encoder};

    let image = j2k_native::Image::new(photo, &j2k_native::DecodeSettings::default()).ok()?;
    let width = image.width();
    let height = image.height();
    let data = image.decode().ok()?; // 8-bit interleaved samples

    let pixels = (width as usize).checked_mul(height as usize)?;
    if pixels == 0 || data.is_empty() {
        return None;
    }
    // The high-level decode path is 8-bit; the channel count is the sample stride.
    let color = match data.len() / pixels {
        1 => ColorType::Luma,
        3 => ColorType::Rgb,
        4 => ColorType::Rgba, // alpha dropped by the JPEG encoder (JPEG has none)
        _ => return None,
    };
    // JPEG dimensions are 16-bit; a passport face image is well within range.
    let (w, h) = (u16::try_from(width).ok()?, u16::try_from(height).ok()?);

    let mut out = Vec::new();
    Encoder::new(&mut out, 90).encode(&data, w, h, color).ok()?;
    Some(out)
}

/// Standard base64 (as `Convert.ToBase64String`).
pub fn base64(data: &[u8]) -> String {
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in data.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        out.push(T[(n >> 18 & 63) as usize] as char);
        out.push(T[(n >> 12 & 63) as usize] as char);
        out.push(if chunk.len() > 1 { T[(n >> 6 & 63) as usize] as char } else { '=' });
        out.push(if chunk.len() > 2 { T[(n & 63) as usize] as char } else { '=' });
    }
    out
}
