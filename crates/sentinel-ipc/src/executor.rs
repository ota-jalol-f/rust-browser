//! The [`RequestExecutor`] trait: the seam between bridge policy and host mechanism.
//!
//! Once the bridge has decided a request is permitted, it hands the request to a
//! `RequestExecutor` — implemented by the device host and DLL host — which performs
//! the operation and returns a [`Response`]. Defining the trait here (in the shared
//! protocol crate) lets the bridge depend on the seam without depending on the
//! concrete hosts.

use crate::message::{Request, Response};

/// Executes a permitted [`Request`], producing a [`Response`].
///
/// Implementors own the actual hardware / native-code resources (USB, serial,
/// DLLs). The bridge calls this only after a permission check has passed.
pub trait RequestExecutor {
    /// Perform `request` and return its result.
    fn execute(&mut self, request: Request) -> Response;
}
