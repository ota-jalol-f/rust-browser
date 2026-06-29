//! Master key derivation.
//!
//! Public surface: [`MasterKey`] and [`generate_salt`]. The key is derived from a
//! user passphrase with Argon2id (memory-hard, resistant to GPU/ASIC brute force)
//! and is zeroized from memory on drop. The raw bytes are never exposed publicly;
//! only the [`cipher`](crate::cipher) module reads them via `pub(crate)`.

use argon2::Argon2;
use sentinel_core::{Error, Result};
use zeroize::{Zeroize, ZeroizeOnDrop};

/// Length of a derived key in bytes (256 bits — required by AES-256-GCM).
pub const KEY_LEN: usize = 32;

/// Length of the random salt used for key derivation, in bytes.
pub const SALT_LEN: usize = 16;

/// A 256-bit symmetric key derived from a passphrase, wiped on drop.
///
/// Construct with [`MasterKey::derive`]. The same `(passphrase, salt)` pair always
/// yields the same key, so the salt must be stored (it is not secret) alongside the
/// ciphertext to allow the key to be re-derived on the next launch.
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct MasterKey([u8; KEY_LEN]);

impl MasterKey {
    /// Derive a key from `passphrase` and `salt` using Argon2id with the crate's
    /// default cost parameters.
    ///
    /// # Errors
    /// Returns [`Error::Crypto`] if `salt` is shorter than [`SALT_LEN`] (Argon2
    /// requires a sufficiently long salt) or if the KDF itself fails.
    pub fn derive(passphrase: &str, salt: &[u8]) -> Result<Self> {
        if salt.len() < SALT_LEN {
            return Err(Error::Crypto(format!(
                "salt must be at least {SALT_LEN} bytes, got {}",
                salt.len()
            )));
        }
        let mut key = [0u8; KEY_LEN];
        Argon2::default()
            .hash_password_into(passphrase.as_bytes(), salt, &mut key)
            .map_err(|e| Error::Crypto(format!("argon2 derivation failed: {e}")))?;
        Ok(Self(key))
    }

    /// Borrow the raw key bytes for use by the cipher module only.
    pub(crate) fn as_bytes(&self) -> &[u8; KEY_LEN] {
        &self.0
    }
}

/// Generate a fresh random salt suitable for [`MasterKey::derive`].
///
/// Uses the operating system CSPRNG. Store the returned salt next to the data it
/// protects; it is not secret but must be stable for a given key.
pub fn generate_salt() -> [u8; SALT_LEN] {
    use aes_gcm::aead::rand_core::RngCore;
    let mut salt = [0u8; SALT_LEN];
    aes_gcm::aead::OsRng.fill_bytes(&mut salt);
    salt
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derivation_is_deterministic() {
        let salt = [7u8; SALT_LEN];
        let a = MasterKey::derive("correct horse battery staple", &salt).unwrap();
        let b = MasterKey::derive("correct horse battery staple", &salt).unwrap();
        assert_eq!(a.as_bytes(), b.as_bytes());
    }

    #[test]
    fn different_salt_yields_different_key() {
        let a = MasterKey::derive("pw", &[1u8; SALT_LEN]).unwrap();
        let b = MasterKey::derive("pw", &[2u8; SALT_LEN]).unwrap();
        assert_ne!(a.as_bytes(), b.as_bytes());
    }

    #[test]
    fn different_passphrase_yields_different_key() {
        let salt = [9u8; SALT_LEN];
        let a = MasterKey::derive("alpha", &salt).unwrap();
        let b = MasterKey::derive("bravo", &salt).unwrap();
        assert_ne!(a.as_bytes(), b.as_bytes());
    }

    #[test]
    fn short_salt_rejected() {
        assert!(matches!(MasterKey::derive("pw", &[0u8; 4]), Err(Error::Crypto(_))));
    }

    #[test]
    fn generated_salts_differ() {
        assert_ne!(generate_salt(), generate_salt());
    }
}
