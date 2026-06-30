//! `sentinel-ipc`: the typed protocol between the main process and its isolated
//! host processes (device host for USB/serial, DLL host for native code).
//!
//! Process isolation is how Sentinel contains the blast radius of native faults:
//! a crashing USB driver or a misbehaving page-supplied DLL takes down only its
//! host, never the browser. This crate defines what crosses that boundary.
//!
//! - [`message`] — [`Request`]/[`Response`] and their correlation envelopes.
//! - [`wire`] — length-delimited framing ([`encode`], [`FrameReader`]) for streams.
//!
//! Pure and transport-agnostic: no sockets or pipes here, so the contract is
//! fully unit-tested. The actual `tokio` transport is wired in the host crates.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod executor;
pub mod message;
pub mod wire;

pub use executor::RequestExecutor;
pub use message::{Request, RequestEnvelope, Response, ResponseEnvelope, UsbDeviceInfo};
pub use wire::{encode, FrameReader, MAX_FRAME_LEN};
