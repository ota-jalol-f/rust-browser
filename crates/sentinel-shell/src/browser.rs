//! Address-bar input normalization.
//!
//! The windowed browser lives in [`crate::app`] (winit + the engine's low-level
//! renderer). This module holds only the pure, unit-tested URL helper, so it
//! builds and tests without the SDK.

/// Normalize raw address-bar input into a loadable URL.
///
/// - Empty/whitespace → `"about:blank"`.
/// - Already has a recognized scheme (`http:`, `https:`, `file:`, `about:`) → unchanged.
/// - Otherwise → prefixed with `https://` (privacy-first: never silently downgrade
///   a bare host to plaintext `http`).
pub fn normalize_url(input: &str) -> String {
    let s = input.trim();
    if s.is_empty() {
        return "about:blank".into();
    }
    let has_scheme = ["http:", "https:", "file:", "about:"]
        .iter()
        .any(|p| s.starts_with(p));
    if has_scheme {
        s.to_string()
    } else {
        format!("https://{s}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bare_host_gets_https() {
        assert_eq!(normalize_url("example.com"), "https://example.com");
    }

    #[test]
    fn known_schemes_preserved() {
        assert_eq!(normalize_url("http://x.test"), "http://x.test");
        assert_eq!(normalize_url("https://x.test"), "https://x.test");
        assert_eq!(normalize_url("about:blank"), "about:blank");
        assert_eq!(normalize_url("file:///c:/page.html"), "file:///c:/page.html");
    }

    #[test]
    fn empty_or_whitespace_is_blank() {
        assert_eq!(normalize_url(""), "about:blank");
        assert_eq!(normalize_url("   "), "about:blank");
    }

    #[test]
    fn surrounding_whitespace_trimmed() {
        assert_eq!(normalize_url("  example.com  "), "https://example.com");
    }
}
