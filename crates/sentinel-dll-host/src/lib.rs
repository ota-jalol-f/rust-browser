//! `sentinel-dll-host`: the out-of-process host that loads and calls native DLLs.
//!
//! This is the most dangerous capability in Sentinel — running arbitrary native
//! code — so it is isolated in its own process and every load is gated by a
//! SHA-256 integrity check matching the user's consent. See the honest threat
//! model in the design spec: granting a page DLL access is equivalent to letting
//! it install software.
//!
//! - [`abi`] — the stable `sentinel_invoke` / `sentinel_free` plugin ABI.
//! - [`host`] — [`DllHost`]: load (hash-gated), invoke, unload by handle.
//!
//! `unsafe` is unavoidable here (FFI); it is confined to [`host`] with documented
//! safety contracts rather than blanket-forbidden.

#![warn(missing_docs)]
#![deny(unsafe_op_in_unsafe_fn)]

pub mod abi;
pub mod host;

pub use host::{sha256_hex, DllHost};
