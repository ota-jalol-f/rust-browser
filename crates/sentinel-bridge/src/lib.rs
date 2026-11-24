//! `sentinel-bridge`: the `window.native.*` gateway between page JS and native
//! capabilities.
//!
//! The bridge has two halves:
//! - [`router`] — pure, fully-tested policy: map a page [`sentinel_ipc::Request`]
//!   to its [`sentinel_permissions::Capability`] and decide forward / prompt /
//!   deny. This is where security lives; the rest is plumbing.
//! - `jsapi` (feature `ultralight`) — installs `window.native` into a page's
//!   JavaScriptCore context and marshals calls to the router. Compiled only with
//!   the engine present; verified at compile time against the real `ul-next` API.
//!
//! Keeping policy in [`router`] means the dangerous surface (JS marshalling) makes
//! no decisions of its own.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod dispatch;
pub mod router;

pub use dispatch::{dispatch_json, DispatchReport};
pub use router::{required_capability, BridgeOutcome, BridgeRouter};
