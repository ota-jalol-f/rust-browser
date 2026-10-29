//! Standard base64, shaped to match the two .NET calls the reference uses:
//! `Convert.ToBase64String` (the saved photo → `data`) and
//! `Convert.FromBase64String` (validating / decoding `currentPhoto`).

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Encode bytes as standard base64 with `=` padding, like `Convert.ToBase64String`
/// (no line breaks).
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

/// Decode standard base64 the way `Convert.FromBase64String` does, returning `None`
/// on anything it would reject.
///
/// .NET ignores the white-space characters space, `\t`, `\n` and `\r` anywhere in
/// the input, then requires the remaining length to be a non-trivial multiple of 4
/// with at most two trailing `=` and no stray characters. Empty-after-whitespace
/// input decodes to an empty vector (as `Convert.FromBase64String("")` returns an
/// empty array).
pub fn decode_dotnet(input: &str) -> Option<Vec<u8>> {
    // Strip exactly the chars .NET treats as insignificant white space.
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

    // Count trailing padding (0..=2); '=' may appear only at the end.
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
            _ => return None, // a '=' inside the body, or any non-alphabet byte
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
    fn encode_matches_dotnet_vectors() {
        assert_eq!(encode(b""), "");
        assert_eq!(encode(b"f"), "Zg==");
        assert_eq!(encode(b"fo"), "Zm8=");
        assert_eq!(encode(b"foo"), "Zm9v");
        assert_eq!(encode(b"foob"), "Zm9vYg==");
        assert_eq!(encode(&[0xFF, 0xD8, 0xFF]), "/9j/");
    }

    #[test]
    fn roundtrip() {
        let data = [0u8, 1, 2, 3, 250, 128, 64, 17];
        assert_eq!(decode_dotnet(&encode(&data)).unwrap(), data);
    }

    #[test]
    fn decode_ignores_whitespace_like_dotnet() {
        assert_eq!(decode_dotnet("Zm9v\r\nYg==").unwrap(), b"foob");
        assert_eq!(decode_dotnet("  Zg ==  ").unwrap(), b"f");
    }

    #[test]
    fn empty_or_all_whitespace_is_empty() {
        assert_eq!(decode_dotnet("").unwrap(), Vec::<u8>::new());
        assert_eq!(decode_dotnet("  \r\n ").unwrap(), Vec::<u8>::new());
    }

    #[test]
    fn bad_length_or_alphabet_is_rejected() {
        assert!(decode_dotnet("Zg=").is_none()); // length 3, not a multiple of 4
        assert!(decode_dotnet("Zm9v!g==").is_none()); // '!' not in the alphabet
        assert!(decode_dotnet("Z=m9").is_none()); // '=' not at the end
    }
}
