//! Headless HTML→pixels rendering via Ultralight's CPU renderer.
//!
//! Public surface: [`render_html_to_bgra`]. Used by render smoke tests and, later,
//! page thumbnails. The pub(crate) helpers ([`init_renderer`], [`create_cpu_view`],
//! [`pump_until_loaded`]) capture the once-per-process Ultralight setup so other
//! engine entry points (e.g. the JS-bridge verification) reuse the exact path.
//!
//! This module is compiled only when the `ultralight` feature is enabled.

use std::path::PathBuf;

use sentinel_core::{Error, Result};
use ul_next::{config::Config, platform, renderer::Renderer, view::View, view::ViewConfig, Library};

/// Directory containing the running executable, used as the Ultralight resource
/// base so resource discovery is independent of the current working directory.
fn exe_dir() -> Result<PathBuf> {
    let exe = std::env::current_exe()
        .map_err(|e| Error::Engine(format!("cannot locate current executable: {e}")))?;
    exe.parent()
        .map(|p| p.to_path_buf())
        .ok_or_else(|| Error::Engine("executable has no parent directory".into()))
}

/// Initialize Ultralight once per process: load the linked library, install the
/// platform handlers (font loader, exe-relative filesystem, logger), and create
/// the CPU [`Renderer`].
///
/// # Process model
/// The `Renderer` may be created only **once per process**, so a process that
/// calls this must not also construct the windowed `App`.
///
/// # Errors
/// [`Error::Engine`] if a platform handler or the renderer fails to initialize.
pub(crate) fn init_renderer() -> Result<Renderer> {
    // Linked mode: symbols are resolved at build time against ul-next-sys's
    // matching import library. The DLLs are staged next to the executable by the
    // crate's build script and resolved by the OS loader.
    let lib = Library::linked();

    // Resolve the SDK resource base from the executable's own directory so it does
    // NOT depend on the current working directory.
    let base = exe_dir()?;
    let base_str = base
        .to_str()
        .ok_or_else(|| Error::Engine("executable path is not valid UTF-8".into()))?;
    let log_path = base.join("ultralight.log");

    platform::enable_platform_fontloader(lib.clone());
    platform::enable_platform_filesystem(lib.clone(), base_str)
        .map_err(|e| Error::Engine(format!("filesystem platform init: {e:?}")))?;
    platform::enable_default_logger(lib.clone(), log_path.to_string_lossy().as_ref())
        .map_err(|e| Error::Engine(format!("logger init: {e:?}")))?;

    let config = Config::start()
        .build(lib.clone())
        .ok_or_else(|| Error::Engine("config build returned None".into()))?;
    Renderer::create(config).map_err(|e| Error::Engine(format!("renderer create: {e:?}")))
}

/// Create a non-accelerated (CPU) [`View`] of the given size, which exposes a
/// readable pixel surface and a usable JS context.
///
/// # Errors
/// [`Error::Engine`] if the view config or the view itself fails to construct.
pub(crate) fn create_cpu_view(renderer: &Renderer, width: u32, height: u32) -> Result<View> {
    let lib = Library::linked();
    let view_config = ViewConfig::start()
        .is_accelerated(false)
        .build(lib)
        .ok_or_else(|| Error::Engine("view config build returned None".into()))?;
    renderer
        .create_view(width, height, &view_config, None)
        .ok_or_else(|| Error::Engine("create_view returned None".into()))
}

/// Drive the renderer until `view` finishes loading, bounded by a fixed iteration
/// cap so a stuck load cannot hang the caller.
pub(crate) fn pump_until_loaded(renderer: &Renderer, view: &View) {
    const MAX_PUMPS: u32 = 600;
    for _ in 0..MAX_PUMPS {
        renderer.update();
        renderer.render();
        if !view.is_loading() {
            break;
        }
    }
}

