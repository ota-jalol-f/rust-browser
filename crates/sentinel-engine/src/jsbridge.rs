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
           /* Expose the Promise-returning enqueue so per-plugin host objects\
              (installed separately, see install_host_objects) route through the\
              same callId -> __sentinelResolve machinery, origin stamp and gate. */\
           window.__sentinelRun = enqueue;\
           window.share = {\
             RunPlugin: function(id, request){\
               return enqueue(JSON.stringify({kind:'run', id:id, request:reqStr(request)}));\
             },\
             RunPluginStream: function(id, request, onChunk){\
               return enqueue(JSON.stringify({kind:'run', id:id, request:reqStr(request), stream:true}), onChunk);\
             },\
             RunPluginUi: function(id, request){\
               return enqueue(JSON.stringify({kind:'run', id:id, request:reqStr(request), ui:true}));\
             },\
             GetPlugins: function(){ return enqueue(JSON.stringify({kind:'list'})); }\
           };\
           /* pv-next compatibility shim: it reaches plugins via\
              window.chrome.webview.hostObjects.share.RunPlugin/GetPlugins (the\
              reference WebView2 host-object path), so alias it to window.share.\
              Methods return Promises, matching what pv-next `await`s. */\
           window.chrome = window.chrome || {};\
           window.chrome.webview = window.chrome.webview || {};\
           /* Make chrome.webview an EventTarget for the deferred-result channel\
              (share.InvokeDeferred -> 'mybrowser:deferred' message). pv-next's\
              bridge attaches its listener to chrome.webview exactly once and\
              remembers the object, so AUGMENT in place and add the hooks only when\
              absent — replacing the object or the listener store would orphan\
              pv-next's listener and hang every deferred call. __deliver dispatches\
              {data:<object>} because the listener rejects a non-object data. */\
           var __cw = window.chrome.webview;\
           if (typeof __cw.addEventListener !== 'function') {\
             __cw.__sentinelMsgListeners = [];\
             __cw.addEventListener = function(type, fn){\
               if (type === 'message' && typeof fn === 'function') __cw.__sentinelMsgListeners.push(fn);\
             };\
             __cw.removeEventListener = function(type, fn){\
               if (type !== 'message') return;\
               var a = __cw.__sentinelMsgListeners, i = a.indexOf(fn);\
               if (i >= 0) a.splice(i, 1);\
             };\
             __cw.__deliver = function(data){\
               (__cw.__sentinelMsgListeners || []).slice().forEach(function(fn){\
                 try { fn({ data: data }); } catch (e) {}\
               });\
             };\
           }\
           window.chrome.webview.hostObjects = window.chrome.webview.hostObjects || {};\
           window.chrome.webview.hostObjects.share = window.share;\
         })();";
    match view
        .evaluate_script(shim)
        .map_err(|e| Error::Engine(format!("async shim eval failed: {e:?}")))?
    {
        Ok(_) => Ok(()),
        Err(js_exception) => Err(Error::Engine(format!("async shim threw: {js_exception}"))),
    }
}

