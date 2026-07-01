//! `PageView`: a self-driven Ultralight page the host renders into its own window.
//!
//! Unlike the AppCore path (which owns the window and crashes on View callbacks),
//! this uses the low-level `Renderer` + a CPU `View`. The host (shell) owns the
//! window (via winit), pumps [`PageView::update`], reads pixels with
//! [`PageView::snapshot_bgra`], forwards input, and — crucially — can install the
//! `window.native` bridge here, which works on Renderer views (proven by tests).
//!
//! Compiled only with the `ultralight` feature.

use std::sync::Arc;

use sentinel_core::{Error, Result};
use sentinel_ipc::RequestExecutor;
use sentinel_permissions::{Origin, PermissionBroker};
use ul_next::{
    event::{MouseButton, MouseEvent, MouseEventType},
    renderer::Renderer,
    view::View,
    Library,
};

use crate::jsbridge::{SharedExecutor, SharedRouter};

/// A CPU-rendered page the host presents itself.
pub struct PageView {
    renderer: Renderer,
    view: View,
    lib: Arc<Library>,
    width: u32,
    height: u32,
}

impl PageView {
    /// Create a page of `width`x`height` pixels using the CPU renderer.
    ///
    /// # Errors
    /// [`Error::Engine`] if the engine or view fails to initialize.
    pub fn new(width: u32, height: u32) -> Result<Self> {
        let renderer = crate::headless::init_renderer()?;
        let view = crate::headless::create_cpu_view(&renderer, width, height)?;
        Ok(Self { renderer, view, lib: Library::linked(), width, height })
    }

    /// Borrow the underlying view (for `jsbridge::install_native`, callbacks, etc.).
    pub fn view(&self) -> &View {
        &self.view
    }

    /// Load a self-contained HTML string.
    ///
    /// # Errors
    /// [`Error::Engine`] if the view rejects the HTML.
    pub fn load_html(&self, html: &str) -> Result<()> {
        self.view
            .load_html(html)
            .map_err(|e| Error::Engine(format!("load_html: {e:?}")))
    }

    /// Load a URL.
    ///
    /// # Errors
    /// [`Error::Engine`] if the view rejects the URL.
    pub fn load_url(&self, url: &str) -> Result<()> {
        self.view
            .load_url(url)
            .map_err(|e| Error::Engine(format!("load_url: {e:?}")))
    }

    /// Pump one update+render cycle. Call every frame.
    pub fn update(&self) {
        self.renderer.update();
        self.renderer.render();
    }

    /// Whether the current page is still loading.
    pub fn is_loading(&self) -> bool {
        self.view.is_loading()
    }

    /// Current logical size in pixels.
    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Copy the current frame as premultiplied-BGRA bytes plus its row stride.
    /// Returns `(width, height, row_bytes, pixels)`.
    pub fn snapshot_bgra(&self) -> Option<(u32, u32, u32, Vec<u8>)> {
        let mut surface = self.view.surface()?;
        let row_bytes = surface.row_bytes();
        let pixels = surface.lock_pixels()?;
        Some((self.width, self.height, row_bytes, pixels.to_vec()))
    }

    /// Install the `window.native` bridge on every page load (DOM-ready), routing
    /// calls for the page's origin through `router` and executing granted ones on
    /// `executor`. Pages with no parseable URL (e.g. `load_html`) use
    /// `fallback_origin`.
    ///
    /// This works because the callback + install run on a low-level Renderer view
    /// (verified by the engine's headless tests) — unlike AppCore overlay views.
    pub fn install_bridge<B, E>(
        &self,
        router: SharedRouter<B>,
        executor: SharedExecutor<E>,
        fallback_origin: Origin,
    ) where
        B: PermissionBroker + 'static,
        E: RequestExecutor + 'static,
    {
        self.view.set_dom_ready_callback(move |view, _frame_id, is_main_frame, url| {
            if !is_main_frame {
                return;
            }
            let origin = Origin::parse(&url).unwrap_or_else(|_| fallback_origin.clone());
            if let Err(e) =
                crate::jsbridge::install_native(view, router.clone(), executor.clone(), origin)
            {
                eprintln!("window.native install failed: {e}");
            }
        });
    }

    /// Forward a mouse move to `(x, y)` (view-relative pixels).
    pub fn mouse_move(&self, x: i32, y: i32) {
        if let Ok(ev) =
            MouseEvent::new(self.lib.clone(), MouseEventType::MouseMoved, x, y, MouseButton::None)
        {
            self.view.fire_mouse_event(ev);
        }
    }

    /// Forward a left-button press or release at `(x, y)`.
    pub fn mouse_button(&self, x: i32, y: i32, pressed: bool) {
        let ty = if pressed { MouseEventType::MouseDown } else { MouseEventType::MouseUp };
        if let Ok(ev) = MouseEvent::new(self.lib.clone(), ty, x, y, MouseButton::Left) {
            self.view.fire_mouse_event(ev);
        }
    }
}
