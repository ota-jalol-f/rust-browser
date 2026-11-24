//! `sentinel-ipc`: the typed protocol between the main process and its isolated
//! host processes (device host for USB/serial, DLL host for native code).
//!
//! Process isolation is how Sentinel contains the blast radius of native faults:
//! a crashing USB driver or a misbehaving page-supplied DLL takes down only its
//! host, never the browser. This crate defines what crosses that boundary.
//!
//! - [`message`] — [`Request`]/[`Response`] and their correlation envelopes.
//! - [`wire`] — length-delimited framing ([`encode`], [`FrameReader`]) for streams.
//! - [`transport`] — a synchronous [`serve`]/[`Client`] request-response loop over
//!   any `Read + Write` stream. The platform pipe binding and the out-of-process
//!   helper executable layer on top of it, in a host crate.
//!
//! Pure and transport-agnostic: no sockets or pipes here — the transport works
//! over any byte stream, so the whole contract is unit-tested over loopback.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod executor;
pub mod message;
pub mod transport;
pub mod wire;

pub use executor::RequestExecutor;
pub use message::{Request, RequestEnvelope, Response, ResponseEnvelope, UsbDeviceInfo};
pub use transport::{serve, Client};
pub use wire::{encode, FrameReader, MAX_FRAME_LEN};
