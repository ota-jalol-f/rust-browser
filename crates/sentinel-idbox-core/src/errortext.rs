//! Asbt.UtilsV2 Logs `CError.GetAllMessages`, the body of `FormReader`'s error toasts:
//! every message of the exception chain followed by `"\r\n"`, the entries separated by
//! `"\r\n"` (probe-measured: `"single\r\n"`, `"outer\r\n\r\nmiddle\r\n\r\ninner\r\n"`).
//!
//! Rust has no `InnerException` chain, so the port takes the chain as a slice of
//! messages (outermost first), which is how every caller builds it. Ported from the
//! reference `Asbt.IdBox.Shared.ErrorText`.

/// Join an exception-message chain (outermost first) the way `CError.GetAllMessages`
/// does: each message plus `"\r\n"`, entries separated by `"\r\n"`.
pub fn get_all_messages(chain: &[&str]) -> String {
    let mut text = String::new();
    for message in chain {
        if !text.is_empty() {
            text.push_str("\r\n");
        }
        text.push_str(message);
        text.push_str("\r\n");
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_asbt_utils_v2_logs() {
        assert_eq!(get_all_messages(&["single"]), "single\r\n");
        assert_eq!(
            get_all_messages(&["outer", "middle", "inner"]),
            "outer\r\n\r\nmiddle\r\n\r\ninner\r\n"
        );
    }
}
