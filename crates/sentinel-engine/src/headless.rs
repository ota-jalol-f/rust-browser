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
