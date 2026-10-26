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

/// Append a step marker to `sentinel-bridge-debug.log` (in the CWD) when the
/// `SENTINEL_BRIDGE` env var is set. Flushed each call so that if the NEXT native
/// call access-violates, the last logged line names the last step that succeeded —
/// pinpointing which JavaScriptCore call crashes on an AppCore overlay view.
fn debug_step(step: &str) {
    if std::env::var_os("SENTINEL_BRIDGE").is_none() {
        return;
    }
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("sentinel-bridge-debug.log")
    {
        let _ = writeln!(f, "install_native: {step}");
        let _ = f.flush();
    }
}

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
    debug_step("start");
    {
        debug_step("before lock_js_context");
        let ctx = view.lock_js_context();
        debug_step("before global_object");
        let global = ctx.global_object();

        debug_step("before new_function_with_callback");
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

        debug_step("before set_property");
        global
            .set_property("__sentinelNativeCall", &callback, JSPropertyAttributes::default())
            .map_err(|e| Error::Engine(format!("set_property failed: {e:?}")))?;
        debug_step("after set_property (dropping ctx guard)");
    }

    debug_step("before evaluate_script(shim)");
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
    let outcome = view
        .evaluate_script(&shim)
        .map_err(|e| Error::Engine(format!("bridge shim eval failed: {e:?}")))?;
    debug_step("after evaluate_script(shim)");
    match outcome
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

// ---------------------------------------------------------------------------
// Async plugin bridge (`window.share`): the callId -> __sentinelResolve round
// trip, with streaming. Plugin calls may open a modal window and finish much
// later, so unlike `window.native` (synchronous) they MUST NOT block the JS
// callback. The page gets a Promise; the native `__sentinelEnqueue` hands
// (callId, request) to the host, which later settles it with [`resolve_call`].
//
// Streaming: the reference .NET bridge could only return a single JSON envelope,
// so plugins that produce a stream (idbox chip-read status events, live preview
// frames, progress) had no path. Here `share.RunPluginStream(id, req, onChunk)`
// delivers any number of intermediate chunks via [`emit_chunk`]
// (`__sentinelEmit`) before the final [`resolve_call`] — one generic mechanism
// covering both status/progress events and binary/preview chunks.
// ---------------------------------------------------------------------------

/// One async plugin call enqueued by page JS, awaiting a host result.
#[derive(Debug, Clone, PartialEq)]
pub struct PendingCall {
    /// The JS-assigned id; echo it back to [`resolve_call`] to settle the Promise.
    pub call_id: f64,
    /// The request payload (JSON string, shim-defined: `{kind, id?, request?}`).
    pub request: String,
    /// The origin of the document that made the call, captured at enqueue time.
    /// The host gates each call by this origin and must not deliver a result
    /// across a navigation (drop the call if the active origin no longer matches).
    pub origin: Origin,
}

/// Queue of pending async calls, shared between the JS callback and the host loop.
pub type PendingCalls = Arc<Mutex<Vec<PendingCall>>>;

/// Format a JS-origin call id back into a JS numeric literal (whole ids without a
/// trailing `.0`, matching how the shim keys its pending map).
fn format_call_id(id: f64) -> String {
    if id.is_finite() && id.fract() == 0.0 {
        format!("{}", id as i64)
    } else {
        format!("{id}")
    }
}