/// Render `html` at `width`x`height` (pixels) and return the raw surface bytes in
/// **premultiplied BGRA** order (Ultralight's CPU surface format). The buffer is
/// `row_bytes * height` long, where `row_bytes` is usually `width * 4`.
///
/// # Errors
/// Returns [`Error::Engine`] if the engine fails to initialize, the view/surface
/// cannot be created, or the page fails to load.
///
/// # Process model
/// See [`init_renderer`] — the `Renderer` is per-process; do not mix with the
/// windowed `App`.
pub fn render_html_to_bgra(html: &str, width: u32, height: u32) -> Result<Vec<u8>> {
    let renderer = init_renderer()?;
    let view = create_cpu_view(&renderer, width, height)?;

    view.load_html(html)
        .map_err(|e| Error::Engine(format!("load_html: {e:?}")))?;
    pump_until_loaded(&renderer, &view);

    // CPU renderer => the View owns a pixel-buffer Surface we can read back.
    let mut surface = view
        .surface()
        .ok_or_else(|| Error::Engine("view has no CPU surface (was it accelerated?)".into()))?;
    let pixels = surface
        .lock_pixels()
        .ok_or_else(|| Error::Engine("could not lock surface pixels".into()))?;
    Ok(pixels.to_vec())
}

/// Load `url` headlessly, let the page settle (network + scripts + ES-module
/// evaluation + framework mount) for `budget`, then return a JSON diagnostic
/// string describing whether an SPA actually booted. Used as a one-off
/// engine-compatibility probe — NOT a normal code path.
///
/// The returned JSON has shape:
/// `{"nonblank":bool,"elapsed_ms":n,"diag":{...page diagnostics...}}`.
///
/// # Errors
/// [`Error::Engine`] if the engine fails to initialize or the URL is rejected.
///
/// # Process model
/// See [`init_renderer`] — the `Renderer` is per-process.
pub fn probe_url(url: &str, width: u32, height: u32, budget: std::time::Duration) -> Result<String> {
    use std::time::Instant;

    let renderer = init_renderer()?;
    let view = create_cpu_view(&renderer, width, height)?;
    view.load_url(url).map_err(|e| Error::Engine(format!("load_url: {e:?}")))?;

    // Pump with wall-clock sleeps so async network and deferred ES modules can
    // make progress. Keep pumping for the whole budget even after `is_loading`
    // goes false, so module evaluation and the framework mount have time to run.
    let start = Instant::now();
    loop {
        renderer.update();
        renderer.render();
        if start.elapsed() >= budget {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(16));
    }

    let diag_js = r#"(function(){
      try {
        var app = document.getElementById('app');
        var loader = document.getElementById('app-loader');
        function esc(s){ return (s==null?'':s.toString()).slice(0,300); }
        return JSON.stringify({
          href: location.href,
          title: document.title,
          readyState: document.readyState,
          scripts: document.scripts.length,
          htmlLen: document.documentElement ? document.documentElement.outerHTML.length : -1,
          loaderPresent: !!loader,
          appChildren: app ? app.children.length : -1,
          appHtmlLen: app ? app.innerHTML.length : -1,
          vueMounted: !!(app && app.__vue_app__),
          bodyTextLen: document.body ? (document.body.innerText||'').length : -1,
          bodyText: esc(document.body ? document.body.innerText : ''),
          ua: esc(navigator.userAgent),
          typePromise: typeof Promise,
          typeFetch: typeof fetch,
          typeWeakRef: typeof WeakRef,
          typeBigInt: typeof BigInt,
          typeProxy: typeof Proxy
        });
      } catch(e){ return JSON.stringify({diagError: ''+e}); }
    })()"#;
    let diag = match view.evaluate_script(diag_js) {
        Ok(Ok(s)) => s,
        Ok(Err(js_exc)) => format!("{{\"jsException\":\"{}\"}}", js_exc.replace('"', "'")),
        Err(e) => format!("{{\"evalError\":\"{e:?}\"}}"),
    };

    // Non-blank check: does any pixel differ from the top-left pixel? A solid
    // surface (all pixels equal) means nothing meaningful was painted.
    let nonblank = match view.surface() {
        Some(mut s) => match s.lock_pixels() {
            Some(px) if px.len() >= 4 => {
                let first = [px[0], px[1], px[2]];
                px.chunks_exact(4).any(|p| [p[0], p[1], p[2]] != first)
            }
            _ => false,
        },
        None => false,
    };

    Ok(format!(
        "{{\"nonblank\":{nonblank},\"elapsed_ms\":{},\"diag\":{diag}}}",
        start.elapsed().as_millis()
    ))
}

