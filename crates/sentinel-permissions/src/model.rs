//! Core permission types: [`Capability`], [`Origin`], and [`PermissionState`].
//!
//! Pure data with no I/O — the decision rules live in [`crate::broker`]. These are
//! the vocabulary every gated subsystem (USB, serial, DLL) speaks.

use std::fmt;

use serde::{Deserialize, Serialize};

/// A native capability a web page may request through the bridge.
///
/// Each variant is a separate, independently-grantable permission: granting USB
/// does not grant serial or DLL access.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Capability {
    /// Access USB devices via `window.native.usb`.
    Usb,
    /// Access serial ports via `window.native.serial`.
    Serial,
    /// Load and call a native DLL via `window.native.dll`. The most dangerous
    /// capability — granting it is equivalent to letting the origin run software.
    Dll,
}

impl Capability {
    /// All capabilities, for iteration (e.g. listing an origin's grants).
    pub const ALL: [Capability; 3] = [Capability::Usb, Capability::Serial, Capability::Dll];

    /// A short human-readable label for consent prompts.
    pub fn label(self) -> &'static str {
        match self {
            Capability::Usb => "USB devices",
            Capability::Serial => "serial ports",
            Capability::Dll => "native code (DLL)",
        }
    }
}

impl fmt::Display for Capability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Capability::Usb => "usb",
            Capability::Serial => "serial",
            Capability::Dll => "dll",
        };
        f.write_str(s)
    }
}

/// A web origin, normalized to `scheme://host[:port]` (no path, no trailing slash).
///
/// Permissions are keyed by origin, so two URLs from the same origin share grants
/// while different origins are isolated. Construct via [`Origin::parse`].
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Origin(String);

impl Origin {
    /// Parse and normalize the origin of a URL.
    ///
    /// Keeps scheme + host + optional explicit port; drops path, query, fragment,
    /// and any trailing slash. Lowercases scheme and host.
    ///
    /// # Errors
    /// Returns [`sentinel_core::Error::Navigation`] if `url` has no `scheme://host`.
    pub fn parse(url: &str) -> sentinel_core::Result<Self> {
        let url = url.trim();
        let (scheme, rest) = url
            .split_once("://")
            .ok_or_else(|| sentinel_core::Error::Navigation(format!("no scheme in origin: {url}")))?;
        if scheme.is_empty() || rest.is_empty() {
            return Err(sentinel_core::Error::Navigation(format!("invalid origin: {url}")));
        }
        // Authority ends at the first '/', '?', or '#'.
        let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
        let authority = &rest[..authority_end];
        if authority.is_empty() {
            return Err(sentinel_core::Error::Navigation(format!("empty host in origin: {url}")));
        }
        Ok(Origin(format!("{}://{}", scheme.to_ascii_lowercase(), authority.to_ascii_lowercase())))
    }

    /// The normalized origin string, e.g. `https://example.com`.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Origin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// The current decision for an (origin, capability) pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PermissionState {
    /// Explicitly allowed; the bridge proceeds without prompting.
    Granted,
    /// Explicitly blocked; the bridge rejects without prompting.
    Denied,
    /// No decision recorded — the bridge must prompt the user. This is the
    /// secure default for any origin/capability never seen before.
    Ask,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origin_strips_path_query_fragment() {
        let o = Origin::parse("https://Example.com/some/path?q=1#frag").unwrap();
        assert_eq!(o.as_str(), "https://example.com");
    }

    #[test]
    fn origin_keeps_explicit_port() {
        let o = Origin::parse("http://localhost:8080/app").unwrap();
        assert_eq!(o.as_str(), "http://localhost:8080");
    }

    #[test]
    fn origin_requires_scheme_and_host() {
        assert!(Origin::parse("example.com").is_err());
        assert!(Origin::parse("https://").is_err());
        assert!(Origin::parse("https:///path").is_err());
    }

    #[test]
    fn same_origin_different_paths_are_equal() {
        let a = Origin::parse("https://a.com/x").unwrap();
        let b = Origin::parse("https://a.com/y").unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn capability_serde_is_lowercase() {
        assert_eq!(serde_json::to_string(&Capability::Usb).unwrap(), "\"usb\"");
        assert_eq!(serde_json::to_string(&Capability::Dll).unwrap(), "\"dll\"");
    }

    #[test]
    fn capability_all_covers_every_variant() {
        assert_eq!(Capability::ALL.len(), 3);
    }
}
