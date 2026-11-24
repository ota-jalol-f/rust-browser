//! The Sentinel native plugin ABI (version 1).
//!
//! A page-supplied or browser-installed DLL participates by exporting two C
//! functions. The bytes-in / bytes-out shape keeps the ABI general (the plugin
//! decides how to interpret the input — JSON, a custom binary protocol, etc.)
//! while staying simple enough to call safely from the host.
//!
//! ```c
//! // Process `in_len` bytes at `in_ptr`. On success return 0 and set
//! // *out_ptr / *out_len to a buffer the plugin allocated; the host copies it
//! // and then calls sentinel_free on it. Return non-zero on error.
//! int32_t sentinel_invoke(const uint8_t* in_ptr, size_t in_len,
//!                         uint8_t** out_ptr, size_t* out_len);
//!
//! // Free a buffer previously returned by sentinel_invoke.
//! void sentinel_free(uint8_t* ptr, size_t len);
//! ```
//!
//! (For plugins that wrap an existing C library, `bindgen` can generate the
//! Rust↔C bindings the plugin uses internally; this ABI is the stable boundary
//! the host depends on.)

/// `sentinel_invoke` signature: see the module ABI description.
pub type InvokeFn =
    unsafe extern "C" fn(*const u8, usize, *mut *mut u8, *mut usize) -> i32;

/// `sentinel_free` signature: frees a buffer returned by [`InvokeFn`].
pub type FreeFn = unsafe extern "C" fn(*mut u8, usize);

/// Exported symbol name for the invoke entry point.
pub const INVOKE_SYMBOL: &[u8] = b"sentinel_invoke";

/// Exported symbol name for the free function.
pub const FREE_SYMBOL: &[u8] = b"sentinel_free";
