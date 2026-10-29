//! Standard base64 (encode for the result JPEG → `data`, decode for the overlay
//! canvas's PNG data URL).

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Encode bytes as standard base64 with `=` padding (as `Convert.ToBase64String`).
pub fn encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0];
        let b1 = *chunk.get(1).unwrap_or(&0);
        let b2 = *chunk.get(2).unwrap_or(&0);
        let n = (u32::from(b0) << 16) | (u32::from(b1) << 8) | u32::from(b2);
        out.push(ALPHABET[(n >> 18 & 63) as usize] as char);
        out.push(ALPHABET[(n >> 12 & 63) as usize] as char);
        out.push(if chunk.len() > 1 { ALPHABET[(n >> 6 & 63) as usize] as char } else { '=' });
        out.push(if chunk.len() > 2 { ALPHABET[(n & 63) as usize] as char } else { '=' });
    }
    out
}

/// Decode standard base64, ignoring ASCII whitespace; `None` on an invalid body.
pub fn decode(input: &str) -> Option<Vec<u8>> {
    let mut cleaned: Vec<u8> = Vec::with_capacity(input.len());
    for &b in input.as_bytes() {
        match b {
            b' ' | b'\t' | b'\n' | b'\r' => {}
            other => cleaned.push(other),
        }
    }
    if cleaned.is_empty() {
        return Some(Vec::new());
    }
    if cleaned.len() % 4 != 0 {
        return None;
    }
    let mut pad = 0usize;
    while pad < 2 && cleaned.len() > pad && cleaned[cleaned.len() - 1 - pad] == b'=' {
        pad += 1;
    }
    let body = &cleaned[..cleaned.len() - pad];
    let mut out = Vec::with_capacity(cleaned.len() / 4 * 3);
    let mut acc = 0u32;
    let mut bits = 0u32;
    for &c in body {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        };
        acc = (acc << 6) | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let data = [0u8, 1, 2, 250, 128, 64, 17, 255];
        assert_eq!(decode(&encode(&data)).unwrap(), data);
    }

    #[test]
    fn known_vectors() {
        assert_eq!(encode(b"foo"), "Zm9v");
        assert_eq!(encode(&[0xFF, 0xD8, 0xFF, 0xD9]), "/9j/2Q==");
        assert_eq!(decode("Zm9v").unwrap(), b"foo");
    }
}
