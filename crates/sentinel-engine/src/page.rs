//! `Engine` + `PageView`: low-level Ultralight rendering the host presents itself.
//!
//! Unlike the AppCore path (whose overlay views crash on View callbacks), this uses
//! one low-level `Renderer` driving one or more CPU `View`s. The host (shell) owns
//! the window (winit), pumps [`Engine::update`] once per frame, reads each view's
//! pixels with [`PageView::snapshot_bgra`], composites them, and forwards input.
//! Crucially the `window.native` / navigation bridges install here, on Renderer
//! views (verified by the engine's headless tests). Feature `ultralight` only.

use std::sync::{Arc, Mutex};

use sentinel_core::{Error, Result};
use sentinel_ipc::RequestExecutor;
use sentinel_permissions::{Origin, PermissionBroker};
use ul_next::{
    event::{
        KeyEvent, KeyEventCreationInfo, KeyEventModifiers, KeyEventType, MouseButton, MouseEvent,
        MouseEventType, ScrollEvent, ScrollEventType,
    },
    javascript::{JSObject, JSPropertyAttributes, JSValue},
    key_code::VirtualKeyCode,
    renderer::{Renderer, Session},
    view::{View, ViewConfig},
    Library,
};

use crate::jsbridge::{SharedExecutor, SharedRouter};

const NO_MODS: KeyEventModifiers =
    KeyEventModifiers { alt: false, ctrl: false, meta: false, shift: false };

/// Editing / navigation keys that don't produce text but drive input fields.
#[derive(Debug, Clone, Copy)]
pub enum SpecialKey {
    /// Backspace.
    Backspace,
    /// Enter / Return.
    Enter,
    /// Tab.
    Tab,
    /// Escape.
    Escape,
    /// Forward delete.
    Delete,
    /// Left arrow.
    Left,
    /// Right arrow.
    Right,
    /// Up arrow.
    Up,
    /// Down arrow.
    Down,
    /// Home.
    Home,
    /// End.
    End,
    /// Page up.
    PageUp,
    /// Page down.
    PageDown,
}

impl SpecialKey {
    fn virtual_key(self) -> VirtualKeyCode {
        match self {
            SpecialKey::Backspace => VirtualKeyCode::Back,
            SpecialKey::Enter => VirtualKeyCode::Return,
            SpecialKey::Tab => VirtualKeyCode::Tab,
            SpecialKey::Escape => VirtualKeyCode::Escape,
            SpecialKey::Delete => VirtualKeyCode::Delete,
            SpecialKey::Left => VirtualKeyCode::Left,
            SpecialKey::Right => VirtualKeyCode::Right,
            SpecialKey::Up => VirtualKeyCode::Up,
            SpecialKey::Down => VirtualKeyCode::Down,
            SpecialKey::Home => VirtualKeyCode::Home,
            SpecialKey::End => VirtualKeyCode::End,
            SpecialKey::PageUp => VirtualKeyCode::Prior,
            SpecialKey::PageDown => VirtualKeyCode::Next,
        }
    }
}

/// Owns the single Ultralight `Renderer` and creates [`PageView`]s. Only one
/// `Renderer` may exist per process, so create exactly one `Engine`.
pub struct Engine {
    renderer: Renderer,
    /// Non-persistent session shared by all views: cookies, localStorage and cache
    /// stay in memory and are never written to disk (privacy-first — the browser
    /// leaves no browsing data behind between runs).
    session: Session,
    lib: Arc<Library>,
}

impl Engine {
    /// Initialize Ultralight and the CPU renderer with an ephemeral session.
    ///
    /// # Errors
    /// [`Error::Engine`] on initialization or session-creation failure.
    pub fn new() -> Result<Self> {
        let renderer = crate::headless::init_renderer()?;
        let session = renderer
            .create_session(false, "sentinel-ephemeral")
            .map_err(|e| Error::Engine(format!("create ephemeral session: {e:?}")))?;
        Ok(Self { renderer, session, lib: Library::linked() })
    }

