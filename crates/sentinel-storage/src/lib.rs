//! `sentinel-storage`: encrypted persistence for the Sentinel browser.
//!
//! A thin layer over SQLite (`rusqlite`, bundled) where every sensitive column
//! (URLs, titles) is stored as AES-256-GCM ciphertext produced by
//! [`sentinel_crypto`]. Reading the database file off disk therefore reveals
//! nothing without the user's [`MasterKey`](sentinel_crypto::MasterKey).
//!
//! Non-sensitive columns (row id, timestamps) are stored in the clear so the
//! database can order and paginate without decrypting every row.
//!
//! ## Public surface
//! - [`Store`] — open a database and read/write [`HistoryEntry`] and [`Bookmark`].
//!
//! Sensitive-vs-plaintext column choices are a deliberate, documented trade-off:
//! see the per-method docs.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod store;

pub use store::{Bookmark, HistoryEntry, Store};
