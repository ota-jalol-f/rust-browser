//! `WSQProcesses.RemoveNullElements`, split so the "too large" warning is a return
//! value instead of a MessageBox.

/// C#'s threshold: the warning fires when the last non-zero index is > 18000
/// (length ≥ 18002).
pub const WARN_LAST_INDEX: usize = 18000;

/// Drop the trailing zero bytes of the `RS_EncodeWSQ` buffer. The second value is
/// C#'s `i > 18000` (the caller shows `WsqTooLarge`; the finger is kept).
///
/// C# walked back from the end with no lower bound, so an all-zero buffer threw
/// `IndexOutOfRange`; here it yields an empty array, which `build` then rejects with
/// "Бармоқ излари тўлиқ берилмаган".
pub fn remove_null_elements(wsq_with_null: &[u8]) -> (Vec<u8>, bool) {
    // Last non-zero index, or None for an all-zero / empty buffer.
    let last = wsq_with_null.iter().rposition(|&b| b != 0);
    match last {
        Some(i) => (wsq_with_null[..=i].to_vec(), i > WARN_LAST_INDEX),
        None => (Vec::new(), false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trims_trailing_zeros() {
        let (bar, too_large) = remove_null_elements(&[1, 2, 3, 0, 0, 0]);
        assert_eq!(bar, [1, 2, 3]);
        assert!(!too_large);
    }

    #[test]
    fn all_zeros_is_empty_not_a_panic() {
        let (bar, too_large) = remove_null_elements(&[0, 0, 0]);
        assert!(bar.is_empty());
        assert!(!too_large);
    }

    #[test]
    fn boundary_18000_18001() {
        // last non-zero index 18000 (len 18001) → not too large.
        let mut a = vec![1u8; 18001];
        let (bar, too_large) = remove_null_elements(&a);
        assert_eq!(bar.len(), 18001);
        assert!(!too_large);
        // last non-zero index 18001 (len 18002) → too large.
        a = vec![1u8; 18002];
        let (bar, too_large) = remove_null_elements(&a);
        assert_eq!(bar.len(), 18002);
        assert!(too_large);
    }

    #[test]
    fn empty_input() {
        let (bar, too_large) = remove_null_elements(&[]);
        assert!(bar.is_empty());
        assert!(!too_large);
    }
}
