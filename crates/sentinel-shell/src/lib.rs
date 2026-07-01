//! `sentinel-shell`: the browser window and chrome.
//!
//! - [`browser`] — the pure `normalize_url` address-bar helper (always built/tested).
//! - [`dashboard`] — the built-in start page HTML.
//! - `app` (feature `ultralight`) — the winit + softbuffer window that presents the
//!   engine's low-level renderer and forwards input. This is the windowed browser.

#![warn(missing_docs)]

pub mod browser;
pub mod chrome;
pub mod dashboard;

#[cfg(feature = "ultralight")]
pub mod app;
