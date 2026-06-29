//! Crate-wide error and result types.
//!
//! Public surface: [`Error`] (all failure modes surfaced by the core and engine
//! layers) and [`Result`] (the crate's result alias). The only dependency is
//! `thiserror`, which derives the `std::error::Error` implementation and the
//! `Display` messages.

use thiserror::Error;

/// All failure modes surfaced by Sentinel's core and engine layers.
///
/// Variants are intentionally coarse-grained: each carries a human-readable
/// message (or a wrapped source error) rather than a deep error tree, because
/// these errors are surfaced to a UI and logged, not pattern-matched in bulk.
#[derive(Debug, Error)]
pub enum Error {
    /// The rendering engine (Ultralight) failed to initialize or operate.
    #[error("engine error: {0}")]
    Engine(String),

    /// An underlying I/O operation failed (file access, etc.).
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// Configuration was invalid (see [`crate::config::BrowserConfig::validate`]).
    #[error("config error: {0}")]
    Config(String),

    /// A cryptographic operation failed — key derivation, encryption, or, most
    /// importantly, **authentication** (wrong key or tampered ciphertext on decrypt).
    #[error("crypto error: {0}")]
    Crypto(String),

    /// A storage / database operation failed.
    #[error("storage error: {0}")]
    Storage(String),

    /// A navigation request was invalid — bad URL, empty history, or a cursor
    /// move with no destination.
    #[error("navigation error: {0}")]
    Navigation(String),
}

/// Convenience alias used throughout the crate: `Result<T>` == `Result<T, Error>`.
pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn io_error_converts_via_from() {
        // `?` on a std::io::Error should yield Error::Io thanks to #[from].
        fn inner() -> Result<()> {
            let _ = std::fs::File::open("\0nonexistent-\0path")?;
            Ok(())
        }
        assert!(matches!(inner(), Err(Error::Io(_))));
    }

    #[test]
    fn display_includes_context() {
        let e = Error::Config("home_url must not be empty".into());
        assert_eq!(e.to_string(), "config error: home_url must not be empty");
    }
}