    /// Create a `width`x`height` CPU page bound to the engine's ephemeral session.
    ///
    /// # Errors
    /// [`Error::Engine`] if the view config or the view itself cannot be created.
    pub fn create_page(&self, width: u32, height: u32) -> Result<PageView> {
        let view_config = ViewConfig::start()
            .is_accelerated(false)
            .build(self.lib.clone())
            .ok_or_else(|| Error::Engine("view config build returned None".into()))?;
        let view = self
            .renderer
            .create_view(width, height, &view_config, Some(&self.session))
            .ok_or_else(|| Error::Engine("create_view returned None".into()))?;
        Ok(PageView { view, lib: self.lib.clone(), width, height })
    }

    /// Pump one update+render cycle for ALL views. Call once per frame.
    pub fn update(&self) {
        self.renderer.update();
        self.renderer.render();
    }
}

/// A CPU-rendered view. Rendering is driven by its [`Engine`].
pub struct PageView {
    view: View,
    lib: Arc<Library>,
    width: u32,
    height: u32,
}

impl PageView {
    /// Borrow the underlying view.
    pub fn view(&self) -> &View {
        &self.view
    }

    /// Load a self-contained HTML string.
    ///
    /// # Errors
    /// [`Error::Engine`] if the view rejects the HTML.
    pub fn load_html(&self, html: &str) -> Result<()> {
        self.view.load_html(html).map_err(|e| Error::Engine(format!("load_html: {e:?}")))
    }

    /// Load a URL.
    ///
    /// # Errors
    /// [`Error::Engine`] if the view rejects the URL.
    pub fn load_url(&self, url: &str) -> Result<()> {
        self.view.load_url(url).map_err(|e| Error::Engine(format!("load_url: {e:?}")))
    }

    /// Evaluate a script in the page (fire-and-forget).
    pub fn eval(&self, script: &str) {
        let _ = self.view.evaluate_script(script);
    }

    /// Whether the current page is still loading.
    pub fn is_loading(&self) -> bool {
        self.view.is_loading()
    }

    /// The current page URL, if any (WebKit's actual URL — reflects redirects and
    /// query params). Empty/errored URLs return `None`.
    pub fn current_url(&self) -> Option<String> {
        self.view.url().ok().filter(|s| !s.is_empty())
    }

    /// Navigate back in the view's WebKit session history.
    pub fn go_back(&self) {
        self.view.go_back();
    }

    /// Navigate forward in the view's WebKit session history.
    pub fn go_forward(&self) {
        self.view.go_forward();
    }

    /// Reload the current page (re-loads its actual URL, preserving query params).
    pub fn reload(&self) {
        if let Some(url) = self.current_url() {
            let _ = self.view.load_url(&url);
        }
    }

    /// Size in pixels.
    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Resize the view's render surface to `width`x`height` pixels (both clamped to
    /// at least 1). Keeps the view's backing surface in step with the window so the
    /// page re-lays-out and fills the new area.
    pub fn resize(&mut self, width: u32, height: u32) {
        let width = width.max(1);
        let height = height.max(1);
        self.view.resize(width, height);
        self.width = width;
        self.height = height;
    }

    /// Copy the current frame as premultiplied-BGRA bytes + row stride:
    /// `(width, height, row_bytes, pixels)`. Prefer [`Self::blit_into`] in a render
    /// loop — this allocates a copy each call.
    pub fn snapshot_bgra(&self) -> Option<(u32, u32, u32, Vec<u8>)> {
        let mut surface = self.view.surface()?;
        let row_bytes = surface.row_bytes();
        let pixels = surface.lock_pixels()?;
        Some((self.width, self.height, row_bytes, pixels.to_vec()))
    }

