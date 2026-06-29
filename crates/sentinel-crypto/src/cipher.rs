//! Authenticated encryption of stored data.
//!
//! Public surface: [`encrypt`] and [`decrypt`]. Uses AES-256-GCM, which provides
//! both confidentiality and integrity: a wrong key or any tampering with the
//! ciphertext is *detected* on decrypt and surfaced as [`Error::Crypto`] rather
//! than returning corrupt plaintext.
//!
//! ## Wire format
//! `encrypt` returns `nonce (12 bytes) || ciphertext-with-tag`. A fresh random
//! nonce is generated per call (critical for GCM security — never reuse a nonce
//! with the same key), so encrypting identical plaintext twice yields different
//! blobs. `decrypt` reverses this layout.

use aes_gcm::{
    aead::{Aead, KeyInit, OsRng, rand_core::RngCore},
    Aes256Gcm, Key, Nonce,
};
use sentinel_core::{Error, Result};

use crate::key::MasterKey;

/// Length of the AES-GCM nonce in bytes.
const NONCE_LEN: usize = 12;

/// Encrypt `plaintext` under `key`, returning `nonce || ciphertext||tag`.
///
/// # Errors
/// Returns [`Error::Crypto`] if the AEAD layer fails (practically only on
/// allocation failure for in-memory data).
pub fn encrypt(key: &MasterKey, plaintext: &[u8]) -> Result<Vec<u8>> {
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key.as_bytes()));

    let mut nonce_bytes = [0u8; NONCE_LEN];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext)
        .map_err(|e| Error::Crypto(format!("encryption failed: {e}")))?;

    let mut out = Vec::with_capacity(NONCE_LEN + ciphertext.len());
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

/// Decrypt a `nonce || ciphertext||tag` blob produced by [`encrypt`].
///
/// # Errors
/// Returns [`Error::Crypto`] if `blob` is too short to contain a nonce, or if
/// authentication fails — i.e. the key is wrong or the ciphertext was tampered
/// with. A failed authentication never yields partial/garbage plaintext.
pub fn decrypt(key: &MasterKey, blob: &[u8]) -> Result<Vec<u8>> {
    if blob.len() < NONCE_LEN {
        return Err(Error::Crypto(format!(
            "ciphertext too short: need at least {NONCE_LEN} bytes for the nonce"
        )));
    }
    let (nonce_bytes, ciphertext) = blob.split_at(NONCE_LEN);
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key.as_bytes()));
    let nonce = Nonce::from_slice(nonce_bytes);

    cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| Error::Crypto("decryption/authentication failed (wrong key or tampered data)".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::key::MasterKey;

    fn test_key(seed: u8) -> MasterKey {
        MasterKey::derive("passphrase", &[seed; crate::key::SALT_LEN]).unwrap()
    }

    #[test]
    fn roundtrip_recovers_plaintext() {
        let key = test_key(1);
        let msg = b"super secret bookmark url";
        let blob = encrypt(&key, msg).unwrap();
        assert_eq!(decrypt(&key, &blob).unwrap(), msg);
    }

    #[test]
    fn nonce_makes_ciphertext_nondeterministic() {
        let key = test_key(1);
        let msg = b"same plaintext";
        let a = encrypt(&key, msg).unwrap();
        let b = encrypt(&key, msg).unwrap();
        assert_ne!(a, b, "fresh nonce must make blobs differ");
        // ...but both still decrypt to the same plaintext.
        assert_eq!(decrypt(&key, &a).unwrap(), decrypt(&key, &b).unwrap());
    }

    #[test]
    fn wrong_key_fails_authentication() {
        let blob = encrypt(&test_key(1), b"data").unwrap();
        assert!(matches!(decrypt(&test_key(2), &blob), Err(Error::Crypto(_))));
    }

    #[test]
    fn tampered_ciphertext_fails_authentication() {
        let key = test_key(1);
        let mut blob = encrypt(&key, b"data").unwrap();
        let last = blob.len() - 1;
        blob[last] ^= 0xff; // flip bits in the tag/ciphertext
        assert!(matches!(decrypt(&key, &blob), Err(Error::Crypto(_))));
    }

    #[test]
    fn short_blob_rejected() {
        assert!(matches!(decrypt(&test_key(1), &[0u8; 4]), Err(Error::Crypto(_))));
    }

    #[test]
    fn empty_plaintext_roundtrips() {
        let key = test_key(1);
        let blob = encrypt(&key, b"").unwrap();
        assert_eq!(decrypt(&key, &blob).unwrap(), b"");
    }
}
