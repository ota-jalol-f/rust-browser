//! Installs the `window.native` bridge into a page's JavaScriptCore context and
//! routes each call through the permission gate, executing permitted requests on
//! a host [`RequestExecutor`].
//!
//! Flow (verified headlessly by [`run_bridge_probe`]):
//! `window.native.usb.list()` → JS shim → `window.__sentinelNativeCall(json)` →
//! [`sentinel_bridge::dispatch_json`] (router checks permission; on grant calls the
//! executor) → JSON [`sentinel_bridge::DispatchReport`] → parsed back in JS.
//!
//! Policy lives in `sentinel-bridge`; the hosts (device/DLL) live behind the
//! `RequestExecutor` seam. This module only marshals strings across the JS
//! boundary. Compiled only with the `ultralight` feature.

use std::sync::{Arc, Mutex};

use sentinel_bridge::{dispatch_json, BridgeRouter};
use sentinel_core::{Error, Result};
use sentinel_ipc::RequestExecutor;
use sentinel_permissions::{Origin, PermissionBroker};
use ul_next::javascript::{JSObject, JSPropertyAttributes, JSValue};
use ul_next::view::View;

/// Version string exposed to pages as `window.native.version`.
pub const BRIDGE_VERSION: &str = "0.1";

/// A router shared between the page callback and the consent flow.
pub type SharedRouter<B> = Arc<Mutex<BridgeRouter<B>>>;

/// A host executor shared between the page callback and the rest of the browser.
pub type SharedExecutor<E> = Arc<Mutex<E>>;

/// Install the `window.native` bridge into `view`'s JS context, routing calls for
/// `origin` through `router` and executing permitted ones on `executor`.
///
/// # Errors
/// [`Error::Engine`] if the native entry point cannot be installed or the JS shim
/// fails to evaluate.
pub fn install_native<B, E>(
    view: &View,
    router: SharedRouter<B>,
    executor: SharedExecutor<E>,
    origin: Origin,
) -> Result<()>
where
    B: PermissionBroker + 'static,
    E: RequestExecutor + 'static,
{
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
                    let router_guard = router.lock().unwrap_or_else(|p| p.into_inner());
                    let mut exec_guard = executor.lock().unwrap_or_else(|p| p.into_inner());
                    dispatch_json(&router_guard, &mut *exec_guard, &origin, &json)
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

    // Friendly namespace built on the native entry point.
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
           if (typeof window.onSentinelReady === 'function') {{ try {{ window.onSentinelReady(); }} catch (e) {{}} }}\
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

/// Reproduce the live-GUI install path headlessly: install `window.native` from
/// inside a `set_dom_ready_callback` (the same hook the shell uses), then verify
/// it took effect. This tells us whether the shell's DOM-ready install crash is
/// reproducible with the CPU renderer (here) or specific to AppCore/GPU.
///
/// Returns `window.native.version` (expected [`BRIDGE_VERSION`]).
///
/// # Errors
/// [`Error::Engine`] on engine/eval failure.
pub fn prove_dom_ready_install<B, E>(
    router: SharedRouter<B>,
    executor: SharedExecutor<E>,
    origin: Origin,
) -> Result<String>
where
    B: PermissionBroker + 'static,
    E: RequestExecutor + 'static,
{
    let renderer = crate::headless::init_renderer()?;
    let view = crate::headless::create_cpu_view(&renderer, 16, 16)?;

    {
        let router = router.clone();
        let executor = executor.clone();
        view.set_dom_ready_callback(move |view, _frame_id, is_main_frame, _url| {
            if is_main_frame {
                let _ = install_native(view, router.clone(), executor.clone(), origin.clone());
            }
        });
    }

    view.load_html("<html><body></body></html>")
        .map_err(|e| Error::Engine(format!("load_html: {e:?}")))?;
    crate::headless::pump_until_loaded(&renderer, &view);

    match view
        .evaluate_script("window.native ? window.native.version : 'NO_NATIVE'")
        .map_err(|e| Error::Engine(format!("version probe eval failed: {e:?}")))?
    {
        Ok(s) => Ok(s),
        Err(js_exception) => Err(Error::Engine(format!("version probe threw: {js_exception}"))),
    }
}

/// Headless, self-contained harness: set up a CPU view, install the bridge with the
/// given `router`/`executor`/`origin`, evaluate `probe_js`, and return its string
/// result. Used by tests to drive the full JS → policy → executor → JSON path.
///
/// # Errors
/// [`Error::Engine`] if engine setup, injection, or evaluation fails.
pub fn run_bridge_probe<B, E>(
    router: SharedRouter<B>,
    executor: SharedExecutor<E>,
    origin: Origin,
    probe_js: &str,
) -> Result<String>
where
    B: PermissionBroker + 'static,
    E: RequestExecutor + 'static,
{
    let renderer = crate::headless::init_renderer()?;
    let view = crate::headless::create_cpu_view(&renderer, 16, 16)?;
    view.load_html("<html><body></body></html>")
        .map_err(|e| Error::Engine(format!("load_html: {e:?}")))?;
    crate::headless::pump_until_loaded(&renderer, &view);

    install_native(&view, router, executor, origin)?;

    match view
        .evaluate_script(probe_js)
        .map_err(|e| Error::Engine(format!("probe eval failed: {e:?}")))?
    {
        Ok(s) => Ok(s),
        Err(js_exception) => Err(Error::Engine(format!("probe threw: {js_exception}"))),
    }
}