/// Install per-plugin JS host objects (`chrome.webview.hostObjects.<name>`) from a
/// spec JSON array (`[{"name","id","methods":[{"js","ui"}]}]`, built from
/// [`sentinel_plugin::PluginRegistry::host_object_specs`]). Call AFTER
/// [`install_async_bridge`] on the same view — each method forwards through the
/// `window.__sentinelRun` it exposes, so the origin stamp, the capability gate and
/// the per-tab queue all still apply.
///
/// This is the surface pv-next actually uses: its services call
/// `hostObjects.idbox.ReadChipUid()`, `hostObjects.system.ping()`, etc. Each
/// generated method sends `{kind:'run', id, request:'{"method":"<js>","args":[…]}',
/// ui:<ui>}` and resolves the page Promise either with the `PluginResult` envelope
/// (the default — the idbox/fingerprints/photo/signpad services unwrap it with
/// `success ? data : throw`) or, when the host method is [`HostMethod::raw`], with
/// the raw JSON-decoded `data` (for services such as `system` that use the value
/// verbatim).
///
/// This also installs `share.InvokeDeferred(callId, objectName, method, argsJson)`,
/// the deferred/modal call path the current pv-next bridge uses for every method in
/// its `a8` allowlist: it returns `{"accepted":true}` synchronously (so the
/// WebView2-style host-object call returns before any modal window opens — a modal
/// loop inside that call crashes the reference WebView2 host), runs the op off-call
/// through the same enqueue/pump machinery, and posts the outcome back as a
/// `chrome.webview` `mybrowser:deferred` message keyed by `callId`. The generic
/// `share.RunPlugin(id, payload)` arrives through this path too and posts the
/// plugin's raw output (the reference `RunPlugin` returns the plugin's own string;
/// its pv-next consumer does not unwrap an envelope).
///
/// `spec_json` must be a trusted JSON array produced by the host (it is spliced
/// into the shim as a literal). An overlay view must NOT get host objects.
///
/// # Errors
/// [`Error::Engine`] if the shim fails to evaluate or throws.
pub fn install_host_objects(view: &View, spec_json: &str) -> Result<()> {
    // Template with a splice token rather than format! (the shim is dense with
    // braces); the token is distinctive and cannot occur in the JSON.
    let template = "(function(){\
        var spec; try { spec = __SENTINEL_HOST_SPEC__; } catch (e) { spec = []; }\
        if (!Array.isArray(spec)) spec = [];\
        var run = window.__sentinelRun;\
        if (typeof run !== 'function') return;\
        window.chrome = window.chrome || {};\
        window.chrome.webview = window.chrome.webview || {};\
        window.chrome.webview.hostObjects = window.chrome.webview.hostObjects || {};\
        var cw = window.chrome.webview;\
        /* Unwrap a PluginResult envelope string to its raw, JSON-decoded data;\
           throws on failure. Used by raw host methods. */\
        function unwrapRaw(s){\
          var env; try { env = JSON.parse(s); } catch(e){ return s; }\
          if (env && typeof env === 'object' && 'success' in env){\
            if (!env.success) throw new Error(env.error_message || 'plugin error');\
            /* A raw method MUST JSON-encode its return in data; surface a contract\
               violation loudly rather than silently coercing a bare string. */\
            try { return JSON.parse(env.data); } catch(e){ throw new Error('raw host method returned non-JSON data'); }\
          }\
          return env;\
        }\
        var byName = {};\
        spec.forEach(function(p){\
          var methods = {};\
          var obj = {};\
          (p.methods || []).forEach(function(m){\
            methods[m.js] = { ui: m.ui === true, raw: m.raw === true };\
            obj[m.js] = function(){\
              var args = Array.prototype.slice.call(arguments);\
              var req = JSON.stringify({method: m.js, args: args});\
              return run(JSON.stringify({kind:'run', id:p.id, request:req, ui:m.ui===true}))\
                .then(function(s){\
                  if (m.raw === true) return unwrapRaw(s);\
                  try { return JSON.parse(s); } catch(e){ return s; }\
                });\
            };\
          });\
          byName[p.name] = { id: p.id, methods: methods };\
          cw.hostObjects[p.name] = obj;\
        });\
        /* share.InvokeDeferred(callId, objectName, method, argsJson): the deferred\
           (modal) call path pv-next uses for every a8 method. Returns\
           {accepted:true} synchronously (the host-object call ends before any\
           window opens), runs the op off-call via the existing enqueue, and\
           delivers the outcome as a chrome.webview 'mybrowser:deferred' message\
           keyed by callId. ('share','RunPlugin') is the generic RunPlugin(id,\
           payload); its result is posted RAW because the reference RunPlugin\
           returns the plugin's own output and pv-next's RunPlugin consumer does\
           not unwrap an envelope. Other methods post per the host method's raw\
           flag: the envelope by default, or the raw data when raw. */\
        if (window.share && typeof window.share.InvokeDeferred !== 'function'){\
          window.share.InvokeDeferred = function(callId, objectName, method, argsJson){\
            var args = []; try { args = JSON.parse(argsJson); } catch(e){}\
            if (!Array.isArray(args)) args = [];\
            var reqObj, postRaw;\
            if (objectName === 'share' && method === 'RunPlugin'){\
              reqObj = { kind:'run', id: args[0], request: (args[1] != null ? String(args[1]) : '') };\
              postRaw = true;\
            } else {\
              var e = byName[objectName];\
              if (!e) return JSON.stringify({ accepted:false, error: \"host object '\" + objectName + \"' not found\" });\
              var m = e.methods[method];\
              if (!m) return JSON.stringify({ accepted:false, error: \"method '\" + objectName + '.' + method + \"' not found\" });\
              reqObj = { kind:'run', id: e.id, request: JSON.stringify({ method: method, args: args }), ui: m.ui };\
              postRaw = m.raw;\
            }\
            function deliver(msg){ if (cw && typeof cw.__deliver === 'function') cw.__deliver(msg); }\
            run(JSON.stringify(reqObj)).then(function(s){\
              var result;\
              if (postRaw){\
                var env; try { env = JSON.parse(s); } catch(e2){ env = null; }\
                if (env && typeof env === 'object' && 'success' in env){\
                  /* RunPlugin is the only deferred raw path; the reference RunPlugin\
                     never throws to JS — it posts {error} (ok:true) — so mirror that. */\
                  if (!env.success){ deliver({ type:'mybrowser:deferred', callId: callId, ok:true, result: JSON.stringify({ error: env.error_message || 'plugin error' }) }); return; }\
                  result = env.data;\
                } else { result = s; }\
              } else { result = s; }\
              deliver({ type:'mybrowser:deferred', callId: callId, ok:true, result: result });\
            }).catch(function(err){\
              deliver({ type:'mybrowser:deferred', callId: callId, ok:false, error: String(err) });\
            });\
            return JSON.stringify({ accepted:true });\
          };\
        }\
      })();";
    let shim = template.replace("__SENTINEL_HOST_SPEC__", spec_json);
    match view
        .evaluate_script(&shim)
        .map_err(|e| Error::Engine(format!("host-objects shim eval failed: {e:?}")))?
    {
        Ok(_) => Ok(()),
        Err(js_exception) => Err(Error::Engine(format!("host-objects shim threw: {js_exception}"))),
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

/// Headless proof that pv-next's exact plugin path —
/// `window.chrome.webview.hostObjects.share.RunPlugin(...)` — reaches the host
/// bridge: install it, call via that path, drain + echo-resolve, and report the
/// surface plus the resolved value as
/// `{"hasHostObjects":bool,"hasShare":bool,"result":"..."}`.
///
/// # Errors
/// [`Error::Engine`] on engine/eval failure.
pub fn prove_hostobjects_shim() -> Result<String> {
    let renderer = crate::headless::init_renderer()?;
    let view = crate::headless::create_cpu_view(&renderer, 16, 16)?;
    view.load_html("<html><body></body></html>")
        .map_err(|e| Error::Engine(format!("load_html: {e:?}")))?;
    crate::headless::pump_until_loaded(&renderer, &view);

    let pending: PendingCalls = Arc::new(Mutex::new(Vec::new()));
    let origin = Origin::parse("https://test.example").expect("valid test origin");
    install_async_bridge(&view, origin, pending.clone())?;

    let kickoff = "window.__r = 'PENDING';\
        window.chrome.webview.hostObjects.share.RunPlugin('Echo', 'hi')\
          .then(function(r){ window.__r = r; });";
    if let Err(js) = view
        .evaluate_script(kickoff)
        .map_err(|e| Error::Engine(format!("hostObjects kickoff failed: {e:?}")))?
    {
        return Err(Error::Engine(format!("hostObjects kickoff threw: {js}")));
    }
    let calls = std::mem::take(&mut *pending.lock().unwrap_or_else(|p| p.into_inner()));
    for c in calls {
        resolve_call(&view, c.call_id, &c.request)?;
    }
    crate::headless::pump_until_loaded(&renderer, &view);

    let probe = "JSON.stringify({\
        hasHostObjects: !!(window.chrome && window.chrome.webview && window.chrome.webview.hostObjects),\
        hasShare: !!(window.chrome && window.chrome.webview && window.chrome.webview.hostObjects\
            && window.chrome.webview.hostObjects.share\
            && typeof window.chrome.webview.hostObjects.share.RunPlugin === 'function'),\
        result: window.__r})";
    match view
        .evaluate_script(probe)
        .map_err(|e| Error::Engine(format!("hostObjects probe failed: {e:?}")))?
    {
        Ok(s) => Ok(s),
        Err(js_exception) => Err(Error::Engine(format!("hostObjects probe threw: {js_exception}"))),
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

/// Headless proof that pv-next's REAL, unmodified bridge drives our host bridge
/// correctly. `bridge_js` is the actual `bridge-*.js` shipped by pv-next (passed in
/// by the test from a fixture); its trailing ES `export{…}` is rewritten to
/// `window.__pv = {…}` so it runs as a classic script, and NOTHING else is touched —
/// its own `a8` deferred allowlist, `Proxy`/`then`-trap dispatch, `InvokeDeferred`
/// call and `mybrowser:deferred` listener are exercised as-is.
///
/// It then drives three paths through that bridge and settles the enqueued work the
/// way the shell would (`GetPlugins` → descriptors; each `run` → a `PluginResult`
/// envelope):
/// - **deferred** `idbox.ReadChipUid()` (in `a8`) → must go through `InvokeDeferred`
///   and come back via the `mybrowser:deferred` message; unwrapped `success?data`.
/// - **direct raw** `system.isDebug()` (NOT in `a8`) → called straight on the host
///   object; our raw wrapper returns the JSON-decoded `data` (here boolean `true`).
/// - **direct envelope** `system.getEnv()` → the default wrapper returns the whole
///   envelope object.
///
/// Returns a JSON summary `{"uid","idef","dbg","dbgType","envHasSuccess"}` the test
/// asserts on (`idef` is the number of `InvokeDeferred` calls — exactly 1, the
/// deferred one; the two direct calls must not use it).
///
/// # Errors
/// [`Error::Engine`] on engine/eval failure.
pub fn prove_pvnext_bridge(bridge_js: &str) -> Result<String> {
    let renderer = crate::headless::init_renderer()?;
    let view = crate::headless::create_cpu_view(&renderer, 16, 16)?;
    view.load_html("<html><body></body></html>")
        .map_err(|e| Error::Engine(format!("load_html: {e:?}")))?;
    crate::headless::pump_until_loaded(&renderer, &view);

    let pending: PendingCalls = Arc::new(Mutex::new(Vec::new()));
    let origin = Origin::parse("https://pv-next.example").expect("valid test origin");
    install_async_bridge(&view, origin, pending.clone())?;
    // idbox exposes a deferred (a8) method; system exposes a direct raw method and a
    // direct envelope method — exactly as the shell would build them from the registry.
    install_host_objects(
        &view,
        r#"[{"name":"idbox","id":"Asbt.IdBox","methods":[{"js":"ReadChipUid","ui":false,"raw":false}]},
            {"name":"system","id":"Asbt.System","methods":[{"js":"isDebug","ui":false,"raw":true},{"js":"getEnv","ui":false,"raw":false}]}]"#,
    )?;

    // Run pv-next's REAL bridge as a classic script: only the ES export is rewritten.
    let export = "export{aq as a,ap as i,aj as n,ao as o,am as r,an as t};";
    if !bridge_js.contains(export) {
        return Err(Error::Engine(
            "bridge fixture missing the expected export statement".to_string(),
        ));
    }
    let bridge = bridge_js.replace(export, "window.__pv={a:aq,i:ap,n:aj,o:ao,r:am,t:an};");
    if let Err(js) = view
        .evaluate_script(&bridge)
        .map_err(|e| Error::Engine(format!("bridge eval failed: {e:?}")))?
    {
        return Err(Error::Engine(format!("bridge threw: {js}")));
    }

    // Drive the three paths through pv-next's own exports. Count InvokeDeferred so the
    // test can prove the deferred method used it and the direct ones did not.
    let harness = "window.__out='PENDING';\
        (async function(){\
          var P=window.__pv;\
          var _id=window.share.InvokeDeferred; window.__idef=0;\
          window.share.InvokeDeferred=function(){ window.__idef++; return _id.apply(this,arguments); };\
          var h=await P.t('idbox');\
          var uid=h?P.i(await P.o(await h.ReadChipUid())):'NO_IDBOX';\
          var sys=await P.t('system');\
          var dbg=sys?await sys.isDebug():'NO_SYS';\
          var env=sys?await sys.getEnv():null;\
          window.__out=JSON.stringify({\
            uid:uid, idef:window.__idef, dbg:dbg, dbgType:typeof dbg,\
            envHasSuccess:!!(env&&typeof env==='object'&&env.success===true)\
          });\
        })().catch(function(err){ window.__out='ERR:'+((err&&err.message)?err.message:String(err)); });";
    if let Err(js) = view
        .evaluate_script(harness)
        .map_err(|e| Error::Engine(format!("harness eval failed: {e:?}")))?
    {
        return Err(Error::Engine(format!("harness threw: {js}")));
    }

    // Settle the enqueued work, pumping microtasks between rounds so each awaited step
    // (GetPlugins → the host-object call → its result) can advance. Replies are keyed
    // on the inner method so a wrong id/method would be caught by the assertions.
    for _ in 0..12 {
        let calls = std::mem::take(&mut *pending.lock().unwrap_or_else(|p| p.into_inner()));
        for call in calls {
            let v: serde_json::Value =
                serde_json::from_str(&call.request).unwrap_or(serde_json::Value::Null);
            let fail = r#"{"data":"","success":false,"error_message":"unexpected call"}"#;
            let reply = match v.get("kind").and_then(|k| k.as_str()) {
                Some("list") => r#"[{"id":"Asbt.IdBox","name":"idbox","version":"1.0.0","kind":"ui"},
                    {"id":"Asbt.System","name":"system","version":"1.0.0","kind":"ui"}]"#
                    .to_string(),
                Some("run") => {
                    let inner: serde_json::Value = v
                        .get("request")
                        .and_then(|x| x.as_str())
                        .and_then(|s| serde_json::from_str(s).ok())
                        .unwrap_or(serde_json::Value::Null);
                    let method = inner.get("method").and_then(|m| m.as_str()).unwrap_or("");
                    match method {
                        "ReadChipUid" => {
                            r#"{"data":"04a1b2c3d4e5f6","success":true,"error_message":""}"#.to_string()
                        }
                        // raw method: data is JSON-encoded (boolean true).
                        "isDebug" => r#"{"data":"true","success":true,"error_message":""}"#.to_string(),
                        "getEnv" => {
                            r#"{"data":"{\"k\":1}","success":true,"error_message":""}"#.to_string()
                        }
                        _ => fail.to_string(),
                    }
                }
                _ => fail.to_string(),
            };
            resolve_call(&view, call.call_id, &reply)?;
        }
        crate::headless::pump_until_loaded(&renderer, &view);
    }

    match view
        .evaluate_script("window.__out")
        .map_err(|e| Error::Engine(format!("result read failed: {e:?}")))?
    {
        Ok(s) => Ok(s),
        Err(js_exception) => Err(Error::Engine(format!("result read threw: {js_exception}"))),
    }
}

// ---------------------------------------------------------------------------
// Modal UI plugin overlay bridge: the overlay view's own private channel. The
// overlay gets `window.ui.send(msg)` (overlay -> host) and the host pushes
// updates with [`push_ui_update`] (host -> overlay, via `window.ui.onUpdate`).
// The overlay is deliberately given NO `window.native` / `window.share`; it can
// only talk to the UI op that owns it.
// ---------------------------------------------------------------------------

/// Messages the overlay JS sent to its owning UI op, drained by the host.
pub type UiMessages = Arc<Mutex<Vec<String>>>;

/// Install the overlay UI bridge into `view`: `window.ui.send(msg)` pushes `msg`
/// onto `messages`. The overlay HTML sets `window.ui.onUpdate` to receive host
/// pushes (see [`push_ui_update`]); the shim merges rather than clobbering it.
///
/// # Errors
/// [`Error::Engine`] if the native entry point or the shim fails.
pub fn install_ui_bridge(view: &View, messages: UiMessages) -> Result<()> {
    {
        let ctx = view.lock_js_context();
        let global = ctx.global_object();
        let callback = JSObject::new_function_with_callback(&ctx, move |ctx, _this, args| {
            let msg = args
                .first()
                .and_then(|v| v.as_string().ok())
                .map(|s| s.to_string())
                .unwrap_or_default();
            messages.lock().unwrap_or_else(|p| p.into_inner()).push(msg);
            Ok(JSValue::new_undefined(ctx))
        });
        global
            .set_property("__sentinelUiMessage", &callback, JSPropertyAttributes::default())
            .map_err(|e| Error::Engine(format!("set __sentinelUiMessage failed: {e:?}")))?;
    }
    // Merge, don't clobber: the overlay HTML may already have set onUpdate.
    let shim = "(function(){ window.ui = window.ui || {}; \
        window.ui.send = function(m){ window.__sentinelUiMessage(String(m)); }; })();";
    match view
        .evaluate_script(shim)
        .map_err(|e| Error::Engine(format!("ui shim eval failed: {e:?}")))?
    {
        Ok(_) => Ok(()),
        Err(js_exception) => Err(Error::Engine(format!("ui shim threw: {js_exception}"))),
    }
}

/// Push a state update (a JSON string) into the overlay via `window.ui.onUpdate`.
/// The overlay renders it with `textContent`, never as HTML.
///
/// # Errors
/// [`Error::Engine`] if the eval fails or throws.
pub fn push_ui_update(view: &View, update_json: &str) -> Result<()> {
    let arg = serde_json::to_string(update_json).unwrap_or_else(|_| "\"\"".into());
    let js =
        format!("window.ui && typeof window.ui.onUpdate === 'function' && window.ui.onUpdate({arg})");
    match view
        .evaluate_script(&js)
        .map_err(|e| Error::Engine(format!("ui update eval failed: {e:?}")))?
    {
        Ok(_) => Ok(()),
        Err(js_exception) => Err(Error::Engine(format!("ui update threw: {js_exception}"))),
    }
}

/// Headless JS-execution speed probe: time a 50M-iteration integer loop with
/// `Date.now()`. Tens of ms indicates a JIT; hundreds/thousands of ms indicate an
/// interpreter-only JavaScriptCore. Settles whether Ultralight's JS can run a
/// heavy SPA at interactive speed, rather than inferring it from WASM absence.
///
/// # Errors
/// [`Error::Engine`] on engine/eval failure.
pub fn prove_js_speed() -> Result<String> {
    let renderer = crate::headless::init_renderer()?;
    let view = crate::headless::create_cpu_view(&renderer, 16, 16)?;
    view.load_html("<html><body></body></html>")
        .map_err(|e| Error::Engine(format!("load_html: {e:?}")))?;
    crate::headless::pump_until_loaded(&renderer, &view);
    let js = "(function(){var t=Date.now();var s=0;\
        for(var i=0;i<50000000;i++){s+=i;}\
        return JSON.stringify({loopMs:Date.now()-t,checksum:s});})()";
    match view
        .evaluate_script(js)
        .map_err(|e| Error::Engine(format!("speed eval failed: {e:?}")))?
    {
        Ok(s) => Ok(s),
        Err(js_exception) => Err(Error::Engine(format!("speed threw: {js_exception}"))),
    }
}

/// Headless proof of the overlay UI bridge and input capture: load a page with a
/// positioned button, install the bridge, push a host→overlay update, then
/// synthesize a click at the button's centre and confirm the overlay→host message
/// arrived. Returns `{"messages":[...],"status":"..."}`. This is what compile +
/// clippy cannot check: that a synthetic click actually reaches `onclick`.
///
/// # Errors
/// [`Error::Engine`] on engine/eval failure.
pub fn prove_ui_bridge_click() -> Result<String> {
    let engine = crate::page::Engine::new()?;
    let page = engine.create_page(200, 200)?;
    let html = "<!doctype html><html><head><meta charset=\"utf-8\"></head><body>\
        <button id=\"b\" style=\"position:absolute;left:10px;top:10px;width:120px;height:40px\" \
          onclick=\"window.ui.send('ok')\">OK</button>\
        <div id=\"s\"></div>\
        <script>window.ui = window.ui || {}; \
          window.ui.onUpdate = function(j){ try { document.getElementById('s').textContent = JSON.parse(j).status; } catch (e) {} };\
        </script></body></html>";
    page.load_html(html).map_err(|e| Error::Engine(format!("load_html: {e:?}")))?;
    for _ in 0..600 {
        engine.update();
        if !page.is_loading() {
            break;
        }
    }

    let messages: UiMessages = Arc::new(Mutex::new(Vec::new()));
    install_ui_bridge(page.view(), messages.clone())?;

    // host -> overlay
    push_ui_update(page.view(), "{\"status\":\"ready\"}")?;
    for _ in 0..8 {
        engine.update();
        std::thread::sleep(std::time::Duration::from_millis(16));
    }

    // overlay -> host: synthesize a click at the button centre (70, 30).
    page.mouse_move(70, 30);
    page.mouse_button(70, 30, true);
    page.mouse_button(70, 30, false);
    for _ in 0..8 {
        engine.update();
        std::thread::sleep(std::time::Duration::from_millis(16));
    }

    let msgs = messages.lock().unwrap_or_else(|p| p.into_inner()).clone();
    let status = match page.view().evaluate_script("document.getElementById('s').textContent") {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => format!("<js:{e}>"),
        Err(e) => format!("<err:{e:?}>"),
    };
    let msgs_json = serde_json::to_string(&msgs).unwrap_or_else(|_| "[]".into());
    let status_json = serde_json::to_string(&status).unwrap_or_else(|_| "\"\"".into());
    Ok(format!("{{\"messages\":{msgs_json},\"status\":{status_json}}}"))
}
