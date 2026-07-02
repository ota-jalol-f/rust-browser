//! The windowed browser: a winit window presenting two composited Ultralight
//! views — a chrome strip (address bar) on top and the page content below — with
//! input routed by position/focus and navigation driven from the chrome.
//!
//! This replaces AppCore (whose overlay views crash on the JS-bridge callback).
//! Compiled only with the `ultralight` feature.

use std::num::NonZeroU32;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use sentinel_bridge::BridgeRouter;
use sentinel_core::{BrowserConfig, Error, Result};
use sentinel_device_host::{DeviceHost, SystemBackend};
use sentinel_dll_host::DllHost;
use sentinel_engine::page::{Engine, PageView, SpecialKey};
use sentinel_hosts::HostRouter;
use sentinel_permissions::{Capability, InMemoryBroker, Origin, PermissionBroker, PermissionState};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition};
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

use crate::browser::normalize_url;
use crate::chrome::{BAR_HEIGHT, CHROME_HTML};

type Router = Arc<Mutex<BridgeRouter<InMemoryBroker>>>;
type Executor = Arc<Mutex<HostRouter<SystemBackend>>>;

/// Origin for the built-in home page (no real URL); pre-granted every capability.
const BUILTIN_ORIGIN: &str = "sentinel://home";

/// Which view currently receives keyboard input.
#[derive(Clone, Copy, PartialEq)]
enum Focus {
    Chrome,
    Content,
}

/// Enumerate USB + serial devices in Rust for injection into the dashboard.
pub fn device_snapshot_json() -> String {
    use sentinel_ipc::{Request, RequestExecutor};
    let mut hosts = HostRouter::new(DeviceHost::new(SystemBackend), DllHost::new());
    let usb = hosts.execute(Request::UsbList);
    let serial = hosts.execute(Request::SerialList);
    let usb_json = serde_json::to_string(&usb).unwrap_or_else(|_| "null".into());
    let serial_json = serde_json::to_string(&serial).unwrap_or_else(|_| "null".into());
    format!("{{\"usb\":{usb_json},\"serial\":{serial_json}}}")
}

/// Run the browser. `home_html` is shown in the content view at startup.
///
/// # Errors
/// [`Error::Engine`] if the event loop or window cannot be created.
pub fn run(config: &BrowserConfig, home_html: String) -> Result<()> {
    let event_loop =
        EventLoop::new().map_err(|e| Error::Engine(format!("winit event loop: {e}")))?;
    let mut app = App {
        home_html,
        width: config.window_width,
        height: config.window_height,
        window: None,
        surface: None,
        engine: None,
        chrome: None,
        content: None,
        bar_h: BAR_HEIGHT as u32,
        cursor: PhysicalPosition::new(0.0, 0.0),
        focus: Focus::Content,
        nav_queue: Arc::new(Mutex::new(Vec::new())),
        router: None,
        executor: None,
        home_origin: Origin::parse(BUILTIN_ORIGIN).expect("valid"),
        chrome_installed: false,
        content_loading: true,
        needs_paint: true,
    };
    event_loop.run_app(&mut app).map_err(|e| Error::Engine(format!("winit run: {e}")))
}

struct App {
    home_html: String,
    width: u32,
    height: u32,
    window: Option<Rc<Window>>,
    surface: Option<softbuffer::Surface<Rc<Window>, Rc<Window>>>,
    engine: Option<Engine>,
    chrome: Option<PageView>,
    content: Option<PageView>,
    bar_h: u32,
    cursor: PhysicalPosition<f64>,
    focus: Focus,
    nav_queue: Arc<Mutex<Vec<String>>>,
    router: Option<Router>,
    executor: Option<Executor>,
    home_origin: Origin,
    /// Whether the chrome's nav bridge has been installed (chrome loads once).
    chrome_installed: bool,
    /// Previous-frame loading state of the content view (to detect load completion).
    content_loading: bool,
    /// Repaint requested (input/nav/startup). Avoids constant-spin CPU when idle.
    needs_paint: bool,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        // Sleep until there's work (redraw/input) instead of spinning at max speed.
        event_loop.set_control_flow(winit::event_loop::ControlFlow::Wait);
        let attrs = Window::default_attributes()
            .with_title("Sentinel")
            .with_inner_size(LogicalSize::new(self.width, self.height))
            .with_resizable(false);
        let window = match event_loop.create_window(attrs) {
            Ok(w) => Rc::new(w),
            Err(e) => return fail(event_loop, format!("create window: {e}")),
        };

