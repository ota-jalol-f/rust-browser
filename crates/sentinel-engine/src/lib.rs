//! `sentinel-engine`: Ultralight integration for the Sentinel browser.
//!
//! This crate wraps [`ul-next`](https://docs.rs/ul-next) 0.5.4. It is split so
//! that the **only** part requiring the proprietary Ultralight SDK lives behind
//! the off-by-default `ultralight` Cargo feature. With the feature disabled the
//! crate still builds (it just exposes no engine entry points), which keeps the
//! rest of the workspace verifiable in environments without the SDK.
//!
//! ## Runtime requirement
//! The `ultralight` feature uses `ul-next`'s `loaded` mode: the Ultralight shared
//! libraries (`UltralightCore`, `WebCore`, `Ultralight`) are `dlopen`'d at runtime
//! via `libloading`. They must be on the system library path (next to the
//! executable on Windows). See `crates/sentinel-engine/README.md` for setup.
//!
//! ## Public surface
//! - [`headless::render_html_to_bgra`] (feature `ultralight`) — render HTML to a
//!   premultiplied-BGRA pixel buffer using the CPU renderer; used by render smoke
//!   tests and future page thumbnails.

#![warn(missing_docs)]

#[cfg(feature = "ultralight")]
pub mod headless;

#[cfg(feature = "ultralight")]
pub mod jsbridge;

#[cfg(feature = "ultralight")]
pub mod page;
