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
        MouseEventType,
    },
    javascript::{JSObject, JSPropertyAttributes, JSValue},
    key_code::VirtualKeyCode,
    renderer::Renderer,
    view::View,
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
    lib: Arc<Library>,
}

impl Engine {
    /// Initialize Ultralight and the CPU renderer.
    ///
    /// # Errors
    /// [`Error::Engine`] on initialization failure.
    pub fn new() -> Result<Self> {
        Ok(Self { renderer: crate::headless::init_renderer()?, lib: Library::linked() })
    }

    /// Create a `width`x`height` CPU page.
    ///
    /// # Errors
    /// [`Error::Engine`] if the view cannot be created.
    pub fn create_page(&self, width: u32, height: u32) -> Result<PageView> {
        let view = crate::headless::create_cpu_view(&self.renderer, width, height)?;
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

    /// Size in pixels.
    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Copy the current frame as premultiplied-BGRA bytes + row stride:
    /// `(width, height, row_bytes, pixels)`.
    pub fn snapshot_bgra(&self) -> Option<(u32, u32, u32, Vec<u8>)> {
        let mut surface = self.view.surface()?;
        let row_bytes = surface.row_bytes();
        let pixels = surface.lock_pixels()?;
        Some((self.width, self.height, row_bytes, pixels.to_vec()))
    }

    /// Install the `window.native` capability bridge on each page's DOM-ready.
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

    /// Install a chrome-control bridge: a `window.__sentinelNav(cmd)` function that
    /// pushes the command string onto `queue` for the host loop to apply. Used by
    /// the address-bar chrome to drive the content view.
    pub fn install_nav_bridge(&self, queue: Arc<Mutex<Vec<String>>>) {
        self.view.set_dom_ready_callback(move |view, _frame_id, is_main_frame, _url| {
            if !is_main_frame {
                return;
            }
            let queue = queue.clone();
            let ctx = view.lock_js_context();
            let global = ctx.global_object();
            let callback = JSObject::new_function_with_callback(&ctx, move |ctx, _this, args| {
                if let Some(s) = args.first().and_then(|v| v.as_string().ok()) {
                    queue.lock().unwrap_or_else(|p| p.into_inner()).push(s.to_string());
                }
                Ok(JSValue::new_string(ctx, "ok"))
            });
            let _ =
                global.set_property("__sentinelNav", &callback, JSPropertyAttributes::default());
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
