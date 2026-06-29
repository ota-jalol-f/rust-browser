//! `sentinel-shell`: the browser window and chrome.
//!
//! Exposes [`browser`] — the windowed controller plus the pure `normalize_url`
//! helper. The binary in `main.rs` wires these together. SDK-dependent code is
//! behind the `ultralight` feature, mirroring `sentinel-engine`.

#![warn(missing_docs)]

pub mod browser;