        let context = match softbuffer::Context::new(window.clone()) {
            Ok(c) => c,
            Err(e) => return fail(event_loop, format!("softbuffer context: {e}")),
        };
        let surface = match softbuffer::Surface::new(&context, window.clone()) {
            Ok(s) => s,
            Err(e) => return fail(event_loop, format!("softbuffer surface: {e}")),
        };

        let size = window.inner_size();
        let (win_w, win_h) = (size.width.max(1), size.height.max(1));
        let bar_h = ((BAR_HEIGHT * window.scale_factor()) as u32).clamp(24, win_h.saturating_sub(1).max(1));
        let content_h = win_h.saturating_sub(bar_h).max(1);

        let engine = match Engine::new() {
            Ok(e) => e,
            Err(e) => return fail(event_loop, format!("engine: {e}")),
        };
        let chrome = match engine.create_page(win_w, bar_h) {
            Ok(p) => p,
            Err(e) => return fail(event_loop, format!("chrome view: {e}")),
        };
        let content = match engine.create_page(win_w, content_h) {
            Ok(p) => p,
            Err(e) => return fail(event_loop, format!("content view: {e}")),
        };

        // Load both pages. The JS bridges are installed later, from the main loop,
        // once each page finishes loading — NOT from a DOM-ready callback (which
        // runs inside Engine::update and crashes with multiple views).
        let _ = chrome.load_html(CHROME_HTML);

        let router: Router = Arc::new(Mutex::new(BridgeRouter::new(InMemoryBroker::new())));
        let executor: Executor =
            Arc::new(Mutex::new(HostRouter::new(DeviceHost::new(SystemBackend), DllHost::new())));
        let home_origin = Origin::parse(BUILTIN_ORIGIN).expect("BUILTIN_ORIGIN is valid");
        {
            let mut g = router.lock().unwrap_or_else(|p| p.into_inner());
            for cap in Capability::ALL {
                g.broker_mut().set(&home_origin, cap, PermissionState::Granted);
            }
        }
        let _ = content.load_html(&self.home_html);

