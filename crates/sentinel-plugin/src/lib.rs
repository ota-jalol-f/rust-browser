//! `sentinel-plugin`: the Rust plugin contract and in-process registry.
//!
//! This is the Rust replacement for the reference browser's `IUiPlugin` /
//! `IBrowserExtension` plugin model. A [`Plugin`] reports a [`PluginDescriptor`]
//! and answers one call at a time via [`Plugin::execute`], returning the
//! `{data, success, error_message}` [`PluginResult`] envelope — byte-compatible
//! with what the pv-next frontend expects from `share.RunPlugin` / host objects,
//! so the web side is unchanged.
//!
//! [`PluginRegistry`] owns the loaded plugins and is the core the `share` JS
//! bridge dispatches through: [`PluginRegistry::descriptors`] backs
//! `share.GetPlugins()` and [`PluginRegistry::dispatch`] backs
//! `share.RunPlugin(id, json)`. It is pure logic with no engine/UI dependency so
//! it is fully unit-tested in isolation; the async bridge and the modal UI host
//! are layered on top (see the Phase 2 plan).

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod registry;
mod result;

pub use registry::{
    HostMethod, HostObjectSpec, Plugin, PluginDescriptor, PluginError, PluginKind, PluginRegistry,
    PluginStream, PluginUi, StreamStep, UiStep,
};
pub use result::PluginResult;