/// Install the async plugin bridge (`window.share`) into `view` for a document of
/// `origin`. Page JS calls `window.share.RunPlugin(id, req)` / `GetPlugins()`,
/// each returning a Promise; the native `__sentinelEnqueue(callId, request)`
/// stamps the call with `origin`, pushes it onto `pending`, and returns
/// immediately. The host drains `pending`, GATES each call by its origin, does the
/// work (possibly a modal window), and settles each Promise with [`resolve_call`].
///
/// # Errors
/// [`Error::Engine`] if the native entry point cannot be installed or the JS shim
/// fails to evaluate.
pub fn install_async_bridge(view: &View, origin: Origin, pending: PendingCalls) -> Result<()> {
    {
        let ctx = view.lock_js_context();
        let global = ctx.global_object();
        // __sentinelEnqueue(callId: number, request: string) -> undefined.
        let callback = JSObject::new_function_with_callback(&ctx, move |ctx, _this, args| {
            let call_id = args.first().and_then(|v| v.as_number().ok()).unwrap_or(0.0);
            let request = args
                .get(1)
                .and_then(|v| v.as_string().ok())
                .map(|s| s.to_string())
                .unwrap_or_default();
            pending.lock().unwrap_or_else(|p| p.into_inner()).push(PendingCall {
                call_id,
                request,
                origin: origin.clone(),
            });
            Ok(JSValue::new_undefined(ctx))
        });
        global
            .set_property("__sentinelEnqueue", &callback, JSPropertyAttributes::default())
            .map_err(|e| Error::Engine(format!("set __sentinelEnqueue failed: {e:?}")))?;
    }

    let shim = "(function(){\
           window.__sentinelPending = window.__sentinelPending || {};\
           window.__sentinelNextId = window.__sentinelNextId || 1;\
           function enqueue(request, onChunk){\
             return new Promise(function(resolve){\
               var id = window.__sentinelNextId++;\
               window.__sentinelPending[id] = { resolve: resolve, onChunk: onChunk || null };\
               window.__sentinelEnqueue(id, request);\
             });\
           }\
           window.__sentinelEmit = function(id, json){\
             var e = window.__sentinelPending[id];\
             if (e && e.onChunk){ try { e.onChunk(json); } catch (err) {} }\
           };\
           window.__sentinelResolve = function(id, json){\
             var e = window.__sentinelPending[id];\
             if (e){ delete window.__sentinelPending[id]; e.resolve(json); }\
           };\
           function reqStr(r){ return (r==null?'':String(r)); }\
           window.share = {\
             RunPlugin: function(id, request){\
               return enqueue(JSON.stringify({kind:'run', id:id, request:reqStr(request)}));\
             },\
             RunPluginStream: function(id, request, onChunk){\
               return enqueue(JSON.stringify({kind:'run', id:id, request:reqStr(request), stream:true}), onChunk);\
             },\
             GetPlugins: function(){ return enqueue(JSON.stringify({kind:'list'})); }\
           };\
         })();";
    match view
        .evaluate_script(shim)
        .map_err(|e| Error::Engine(format!("async shim eval failed: {e:?}")))?
    {
        Ok(_) => Ok(()),
        Err(js_exception) => Err(Error::Engine(format!("async shim threw: {js_exception}"))),
    }
}

/// Settle the pending async call `call_id` by resolving its JS Promise with
/// `result_json` (passed to page JS as a string — typically a `PluginResult`
/// envelope or a `GetPlugins` descriptor array). Call from the host main loop.
///
/// # Errors
/// [`Error::Engine`] if the resolve eval fails or throws.
pub fn resolve_call(view: &View, call_id: f64, result_json: &str) -> Result<()> {
    let arg = serde_json::to_string(result_json).unwrap_or_else(|_| "\"\"".into());
    let js = format!(
        "window.__sentinelResolve && window.__sentinelResolve({}, {})",
        format_call_id(call_id),
        arg
    );
    match view
        .evaluate_script(&js)
        .map_err(|e| Error::Engine(format!("resolve eval failed: {e:?}")))?
    {
        Ok(_) => Ok(()),
        Err(js_exception) => Err(Error::Engine(format!("resolve threw: {js_exception}"))),
    }
}

/// Deliver one streamed `chunk_json` to the pending call `call_id` (fires its JS
/// `onChunk` callback registered via `share.RunPluginStream`). Use any number of
/// times between enqueue and [`resolve_call`], for plugins that emit status /
/// progress / preview chunks before completing.
///
/// # Errors
/// [`Error::Engine`] if the emit eval fails or throws.
pub fn emit_chunk(view: &View, call_id: f64, chunk_json: &str) -> Result<()> {
    let arg = serde_json::to_string(chunk_json).unwrap_or_else(|_| "\"\"".into());
    let js = format!(
        "window.__sentinelEmit && window.__sentinelEmit({}, {})",
        format_call_id(call_id),
        arg
    );
    match view
        .evaluate_script(&js)
        .map_err(|e| Error::Engine(format!("emit eval failed: {e:?}")))?
    {
        Ok(_) => Ok(()),
        Err(js_exception) => Err(Error::Engine(format!("emit threw: {js_exception}"))),
    }
}

