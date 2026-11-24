//! The signer used when no E-Kalit backend is linked (every build — the vendor
//! `libesigner.net` is managed .NET and no native API is in the source, so there is
//! no Rust token backend yet). It reports the token not connected, so every signing
//! path returns the reference's "E-Kalit уланмаган" Answere.

use sentinel_ekalit_core::signer::{SignOutcome, Signer};
use sentinel_ekalit_core::texts;

/// A signer with no token behind it.
pub struct StubSigner;

impl Signer for StubSigner {
    fn is_connected(&mut self) -> bool {
        false
    }
    fn get_uid(&mut self) -> Result<String, String> {
        Err(texts::NOT_CONNECTED.to_string())
    }
    fn sign(&mut self, _pin: &str, _value: &str) -> Result<SignOutcome, String> {
        Err(texts::NOT_CONNECTED.to_string())
    }
}
