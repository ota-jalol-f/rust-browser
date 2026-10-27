//! `ElyMRTDDotNet.calculateChecksum(field + checkDigit)` re-implemented in managed
//! code so the MRZ half of `FormReader` runs without the x86 SDK. Probe-measured
//! against the real x86 Ely function (`tests/fixtures/reference.json`: 212 cases + a
//! map of every printable ASCII character):
//!
//! - the last character is the check digit, the rest is the field, weights 7-3-1 from
//!   the left;
//! - a character's value is `c - '0'` below `'A'` (so `'0'..'9'` = 0..9, `':'` = 10,
//!   `'&'` = -10) except `'<'` = 0, and `c - 55` from `'A'` on (`A..Z` = 10..35, lower
//!   case continues: `'a'` = 42);
//! - true when `sum % 10` (C remainder semantics) equals `checkDigit - '0'`; so a `'<'`
//!   or letter check digit is never valid, and the empty string is false.
//!
//! For upper-case A-Z, 0-9 and `'<'` (what the ID BOX sends) this is the ICAO 9303
//! check digit. Non-ASCII input is not modelled (the ID BOX sends ASCII only). Ported
//! from the reference `Asbt.IdBox.Plugin.Mrz.ElyChecksum`.

const WEIGHTS: [i32; 3] = [7, 3, 1];

/// `ElyChecksum.Verify`: `true` when the final character is the valid check digit of
/// the preceding field under the Ely weighting. `false` for the empty string and for
/// any non-digit check digit.
pub fn verify(field_and_digit: &str) -> bool {
    let bytes = field_and_digit.as_bytes();
    if bytes.is_empty() {
        return false;
    }

    let last = bytes.len() - 1;
    let mut sum: i32 = 0;
    for (i, &b) in bytes[..last].iter().enumerate() {
        sum += value(b) * WEIGHTS[i % 3];
    }

    // C# `sum % 10` and `char - '0'`: Rust `%` is also truncated toward zero, so a
    // negative sum (e.g. a field with '&') matches the C# result.
    sum % 10 == i32::from(bytes[last]) - i32::from(b'0')
}

fn value(c: u8) -> i32 {
    if c == b'<' {
        return 0;
    }
    if c < b'A' {
        return i32::from(c) - i32::from(b'0');
    }
    i32::from(c) - 55
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icao_examples() {
        assert!(verify("L898902C<3"));
        assert!(!verify("L898902C<8"));
        assert!(verify("7408122"));
        assert!(!verify("740812<"));
        assert!(verify("1204159"));
        assert!(verify("AA12345678"));
    }

    #[test]
    fn empty_is_false() {
        assert!(!verify(""));
    }

    #[test]
    fn letter_or_filler_check_digit_never_valid() {
        assert!(!verify("L898902C<A"));
        assert!(!verify("L898902C<<"));
    }
}
