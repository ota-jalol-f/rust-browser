//! `sentinel-device-host`: the out-of-process host for USB and serial access.
//!
//! Runs isolated from the main browser so a misbehaving driver or device cannot
//! crash the UI. It receives [`sentinel_ipc::Request`]s, performs the hardware
//! operation via a [`DeviceBackend`], and returns [`sentinel_ipc::Response`]s.
//!
//! - [`backend`] — [`DeviceBackend`] trait + real [`SystemBackend`] (nusb/serialport).
//! - [`host`] — [`DeviceHost`] request router (enumeration implemented; per-device
//!   I/O reported as not-yet-implemented rather than faked).
//!
//! Routing is unit-tested with a mock backend; the real backend's enumeration is
//! exercised by an ignored integration test that runs against this machine.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod backend;
pub mod host;

pub use backend::{DeviceBackend, SystemBackend};
pub use host::DeviceHost;