/// Headless end-to-end proof of the async bridge: install it, have page JS call
/// `window.share.RunPlugin('Echo', request)`, drain the pending queue in Rust,
/// settle each Promise by echoing the enqueued request back, and return what the
/// JS Promise observed. Proves the callId -> `__sentinelResolve` round trip.
///
/// # Errors
/// [`Error::Engine`] on engine/eval failure.
pub fn prove_async_roundtrip(request: &str) -> Result<String> {
    let renderer = crate::headless::init_renderer()?;
    let view = crate::headless::create_cpu_view(&renderer, 16, 16)?;
    view.load_html("<html><body></body></html>")
        .map_err(|e| Error::Engine(format!("load_html: {e:?}")))?;
    crate::headless::pump_until_loaded(&renderer, &view);

    let pending: PendingCalls = Arc::new(Mutex::new(Vec::new()));
    let origin = Origin::parse("https://test.example").expect("valid test origin");
    install_async_bridge(&view, origin, pending.clone())?;

    // Kick off an async call; stash the resolved value on window.__asyncResult.
    // The Promise executor (and thus __sentinelEnqueue) runs synchronously here.
    let kickoff = format!(
        "window.__asyncResult = 'PENDING'; \
         window.share.RunPlugin('Echo', {}).then(function(r){{ window.__asyncResult = r; }});",
        serde_json::to_string(request).unwrap_or_else(|_| "\"\"".into())
    );
    if let Err(js) = view
        .evaluate_script(&kickoff)
        .map_err(|e| Error::Engine(format!("kickoff eval failed: {e:?}")))?
    {
        return Err(Error::Engine(format!("kickoff threw: {js}")));
    }

    // The host would dispatch to the plugin registry here; this proof echoes the
    // enqueued request straight back as the resolved value.
    let calls = std::mem::take(&mut *pending.lock().unwrap_or_else(|p| p.into_inner()));
    for c in calls {
        resolve_call(&view, c.call_id, &c.request)?;
    }
    // Flush JS microtasks so the `.then` callback runs.
    crate::headless::pump_until_loaded(&renderer, &view);

    match view
        .evaluate_script("window.__asyncResult")
        .map_err(|e| Error::Engine(format!("result read failed: {e:?}")))?
    {
        Ok(s) => Ok(s),
        Err(js_exception) => Err(Error::Engine(format!("result read threw: {js_exception}"))),
    }
}

/// Headless proof of the streaming path: `share.RunPluginStream` collects each
/// `__sentinelEmit` chunk via its `onChunk` callback and the final
/// `__sentinelResolve` value. This emits two chunks then completes, and returns a
/// JSON summary `{"chunks":["status-1","status-2"],"final":"done"}` — proving a
/// plugin can stream intermediate events before its final result.
///
/// # Errors
/// [`Error::Engine`] on engine/eval failure.
pub fn prove_async_stream_roundtrip() -> Result<String> {
    let renderer = crate::headless::init_renderer()?;
    let view = crate::headless::create_cpu_view(&renderer, 16, 16)?;
    view.load_html("<html><body></body></html>")
        .map_err(|e| Error::Engine(format!("load_html: {e:?}")))?;
    crate::headless::pump_until_loaded(&renderer, &view);

    let pending: PendingCalls = Arc::new(Mutex::new(Vec::new()));
    let origin = Origin::parse("https://test.example").expect("valid test origin");
    install_async_bridge(&view, origin, pending.clone())?;

    let kickoff = "window.__chunks = []; window.__asyncResult = 'PENDING'; \
         window.share.RunPluginStream('Reader', 'go', function(c){ window.__chunks.push(c); })\
           .then(function(r){ window.__asyncResult = r; });";
    if let Err(js) = view
        .evaluate_script(kickoff)
        .map_err(|e| Error::Engine(format!("stream kickoff eval failed: {e:?}")))?
    {
        return Err(Error::Engine(format!("stream kickoff threw: {js}")));
    }

    let calls = std::mem::take(&mut *pending.lock().unwrap_or_else(|p| p.into_inner()));
    for c in calls {
        emit_chunk(&view, c.call_id, "status-1")?;
        emit_chunk(&view, c.call_id, "status-2")?;
        resolve_call(&view, c.call_id, "done")?;
    }
    crate::headless::pump_until_loaded(&renderer, &view);

    match view
        .evaluate_script("JSON.stringify({chunks: window.__chunks, final: window.__asyncResult})")
        .map_err(|e| Error::Engine(format!("stream result read failed: {e:?}")))?
    {
        Ok(s) => Ok(s),
        Err(js_exception) => Err(Error::Engine(format!("stream result read threw: {js_exception}"))),
    }
}
