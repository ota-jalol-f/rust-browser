//! `sentinel-permissions`: the per-origin consent gate for native capabilities.
//!
//! Every native capability (USB, serial, DLL) is gated here. The bridge asks the
//! [`PermissionBroker`] whether an [`Origin`] may use a [`Capability`]; the secure
//! default for anything never decided is [`PermissionState::Ask`], which drives a
//! user consent prompt.
//!
//! - [`model`] — vocabulary: [`Capability`], [`Origin`], [`PermissionState`].
//! - [`broker`] — [`PermissionBroker`] trait + tested [`InMemoryBroker`].
//!
//! Pure logic, no I/O — a persistent (encrypted) broker can implement the same
//! trait without touching callers.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod broker;
pub mod model;

pub use broker::{InMemoryBroker, PermissionBroker};
pub use model::{Capability, Origin, PermissionState};
