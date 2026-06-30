//! Installs the `window.native` bridge into a page's JavaScriptCore context and
//! routes each call through the permission-gated dispatcher.
//!
//! Mechanism (verified headlessly by [`prove_native_dispatch`]):
//! 1. A single Rust-backed entry point `window.__sentinelNativeCall(jsonRequest)`
//!    is installed on the page's global object. It parses the JSON request, routes
//!    it through a shared [`BridgeRouter`] via [`sentinel_bridge::dispatch_json`],
//!    and returns a JSON outcome string.
//! 2. A JS shim builds the friendly `window.native` namespace (e.g.
//!    `window.native.usb.list()`) on top of that entry point.
//!
//! Policy lives entirely in `sentinel-bridge`; this module only marshals strings
//! across the JS boundary. Compiled only with the `ultralight` feature.

use std::sync::{Arc, Mutex};

use sentinel_bridge::{dispatch_json, BridgeRouter};
use sentinel_core::{Error, Result};
use sentinel_permissions::{Origin, PermissionBroker};
use ul_next::javascript::{JSObject, JSPropertyAttributes, JSValue};
use ul_next::view::View;

/// Version string exposed to pages as `window.native.version`.
pub const BRIDGE_VERSION: &str = "0.1";

/// A router shared between the page callback and the rest of the browser (so the
/// consent flow can record grants and have them take effect on the next call).
pub type SharedRouter<B> = Arc<Mutex<BridgeRouter<B>>>;

/// Install the `window.native` bridge into `view`'s JS context, routing calls for
/// `origin` through `router`.
///
/// # Errors
/// [`Error::Engine`] if the native entry point cannot be installed or the JS shim
/// fails to evaluate.
pub fn install_native<B: PermissionBroker + 'static>(
    view: &View,
    router: SharedRouter<B>,
    origin: Origin,
) -> Result<()> {
    {
        let ctx = view.lock_js_context();
        let global = ctx.global_object();

        // The native entry point: (jsonRequest: string) -> jsonOutcome: string.
        let callback = JSObject::new_function_with_callback(&ctx, move |ctx, _this, args| {
            let request_json = args
                .first()
                .and_then(|v| v.as_string().ok())
                .map(|s| s.to_string());
            let outcome = match request_json {
                Some(json) => {
                    let guard = router.lock().unwrap_or_else(|p| p.into_inner());
                    dispatch_json(&guard, &origin, &json)
                }
                None => {
                    r#"{"status":"bad_request","message":"missing request string"}"#.to_string()
                }
            };
            Ok(JSValue::new_string(ctx, &outcome))
        });

        global
            .set_property("__sentinelNativeCall", &callback, JSPropertyAttributes::default())
            .map_err(|e| Error::Engine(format!("set_property failed: {e:?}")))?;
    }

    // Friendly namespace built on the native entry point. Each method packages an
    // IPC request as JSON, calls into Rust, and parses the JSON outcome.
    let shim = format!(
        "(function(){{\
           const call = window.__sentinelNativeCall;\
           function op(o){{ return JSON.parse(call(JSON.stringify(o))); }}\
           window.native = {{\
             version: '{version}',\
             usb:    {{ list: () => op({{op:'usb_list'}}) }},\
             serial: {{ list: () => op({{op:'serial_list'}}) }},\
             dll:    {{ load: (path, sha256) => op({{op:'dll_load', path, sha256}}) }}\
           }};\
         }})();",
        version = BRIDGE_VERSION
    );
    match view
        .evaluate_script(&shim)
        .map_err(|e| Error::Engine(format!("bridge shim eval failed: {e:?}")))?
    {
        Ok(_) => Ok(()),
        Err(js_exception) => Err(Error::Engine(format!("bridge shim threw: {js_exception}"))),
    }
}

/// Headless, self-contained proof that a page call routes through the permission
/// gate: with no grant recorded, `window.native.usb.list()` must report
/// `needs_consent` for the `usb` capability.
///
/// Returns the string `"<status>:<capability>"` (expected `"needs_consent:usb"`),
/// which can only be produced if JS → Rust dispatch → router → JSON all executed.
///
/// # Errors
/// [`Error::Engine`] if engine setup, injection, or evaluation fails.
pub fn prove_native_dispatch() -> Result<String> {
    use sentinel_permissions::InMemoryBroker;

    let renderer = crate::headless::init_renderer()?;
    let view = crate::headless::create_cpu_view(&renderer, 16, 16)?;
    view.load_html("<html><body></body></html>")
        .map_err(|e| Error::Engine(format!("load_html: {e:?}")))?;
    crate::headless::pump_until_loaded(&renderer, &view);

    let router = Arc::new(Mutex::new(BridgeRouter::new(InMemoryBroker::new())));
    let origin = Origin::parse("https://test.example")
        .map_err(|e| Error::Engine(format!("origin parse: {e}")))?;
    install_native(&view, router, origin)?;

    let probe = "(function(){ const r = window.native.usb.list(); return r.status + ':' + r.capability; })()";
    match view
        .evaluate_script(probe)
        .map_err(|e| Error::Engine(format!("dispatch probe eval failed: {e:?}")))?
    {
        Ok(s) => Ok(s),
        Err(js_exception) => Err(Error::Engine(format!("dispatch probe threw: {js_exception}"))),
    }
}
