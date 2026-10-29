//! `Asbt.UtilsV2.Logs CError.GetAllMessages`, as ported on the idbox branch and
//! used by `FormCaptureDocument`: every message of the exception/`InnerException`
//! chain, each followed by `"\r\n"`, the entries joined by `"\r\n"`.
//!
//! This is the detail text the error boxes drop and the body of the
//! "Cуратга олишда ҳато" toast. The input is the already-unwound message chain
//! (outermost first), since Rust has no exception objects.

/// Format a message chain exactly as `CError.GetAllMessages` did.
///
/// Each message is appended followed by `"\r\n"`; a `"\r\n"` separator is inserted
/// before every entry except the first. For a single message `m` the result is
/// `"{m}\r\n"`; for `["a", "b"]` it is `"a\r\n\r\nb\r\n"`. An empty chain is `""`.
pub fn get_all_messages(chain: &[&str]) -> String {
    let mut text = String::new();
    for msg in chain {
        if !text.is_empty() {
            text.push_str("\r\n");
        }
        text.push_str(msg);
        text.push_str("\r\n");
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_chain_is_empty() {
        assert_eq!(get_all_messages(&[]), "");
    }

    #[test]
    fn single_message_gets_one_crlf() {
        assert_eq!(get_all_messages(&["boom"]), "boom\r\n");
    }

    #[test]
    fn chain_is_separated_and_each_terminated() {
        assert_eq!(get_all_messages(&["a", "b"]), "a\r\n\r\nb\r\n");
    }
}