        self.home_origin = home_origin;
        self.bar_h = bar_h;
        self.window = Some(window);
        self.surface = Some(surface);
        self.engine = Some(engine);
        self.chrome = Some(chrome);
        self.content = Some(content);
        self.router = Some(router);
        self.executor = Some(executor);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        // Any input should trigger a repaint (hover, caret, click feedback).
        if matches!(
            event,
            WindowEvent::CursorMoved { .. }
                | WindowEvent::MouseInput { .. }
                | WindowEvent::KeyboardInput { .. }
        ) {
            self.needs_paint = true;
        }
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::RedrawRequested => self.paint(),
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = position;
                let (x, y) = (position.x as i32, position.y as i32);
                if y < self.bar_h as i32 {
                    if let Some(c) = &self.chrome {
                        c.mouse_move(x, y);
                    }
                } else if let Some(c) = &self.content {
                    c.mouse_move(x, y - self.bar_h as i32);
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if button == MouseButton::Left {
                    let (x, y) = (self.cursor.x as i32, self.cursor.y as i32);
                    let pressed = state == ElementState::Pressed;
                    if y < self.bar_h as i32 {
                        self.focus = Focus::Chrome;
                        if let Some(c) = &self.chrome {
                            c.mouse_button(x, y, pressed);
                        }
                    } else {
                        self.focus = Focus::Content;
                        if let Some(c) = &self.content {
                            c.mouse_button(x, y - self.bar_h as i32, pressed);
                        }
                    }
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let target = match self.focus {
                    Focus::Chrome => self.chrome.as_ref(),
                    Focus::Content => self.content.as_ref(),
                };
                if let Some(page) = target {
                    let pressed = event.state == ElementState::Pressed;
                    if let Key::Named(named) = &event.logical_key {
                        if let Some(sk) = map_named_key(named) {
                            page.key_special(sk, pressed);
                        }
                    }
                    if pressed {
                        if let Some(text) = event.text.as_ref() {
                            if !text.chars().all(char::is_control) {
                                page.key_char(text);
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        self.apply_nav_commands();
        self.install_bridges_if_ready();
        // Keep pumping while a page loads; otherwise repaint only when something
        // changed. This avoids burning CPU on a static, idle page.
        let loading = self.chrome.as_ref().is_some_and(|c| c.is_loading())
            || self.content.as_ref().is_some_and(|c| c.is_loading());
        if loading || self.needs_paint {
            if let Some(window) = &self.window {
                window.request_redraw();
            }
            self.needs_paint = false;
        }
    }
}

impl App {
    /// Install the JS bridges from the main loop once their pages have loaded —
    /// safe here (outside `Engine::update`), unlike a DOM-ready callback. Re-installs
    /// window.native on the content view after each navigation (the JS context resets).
    fn install_bridges_if_ready(&mut self) {
        // Chrome nav bridge: install once, after the chrome page loads.
        if !self.chrome_installed {
            if let Some(chrome) = &self.chrome {
                if !chrome.is_loading() {
                    chrome.install_nav_now(self.nav_queue.clone());
                    self.chrome_installed = true;
                }
            }
        }

        // On each content load-completion: (re)install window.native for the new
        // page's origin, and sync the address bar to the actual URL.
        let loading = self.content.as_ref().is_some_and(|c| c.is_loading());
        if self.content_loading && !loading {
            let url = self.content.as_ref().and_then(|c| c.current_url());
            let origin = url
                .as_deref()
                .and_then(|u| Origin::parse(u).ok())
                .unwrap_or_else(|| self.home_origin.clone());
            if let (Some(content), Some(router), Some(executor)) =
                (&self.content, &self.router, &self.executor)
            {
                let _ = content.install_native_now(router.clone(), executor.clone(), origin);
            }
            if let (Some(u), Some(chrome)) = (url, &self.chrome) {
                let js = serde_json::to_string(&u).unwrap_or_else(|_| "\"\"".into());
                chrome.eval(&format!("window.setUrl && window.setUrl({js})"));
            }
            self.needs_paint = true;
        }
        self.content_loading = loading;
    }

    /// Drain navigation commands from the chrome and apply them to the content view
    /// using the view's own WebKit session history (which tracks query params,
    /// redirects, and in-page navigations correctly). The address bar is synced when
    /// the resulting load completes (see [`Self::install_bridges_if_ready`]).
    fn apply_nav_commands(&mut self) {
        let commands: Vec<String> = {
            let mut q = self.nav_queue.lock().unwrap_or_else(|p| p.into_inner());
            std::mem::take(&mut *q)
        };
        if commands.is_empty() {
            return;
        }
        self.needs_paint = true;
        let Some(content) = &self.content else {
            return;
        };
        for cmd in commands {
            if let Some(url) = cmd.strip_prefix("nav ") {
                let _ = content.load_url(&normalize_url(url));
            } else if cmd == "back" {
                content.go_back();
            } else if cmd == "forward" {
                content.go_forward();
            } else if cmd == "reload" {
                content.reload();
            }
        }
    }

    fn paint(&mut self) {
        let win_w = self.width_px();
        let win_h = self.height_px();
        let bar_h = self.bar_h;
        let (Some(engine), Some(chrome), Some(content), Some(surface)) =
            (&self.engine, &self.chrome, &self.content, &mut self.surface)
        else {
            return;
        };
        engine.update();

        let (Some(nw), Some(nh)) = (NonZeroU32::new(win_w), NonZeroU32::new(win_h)) else {
            return;
        };
        if surface.resize(nw, nh).is_err() {
            return;
        }
        let Ok(mut buf) = surface.buffer_mut() else {
            return;
        };
        // Clear to the chrome background so any gap is not garbage.
        buf.fill(0x20242b);

        chrome.blit_into(&mut buf, win_w, win_h, 0);
        content.blit_into(&mut buf, win_w, win_h, bar_h);
        let _ = buf.present();
    }

    fn width_px(&self) -> u32 {
        self.window.as_ref().map(|w| w.inner_size().width.max(1)).unwrap_or(self.width)
    }
    fn height_px(&self) -> u32 {
        self.window.as_ref().map(|w| w.inner_size().height.max(1)).unwrap_or(self.height)
    }
}

/// Map a winit named key to the engine's editing/navigation key, if applicable.
fn map_named_key(named: &NamedKey) -> Option<SpecialKey> {
    Some(match named {
        NamedKey::Backspace => SpecialKey::Backspace,
        NamedKey::Enter => SpecialKey::Enter,
        NamedKey::Tab => SpecialKey::Tab,
        NamedKey::Escape => SpecialKey::Escape,
        NamedKey::Delete => SpecialKey::Delete,
        NamedKey::ArrowLeft => SpecialKey::Left,
        NamedKey::ArrowRight => SpecialKey::Right,
        NamedKey::ArrowUp => SpecialKey::Up,
        NamedKey::ArrowDown => SpecialKey::Down,
        NamedKey::Home => SpecialKey::Home,
        NamedKey::End => SpecialKey::End,
        NamedKey::PageUp => SpecialKey::PageUp,
        NamedKey::PageDown => SpecialKey::PageDown,
        _ => return None,
    })
}

/// Log an init failure and exit the loop.
fn fail(event_loop: &ActiveEventLoop, msg: String) {
    eprintln!("sentinel: {msg}");
    event_loop.exit();
}