/// Probe the engine's JS capabilities relevant to the pv-next hardware flows:
/// WebAssembly (compile + run), media capture (`getUserMedia`, `mediaDevices`,
/// `RTCPeerConnection`), and WebGL. Loads a tiny self-contained page (no network)
/// and returns `window.__caps` as a JSON string after letting async calls settle.
///
/// # Errors
/// [`Error::Engine`] if the engine fails to initialize or the eval fails.
///
/// # Process model
/// See [`init_renderer`] — the `Renderer` is per-process.
pub fn probe_capabilities(budget: std::time::Duration) -> Result<String> {
    use std::time::Instant;

    let renderer = init_renderer()?;
    let view = create_cpu_view(&renderer, 320, 240)?;

    // Self-contained page: feature-detect synchronously, run a tiny WASM module
    // synchronously (Module/Instance are sync), and kick off the async media calls
    // whose outcome we read after pumping.
    let html = r##"<!doctype html><html><head><meta charset="utf-8"></head><body>
<script>
var caps = window.__caps = { phase: 'init' };
try {
  caps.WebAssembly = typeof WebAssembly;
  caps.wasmValidate = (typeof WebAssembly !== 'undefined' && WebAssembly.validate)
    ? WebAssembly.validate(new Uint8Array([0,97,115,109,1,0,0,0])) : 'n/a';
  try {
    // module exporting add(i32,i32)->i32
    var bytes = new Uint8Array([0,97,115,109,1,0,0,0, 1,7,1,96,2,127,127,1,127,
      3,2,1,0, 7,7,1,3,97,100,100,0,0, 10,9,1,7,0,32,0,32,1,106,11]);
    var inst = new WebAssembly.Instance(new WebAssembly.Module(bytes));
    caps.wasmRun = 'add(2,3)=' + inst.exports.add(2,3);
  } catch (e) { caps.wasmRun = 'err:' + (e && e.name) + ':' + (e && e.message); }

  caps.hasMediaDevices = !!(navigator && navigator.mediaDevices);
  caps.hasGetUserMedia = !!(navigator && navigator.mediaDevices && navigator.mediaDevices.getUserMedia);
  caps.hasLegacyGUM = !!(navigator && (navigator.getUserMedia || navigator.webkitGetUserMedia));
  caps.hasEnumerateDevices = !!(navigator && navigator.mediaDevices && navigator.mediaDevices.enumerateDevices);
  caps.RTCPeerConnection = typeof (window.RTCPeerConnection || window.webkitRTCPeerConnection);
  caps.MediaStream = typeof window.MediaStream;
  caps.MediaRecorder = typeof window.MediaRecorder;
  caps.webgl = (function(){ try { var c=document.createElement('canvas');
    return !!(c.getContext('webgl') || c.getContext('experimental-webgl')); } catch(e){ return 'err'; } })();
  caps.webgl2 = (function(){ try { return !!document.createElement('canvas').getContext('webgl2'); } catch(e){ return 'err'; } })();
  caps.OffscreenCanvas = typeof OffscreenCanvas;
  caps.Worker = typeof Worker;
  caps.SharedArrayBuffer = typeof SharedArrayBuffer;

  caps.gum = 'pending';
  if (caps.hasGetUserMedia) {
    try {
      navigator.mediaDevices.getUserMedia({ video: true })
        .then(function (s) { caps.gum = 'ok:tracks=' + (s.getVideoTracks ? s.getVideoTracks().length : '?'); })
        .catch(function (e) { caps.gum = 'reject:' + (e && e.name) + ':' + (e && e.message); });
    } catch (e) { caps.gum = 'throw:' + (e && e.name) + ':' + (e && e.message); }
  } else { caps.gum = 'absent'; }

  caps.enumDev = 'pending';
  if (caps.hasEnumerateDevices) {
    try {
      navigator.mediaDevices.enumerateDevices()
        .then(function (d) { caps.enumDev = 'ok:' + d.length; })
        .catch(function (e) { caps.enumDev = 'reject:' + (e && e.name); });
    } catch (e) { caps.enumDev = 'throw:' + e; }
  } else { caps.enumDev = 'absent'; }

  caps.phase = 'done';
} catch (e) { caps.fatal = '' + e; }
</script></body></html>"##;

    view.load_html(html).map_err(|e| Error::Engine(format!("load_html: {e:?}")))?;

    let start = Instant::now();
    loop {
        renderer.update();
        renderer.render();
        if start.elapsed() >= budget {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(16));
    }

    match view.evaluate_script("JSON.stringify(window.__caps || {missing:true})") {
        Ok(Ok(s)) => Ok(s),
        Ok(Err(js)) => Ok(format!("{{\"jsException\":\"{}\"}}", js.replace('"', "'"))),
        Err(e) => Err(Error::Engine(format!("caps eval failed: {e:?}"))),
    }
}
