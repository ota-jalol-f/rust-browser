//! Browser-wide configuration types.
//!
//! Public surface: [`BrowserConfig`] — the user-tunable settings consumed by the
//! engine and shell at startup. Pure data plus validation; depends only on
//! `serde` (for persistence) and the crate [`crate::error`] module.

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// User-tunable browser settings.
///
/// Construct with [`BrowserConfig::default`] and override fields, or deserialize
/// from a config file. Always call [`BrowserConfig::validate`] before use.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrowserConfig {
    /// URL opened when the browser starts and when the user presses "home".
    pub home_url: String,
    /// Initial window width in logical pixels.
    pub window_width: u32,
    /// Initial window height in logical pixels.
    pub window_height: u32,
    /// Prefer the GPU renderer when the platform supports it. When `false`, or
    /// when the platform lacks a usable GPU path (e.g. Windows 7 without the
    /// D3D11 platform update), the engine falls back to the CPU renderer.
    pub enable_gpu: bool,
}

impl Default for BrowserConfig {
    fn default() -> Self {
        Self {
            home_url: "about:blank".into(),
            window_width: 1280,
            window_height: 800,
            enable_gpu: true,
        }
    }
}

impl BrowserConfig {
    /// Validate the configuration's invariants.
    ///
    /// # Errors
    /// Returns [`Error::Config`] when `home_url` is empty/whitespace or when
    /// either window dimension is zero (which Ultralight cannot render into).
    pub fn validate(&self) -> Result<()> {
        if self.home_url.trim().is_empty() {
            return Err(Error::Config("home_url must not be empty".into()));
        }
        if self.window_width == 0 || self.window_height == 0 {
            return Err(Error::Config("window dimensions must be non-zero".into()));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_valid() {
        assert!(BrowserConfig::default().validate().is_ok());
    }

    #[test]
    fn zero_dimensions_rejected() {
        let c = BrowserConfig { window_width: 0, ..Default::default() };
        assert!(matches!(c.validate(), Err(Error::Config(_))));
    }

    #[test]
    fn empty_home_url_rejected() {
        let c = BrowserConfig { home_url: "   ".into(), ..Default::default() };
        assert!(matches!(c.validate(), Err(Error::Config(_))));
    }

    #[test]
    fn roundtrips_through_json() {
        // serde wiring must stay intact for on-disk config persistence.
        let c = BrowserConfig::default();
        let json = serde_json::to_string(&c).expect("serialize");
        let back: BrowserConfig = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(c, back);
    }
}