    /// Convert this view's surface (premultiplied BGRA) directly into a softbuffer
    /// `0x00RRGGBB` buffer of width `buf_w`/height `buf_h`, at vertical offset `y0`.
    /// No intermediate allocation — the hot path for the render loop.
    pub fn blit_into(&self, buf: &mut [u32], buf_w: u32, buf_h: u32, y0: u32) {
        let mut surface = match self.view.surface() {
            Some(s) => s,
            None => return,
        };
        let row_bytes = surface.row_bytes() as usize;
        let px = match surface.lock_pixels() {
            Some(p) => p,
            None => return,
        };
        let cols = self.width.min(buf_w) as usize;
        for y in 0..self.height {
            let dst_y = y0 + y;
            if dst_y >= buf_h {
                break;
            }
            let src = y as usize * row_bytes;
            let dst = (dst_y * buf_w) as usize;
            let src_row = &px[src..];
            let dst_row = &mut buf[dst..dst + cols];
            for (dst_px, src_px) in dst_row.iter_mut().zip(src_row.chunks_exact(4)) {
                let b = src_px[0] as u32;
                let g = src_px[1] as u32;
                let r = src_px[2] as u32;
                *dst_px = (r << 16) | (g << 8) | b;
            }
        }
    }

    /// Install the `window.native` capability bridge NOW (call from the host loop
    /// after the page finished loading — NOT from a DOM-ready callback, which runs
    /// inside `Engine::update` and crashes when multiple views exist).
    ///
    /// # Errors
    /// [`Error::Engine`] if the bridge script fails to install.
    pub fn install_native_now<B, E>(
        &self,
        router: SharedRouter<B>,
        executor: SharedExecutor<E>,
        origin: Origin,
    ) -> Result<()>
    where
        B: PermissionBroker + 'static,
        E: RequestExecutor + 'static,
    {
        crate::jsbridge::install_native(&self.view, router, executor, origin)
    }

    /// Install the chrome-control `window.__sentinelNav(cmd)` function NOW, which
    /// pushes command strings onto `queue` for the host loop to apply. Call from the
    /// host loop after the chrome page loaded (see [`Self::install_native_now`]).
    pub fn install_nav_now(&self, queue: Arc<Mutex<Vec<String>>>) {
        let ctx = self.view.lock_js_context();
        let global = ctx.global_object();
        let callback = JSObject::new_function_with_callback(&ctx, move |ctx, _this, args| {
            if let Some(s) = args.first().and_then(|v| v.as_string().ok()) {
                queue.lock().unwrap_or_else(|p| p.into_inner()).push(s.to_string());
            }
            Ok(JSValue::new_string(ctx, "ok"))
        });
        let _ = global.set_property("__sentinelNav", &callback, JSPropertyAttributes::default());
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

    /// Scroll by `(dx, dy)` pixels (positive `dy` scrolls the page content down).
    pub fn scroll(&self, dx: i32, dy: i32) {
        if let Ok(ev) =
            ScrollEvent::new(self.lib.clone(), ScrollEventType::ScrollByPixel, dx, dy)
        {
            self.view.fire_scroll_event(ev);
        }
    }

    /// Insert `text` via a `Char` key event (the only key type that generates text).
    pub fn key_char(&self, text: &str) {
        let info = KeyEventCreationInfo {
            ty: KeyEventType::Char,
            modifiers: NO_MODS,
            virtual_key_code: VirtualKeyCode::Unknown,
            native_key_code: 0,
            text,
            unmodified_text: text,
            is_keypad: false,
            is_auto_repeat: false,
            is_system_key: false,
        };
        if let Ok(ev) = KeyEvent::new(self.lib.clone(), info) {
            self.view.fire_key_event(ev);
        }
    }

    /// Forward an editing/navigation key (backspace, enter, arrows…) as a raw event.
    pub fn key_special(&self, key: SpecialKey, pressed: bool) {
        let info = KeyEventCreationInfo {
            ty: if pressed { KeyEventType::RawKeyDown } else { KeyEventType::KeyUp },
            modifiers: NO_MODS,
            virtual_key_code: key.virtual_key(),
            native_key_code: 0,
            text: "",
            unmodified_text: "",
            is_keypad: false,
            is_auto_repeat: false,
            is_system_key: false,
        };
        if let Ok(ev) = KeyEvent::new(self.lib.clone(), info) {
            self.view.fire_key_event(ev);
        }
    }
}
