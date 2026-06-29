//! `sentinel-crypto`: at-rest encryption for the Sentinel browser.
//!
//! Implements the storage half of Sentinel's #1 security pillar: history,
//! bookmarks, cookies, and credentials are encrypted on disk so that reading the
//! database file reveals nothing without the user's passphrase.
//!
//! - [`key`] — Argon2id key derivation ([`MasterKey`]) + salt generation.
//! - [`cipher`] — AES-256-GCM authenticated [`encrypt`]/[`decrypt`].
//!
//! Authenticated encryption means tampering and wrong-key use are *detected*, not
//! silently passed through. Key material is zeroized from memory on drop.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod cipher;
pub mod key;

pub use cipher::{decrypt, encrypt};
pub use key::{generate_salt, MasterKey, KEY_LEN, SALT_LEN};
