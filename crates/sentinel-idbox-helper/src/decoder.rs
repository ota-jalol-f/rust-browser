//! The chip-photo decoder for the helper.
//!
//! The reference used FreeImage to decode the DG2/DG6 image (JPEG or JPEG 2000) and
//! re-encode it as JPEG. Here we pass the bytes through: a JPEG DG2 (the common case)
//! reaches the page unchanged, base64-encoded. **Limitation:** a JPEG 2000 DG2 is not
//! transcoded to JPEG (FreeImage parity) — it is forwarded as-is; adding a JP2 codec is
//! the one remaining step for full photo fidelity.

use sentinel_idbox_core::chip::PhotoDecoder;

/// A pass-through photo decoder.
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
