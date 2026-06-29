//! Headless HTML→pixels rendering via Ultralight's CPU renderer.
//!
//! Public surface: [`render_html_to_bgra`]. Used by render smoke tests and, later,
//! page thumbnails. Initializes the Ultralight library and platform handlers,
//! which Ultralight requires exactly once per process before `Renderer::create`.
//!
//! This module is compiled only when the `ultralight` feature is enabled.

use sentinel_core::{Error, Result};
use ul_next::{config::Config, platform, renderer::Renderer, view::ViewConfig, Library};

/// Render `html` at `width`x`height` (pixels) and return the raw surface bytes in
/// **premultiplied BGRA** order (Ultralight's CPU surface format). The buffer is
/// `row_bytes * height` long, where `row_bytes` is usually `width * 4`.
///
/// Uses the CPU renderer (no GPU device required), which is also the Windows 7
/// fallback path. Pumps the renderer until the page reports it finished loading,
/// bounded by a fixed iteration cap so a stuck load cannot hang the caller.
///
/// # Errors
/// Returns [`Error::Engine`] if the Ultralight libraries cannot be loaded, if any
/// engine object (config, renderer, view, surface) fails to initialize, or if the
/// page fails to load.
///
/// # Safety / process model
/// Ultralight's `Renderer` may be created only **once per process**. Do not call
/// this in a process that also constructs the windowed `App`. The underlying
/// library load is `unsafe` (it `dlopen`s native code); we contain that here.
pub fn render_html_to_bgra(html: &str, width: u32, height: u32) -> Result<Vec<u8>> {
    // SAFETY: `Library::load` dlopens the Ultralight shared libraries. This is
    // sound provided the libraries on the path are the genuine Ultralight SDK
    // binaries (a deployment invariant documented in the crate README). We load
    // once and keep the `Arc<Library>` alive for the duration of all engine use.
    let lib = unsafe { Library::load() }
        .map_err(|e| Error::Engine(format!("failed to load Ultralight libraries: {e}")))?;

    // Platform handlers must be installed before `Renderer::create`.
    platform::enable_platform_fontloader(lib.clone());
    platform::enable_platform_filesystem(lib.clone(), ".")
        .map_err(|e| Error::Engine(format!("filesystem platform init: {e:?}")))?;
    platform::enable_default_logger(lib.clone(), "./ultralight.log")
        .map_err(|e| Error::Engine(format!("logger init: {e:?}")))?;

    let config = Config::start()
        .build(lib.clone())
        .ok_or_else(|| Error::Engine("config build returned None".into()))?;
    let renderer = Renderer::create(config)
        .map_err(|e| Error::Engine(format!("renderer create: {e:?}")))?;

    let view_config = ViewConfig::start()
        .is_accelerated(false) // CPU surface, so `View::surface` is available
        .build(lib.clone())
        .ok_or_else(|| Error::Engine("view config build returned None".into()))?;
    let view = renderer
        .create_view(width, height, &view_config, None)
        .ok_or_else(|| Error::Engine("create_view returned None".into()))?;

    view.load_html(html)
        .map_err(|e| Error::Engine(format!("load_html: {e:?}")))?;

    // Drive the engine until the page finishes loading. The cap prevents an
    // indefinite hang if a resource never resolves.
    const MAX_PUMPS: u32 = 600;
    for _ in 0..MAX_PUMPS {
        renderer.update();
        renderer.render();
        if !view.is_loading() {
            break;
        }
    }

    // CPU renderer => the View owns a pixel-buffer Surface we can read back.
    let mut surface = view
        .surface()
        .ok_or_else(|| Error::Engine("view has no CPU surface (was it accelerated?)".into()))?;
    let pixels = surface
        .lock_pixels()
        .ok_or_else(|| Error::Engine("could not lock surface pixels".into()))?;
    Ok(pixels.to_vec())
}
