//! Installs the `window.native` bridge into a page's JavaScriptCore context.
//!
//! Mechanism (verified headlessly by [`prove_native_bridge`]):
//! 1. A single Rust-backed entry point `window.__sentinelNativeCall` is installed
//!    on the page's global object via JavaScriptCore.
//! 2. A small JS shim builds the friendly `window.native` namespace on top of it.
//!
//! Routing each call through the permission broker / host IPC builds on this proven
//! entry point — this module currently installs the entry point and a version
//! probe; the request-dispatch wiring (to `sentinel-bridge`) is the next increment.
//!
//! Compiled only when the `ultralight` feature is enabled.

use sentinel_core::{Error, Result};
use ul_next::javascript::{JSObject, JSPropertyAttributes, JSValue};
use ul_next::view::View;

/// Version string exposed to pages as `window.native.version`.
pub const BRIDGE_VERSION: &str = "0.1";

/// Install the `window.native` bridge into `view`'s JavaScript context.
///
/// Idempotent per page load; intended to be called once the page's JS context
/// exists (e.g. on DOM-ready). Safe to call from the thread driving the view.
///
/// # Errors
/// [`Error::Engine`] if the JS shim fails to evaluate or throws.
pub fn install_native(view: &View) -> Result<()> {
    // Install the native entry point, then release the context lock before
    // evaluating the JS shim (which re-locks internally — the lock is recursive,
    // but we keep the borrow scope tight).
    {
        let ctx = view.lock_js_context();
        let global = ctx.global_object();
        let callback = JSObject::new_function_with_callback(&ctx, |ctx, _this, _args| {
            // For now the native entry point just reports the bridge version. The
            // real dispatcher will parse the first argument as a JSON request and
            // route it through the permission broker.
            Ok(JSValue::new_string(ctx, BRIDGE_VERSION))
        });
        global
            .set_property("__sentinelNativeCall", &callback, JSPropertyAttributes::default())
            .map_err(|e| Error::Engine(format!("set_property failed: {e:?}")))?;
    }

    // Friendly namespace built on the native entry point.
    let shim = "(function(){ \
                 window.native = { version: window.__sentinelNativeCall() }; \
                 })();";
    match view
        .evaluate_script(shim)
        .map_err(|e| Error::Engine(format!("bridge shim eval failed: {e:?}")))?
    {
        Ok(_) => Ok(()),
        Err(js_exception) => Err(Error::Engine(format!("bridge shim threw: {js_exception}"))),
    }
}

/// Headless, self-contained verification that a Rust callback is reachable from
/// page JavaScript: sets up a CPU view, installs the bridge, and reads back
/// `window.native.version` — which can only equal [`BRIDGE_VERSION`] if the JS→Rust
/// call actually executed.
///
/// # Errors
/// [`Error::Engine`] if engine setup, injection, or evaluation fails.
pub fn prove_native_bridge() -> Result<String> {
    let renderer = crate::headless::init_renderer()?;
    let view = crate::headless::create_cpu_view(&renderer, 16, 16)?;

    view.load_html("<html><body></body></html>")
        .map_err(|e| Error::Engine(format!("load_html: {e:?}")))?;
    crate::headless::pump_until_loaded(&renderer, &view);

    install_native(&view)?;

    match view
        .evaluate_script("window.native.version")
        .map_err(|e| Error::Engine(format!("version probe eval failed: {e:?}")))?
    {
        Ok(version) => Ok(version),
        Err(js_exception) => Err(Error::Engine(format!("version probe threw: {js_exception}"))),
    }
}
