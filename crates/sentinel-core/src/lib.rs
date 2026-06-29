//! `sentinel-core`: shared, GUI-free types for the Sentinel browser.
//!
//! This crate is pure Rust and fully unit-tested. It holds the cross-cutting
//! pieces every other crate depends on:
//!
//! - [`error`] — the [`Error`] type and [`Result`] alias used everywhere.
//! - [`config`] — [`BrowserConfig`], the user-tunable startup settings.
//! - [`nav`] — [`History`], the back/forward/reload state machine for one tab.
//!
//! Keeping these here (rather than in the engine or shell) means navigation and
//! configuration logic can be tested without an Ultralight SDK or a display.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod config;
pub mod error;
pub mod nav;

pub use config::BrowserConfig;
pub use error::{Error, Result};
pub use nav::History;
