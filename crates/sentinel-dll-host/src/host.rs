//! The DLL host: load, invoke, and unload native plugins by handle.
//!
//! This is the one place in Sentinel that runs arbitrary native code, so it lives
//! out of process (isolated from the UI) and gates every load behind a SHA-256
//! check matching the hash the user consented to. The `unsafe` FFI is confined to
//! [`DllHost::invoke`], with the ABI contract documented in [`crate::abi`].

use std::collections::HashMap;
use std::path::Path;

use sentinel_core::{Error, Result};
use sha2::{Digest, Sha256};

use crate::abi::{FreeFn, InvokeFn, FREE_SYMBOL, INVOKE_SYMBOL};

/// Owns loaded plugin libraries, keyed by an opaque handle issued on load.
#[derive(Default)]
pub struct DllHost {
    libs: HashMap<u64, libloading::Library>,
    next_handle: u64,
}

impl DllHost {
    /// Create an empty host.
    pub fn new() -> Self {
        Self { libs: HashMap::new(), next_handle: 1 }
    }

    /// Load the DLL at `path`, returning a handle.
    ///
    /// If `expected_sha256` is `Some`, the file's SHA-256 must match it (case
    /// insensitive) or the load is refused *before* the library is opened — this
    /// is the integrity gate tying a load to the exact bytes the user consented to.
    ///
    /// # Errors
    /// [`Error::Device`] on read failure, hash mismatch, or if the OS cannot load
    /// the library.
    pub fn load(&mut self, path: &Path, expected_sha256: Option<&str>) -> Result<u64> {
        let bytes = std::fs::read(path)
            .map_err(|e| Error::Device(format!("cannot read dll {}: {e}", path.display())))?;
        let digest = sha256_hex(&bytes);
        if let Some(expected) = expected_sha256 {
            if !expected.eq_ignore_ascii_case(&digest) {
                return Err(Error::Device(format!(
                    "dll hash mismatch: expected {expected}, got {digest}"
                )));
            }
        }

        // SAFETY: loading a native library runs its initializers. The caller has
        // verified the bytes (hash gate above) and obtained user consent; that is
        // the trust boundary this whole host exists to enforce.
        let lib = unsafe { libloading::Library::new(path) }
            .map_err(|e| Error::Device(format!("failed to load dll: {e}")))?;

        let handle = self.next_handle;
        self.next_handle += 1;
        self.libs.insert(handle, lib);
        Ok(handle)
    }

    /// Invoke the plugin's `sentinel_invoke` with `input`, returning its output bytes.
    ///
    /// # Errors
    /// [`Error::Device`] if the handle is unknown, the required symbols are absent,
    /// or the plugin signals failure (non-zero return code).
    pub fn invoke(&self, handle: u64, input: &[u8]) -> Result<Vec<u8>> {
        let lib = self
            .libs
            .get(&handle)
            .ok_or_else(|| Error::Device(format!("unknown dll handle {handle}")))?;

        // SAFETY: we look up the two ABI symbols and call them per the documented
        // contract in `crate::abi`. The plugin owns the returned buffer; we copy it
        // out and hand it straight back to `sentinel_free`, so no allocation crosses
        // the ABI boundary in either direction beyond that paired call.
        unsafe {
            let invoke: libloading::Symbol<InvokeFn> = lib
                .get(INVOKE_SYMBOL)
                .map_err(|e| Error::Device(format!("plugin missing sentinel_invoke: {e}")))?;
            let free: libloading::Symbol<FreeFn> = lib
                .get(FREE_SYMBOL)
                .map_err(|e| Error::Device(format!("plugin missing sentinel_free: {e}")))?;

            let mut out_ptr: *mut u8 = std::ptr::null_mut();
            let mut out_len: usize = 0;
            let code = invoke(input.as_ptr(), input.len(), &mut out_ptr, &mut out_len);
            if code != 0 {
                return Err(Error::Device(format!("plugin returned error code {code}")));
            }

            if out_ptr.is_null() || out_len == 0 {
                return Ok(Vec::new());
            }
            let result = std::slice::from_raw_parts(out_ptr, out_len).to_vec();
            free(out_ptr, out_len);
            Ok(result)
        }
    }

    /// Unload a plugin. Returns `true` if a library was removed.
    ///
    /// Dropping the [`libloading::Library`] unloads the DLL.
    pub fn unload(&mut self, handle: u64) -> bool {
        self.libs.remove(&handle).is_some()
    }

    /// Number of currently-loaded plugins.
    pub fn len(&self) -> usize {
        self.libs.len()
    }

    /// True when no plugins are loaded.
    pub fn is_empty(&self) -> bool {
        self.libs.is_empty()
    }
}

/// Lowercase hex SHA-256 of `bytes`.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut s = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write;
        let _ = write!(s, "{byte:02x}");
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn sha256_hex_matches_known_vector() {
        // SHA-256 of the empty input.
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn hash_mismatch_is_rejected_before_load() {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        file.write_all(b"not really a dll").unwrap();
        let mut host = DllHost::new();
        // Wrong expected hash => refused at the integrity gate, no dlopen attempted.
        let err = host.load(file.path(), Some("00ff")).unwrap_err();
        assert!(matches!(err, Error::Device(ref m) if m.contains("hash mismatch")));
    }

    #[test]
    fn invoke_unknown_handle_errors() {
        let host = DllHost::new();
        assert!(matches!(host.invoke(123, b"x"), Err(Error::Device(_))));
    }

    #[test]
    fn unload_unknown_returns_false() {
        let mut host = DllHost::new();
        assert!(!host.unload(123));
        assert!(host.is_empty());
    }
}
