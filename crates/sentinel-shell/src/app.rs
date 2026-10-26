//! The windowed browser: a winit window presenting two composited Ultralight
//! views — a chrome strip (address bar) on top and the page content below — with
//! input routed by position/focus and navigation driven from the chrome.
//!
//! This replaces AppCore (whose overlay views crash on the JS-bridge callback).
//! Compiled only with the `ultralight` feature.

use std::num::NonZeroU32;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use sentinel_bridge::BridgeRouter;
use sentinel_core::{BrowserConfig, Error, Result};
use sentinel_device_host::{DeviceHost, SystemBackend};
use sentinel_dll_host::DllHost;
use sentinel_engine::page::{Engine, PageView, SpecialKey};
use sentinel_hosts::HostRouter;
use sentinel_permissions::{Capability, InMemoryBroker, Origin, PermissionBroker, PermissionState};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition};
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

/// Pixels scrolled per mouse-wheel line notch.
const SCROLL_LINE_PX: f32 = 48.0;
/// Target frame interval (~60 fps).
const FRAME: Duration = Duration::from_millis(16);

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

/// One browser tab: its content view plus the per-tab state the shell tracks.
struct Tab {
    view: PageView,
    /// Whether this tab still shows the built-in dashboard (home-origin trust).
    is_home: bool,
    /// Previous-frame loading state, to detect load completion.
    loading: bool,
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

/// A short tab label from the view's current URL (host), or "Dashboard" when the
/// view has no real URL (the built-in `load_html` start page).
fn tab_title(view: &PageView) -> String {
    match view.current_url() {
        Some(u) if !u.is_empty() && !u.eq_ignore_ascii_case("about:blank") => {
            let host = u
                .strip_prefix("https://")
                .or_else(|| u.strip_prefix("http://"))
                .unwrap_or(u.as_str());
            host.split('/').next().unwrap_or(host).to_string()
        }
        _ => "Dashboard".to_string(),
    }
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
        tabs: Vec::new(),
        active: 0,
        bar_h: BAR_HEIGHT as u32,
        cursor: PhysicalPosition::new(0.0, 0.0),
        focus: Focus::Content,
        nav_queue: Arc::new(Mutex::new(Vec::new())),
        router: None,
        executor: None,
        home_origin: Origin::parse(BUILTIN_ORIGIN).expect("valid"),
        chrome_installed: false,
        needs_tab_sync: true,
        last_frame: Instant::now(),
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
    /// Open tabs; each owns its own content view. `active` indexes the shown one.
    tabs: Vec<Tab>,
    active: usize,
    bar_h: u32,
    cursor: PhysicalPosition<f64>,
    focus: Focus,
    nav_queue: Arc<Mutex<Vec<String>>>,
    router: Option<Router>,
    executor: Option<Executor>,
    home_origin: Origin,
    /// Whether the chrome's nav bridge has been installed (chrome loads once).
    chrome_installed: bool,
    /// Set when the tab strip / address bar in the chrome needs re-rendering
    /// (tab added/closed/selected, or the active tab finished loading). Flushed
    /// once per frame in `about_to_wait`.
    needs_tab_sync: bool,
    /// Time of the last painted frame (for ~60 fps pacing).
    last_frame: Instant,
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
            .with_inner_size(LogicalSize::new(self.width, self.height));
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
        self.tabs = vec![Tab { view: content, is_home: true, loading: true }];
        self.active = 0;
        self.router = Some(router);
        self.executor = Some(executor);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::RedrawRequested => self.paint(),
            WindowEvent::Resized(size) => {
                // Keep both Ultralight views in step with the window; the softbuffer
                // surface is already resized each frame in `paint`. The chrome keeps
                // its fixed bar height; the content view takes the remaining area.
                let win_w = size.width.max(1);
                let win_h = size.height.max(1);
                let content_h = win_h.saturating_sub(self.bar_h).max(1);
                if let Some(c) = self.chrome.as_mut() {
                    c.resize(win_w, self.bar_h);
                }
                if let Some(t) = self.tabs.get_mut(self.active) {
                    t.view.resize(win_w, content_h);
                }
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let (dx, dy) = match delta {
                    MouseScrollDelta::LineDelta(x, y) => {
                        ((x * SCROLL_LINE_PX) as i32, (y * SCROLL_LINE_PX) as i32)
                    }
                    MouseScrollDelta::PixelDelta(p) => (p.x as i32, p.y as i32),
                };
                if self.cursor.y as i32 >= self.bar_h as i32 {
                    if let Some(t) = self.tabs.get(self.active) {
                        t.view.scroll(dx, dy);
                    }
                } else if let Some(c) = &self.chrome {
                    c.scroll(dx, dy);
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = position;
                let (x, y) = (position.x as i32, position.y as i32);
                if y < self.bar_h as i32 {
                    if let Some(c) = &self.chrome {
                        c.mouse_move(x, y);
                    }
                } else if let Some(t) = self.tabs.get(self.active) {
                    t.view.mouse_move(x, y - self.bar_h as i32);
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
                        if let Some(t) = self.tabs.get(self.active) {
                            t.view.mouse_button(x, y - self.bar_h as i32, pressed);
                        }
                    }
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let target = match self.focus {
                    Focus::Chrome => self.chrome.as_ref(),
                    Focus::Content => self.tabs.get(self.active).map(|t| &t.view),
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

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.apply_nav_commands();
        self.install_bridges_if_ready();
        if self.chrome_installed && self.needs_tab_sync {
            self.sync_tabs_to_chrome();
            self.needs_tab_sync = false;
        }
        // Continuous rendering paced to ~60 fps (smooth hover/scroll/animation
        // without spinning the CPU unbounded).
        if let Some(window) = &self.window {
            window.request_redraw();
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(self.last_frame + FRAME));
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
                    self.needs_tab_sync = true;
                }
            }
        }

        // On the active tab's load-completion: (re)install window.native for the
        // new page's origin, and sync the address bar to the actual URL.
        let loading = self.tabs.get(self.active).is_some_and(|t| t.view.is_loading());
        let was_loading = self.tabs.get(self.active).is_some_and(|t| t.loading);
        if was_loading && !loading {
            let url = self.tabs.get(self.active).and_then(|t| t.view.current_url());
            // The home origin's pre-granted capabilities are reserved for the
            // built-in startup dashboard ONLY: the tab must still be `is_home`
            // (never navigated) AND the view must have no real URL (the load_html
            // dashboard reports none / about:blank). Any page with a real URL is
            // classified by that URL regardless of how it was reached (address bar,
            // link click, or a script `location` change): its own parsed origin, or
            // an opaque privilege-less origin for file:/data:/unparseable URLs. So
            // home privileges can never leak to a navigated page.
            let is_home = self.tabs.get(self.active).is_some_and(|t| t.is_home)
                && url
                    .as_deref()
                    .is_none_or(|u| u.is_empty() || u.eq_ignore_ascii_case("about:blank"));
            let origin = if is_home {
                self.home_origin.clone()
            } else {
                url.as_deref()
                    .and_then(|u| Origin::parse(u).ok())
                    .unwrap_or_else(Origin::opaque)
            };
            if let (Some(tab), Some(router), Some(executor)) =
                (self.tabs.get(self.active), &self.router, &self.executor)
            {
                let _ = tab.view.install_native_now(router.clone(), executor.clone(), origin);
            }
            // Re-render the address bar + tab titles for the completed load.
            self.needs_tab_sync = true;
        }
        if let Some(tab) = self.tabs.get_mut(self.active) {
            tab.loading = loading;
        }
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
        for cmd in commands {
            // Per-tab navigation acts on the active tab. `nav`/`back`/`forward`
            // leave the dashboard, dropping its home-origin trust (see
            // `install_bridges_if_ready`).
            if let Some(url) = cmd.strip_prefix("nav ") {
                if let Some(tab) = self.tabs.get_mut(self.active) {
                    let _ = tab.view.load_url(&normalize_url(url));
                    tab.is_home = false;
                }
            } else if cmd == "back" {
                if let Some(tab) = self.tabs.get_mut(self.active) {
                    tab.view.go_back();
                    tab.is_home = false;
                }
            } else if cmd == "forward" {
                if let Some(tab) = self.tabs.get_mut(self.active) {
                    tab.view.go_forward();
                    tab.is_home = false;
                }
            } else if cmd == "reload" {
                if let Some(tab) = self.tabs.get(self.active) {
                    tab.view.reload();
                }
            } else if cmd == "newtab" {
                self.open_tab();
                self.needs_tab_sync = true;
            } else if let Some(rest) = cmd.strip_prefix("selecttab ") {
                if let Ok(i) = rest.trim().parse::<usize>() {
                    if i < self.tabs.len() {
                        self.active = i;
                        self.needs_tab_sync = true;
                    }
                }
            } else if let Some(rest) = cmd.strip_prefix("closetab ") {
                if let Ok(i) = rest.trim().parse::<usize>() {
                    self.close_tab(i);
                    self.needs_tab_sync = true;
                }
            }
        }
    }

    /// Open a new tab showing the built-in dashboard and make it active.
    fn open_tab(&mut self) {
        let win_w = self.width_px();
        let content_h = self.height_px().saturating_sub(self.bar_h).max(1);
        let view = match self.engine.as_ref().and_then(|e| e.create_page(win_w, content_h).ok()) {
            Some(v) => v,
            None => return,
        };
        let _ = view.load_html(&self.home_html);
        self.tabs.push(Tab { view, is_home: true, loading: true });
        self.active = self.tabs.len() - 1;
    }

    /// Close the tab at `i`, keeping at least one tab open and fixing `active`.
    fn close_tab(&mut self, i: usize) {
        if i >= self.tabs.len() || self.tabs.len() <= 1 {
            return;
        }
        self.tabs.remove(i);
        if self.active > i {
            self.active -= 1;
        } else if self.active >= self.tabs.len() {
            self.active = self.tabs.len() - 1;
        }
    }

    /// Push the tab list (titles + active index) and the active URL to the chrome.
    fn sync_tabs_to_chrome(&self) {
        let Some(chrome) = &self.chrome else {
            return;
        };
        let items: Vec<String> = self
            .tabs
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let title =
                    serde_json::to_string(&tab_title(&t.view)).unwrap_or_else(|_| "\"\"".into());
                format!("{{\"title\":{title},\"active\":{}}}", i == self.active)
            })
            .collect();
        chrome.eval(&format!("window.setTabs && window.setTabs([{}])", items.join(",")));
        let url = self.tabs.get(self.active).and_then(|t| t.view.current_url()).unwrap_or_default();
        let url_json = serde_json::to_string(&url).unwrap_or_else(|_| "\"\"".into());
        chrome.eval(&format!("window.setUrl && window.setUrl({url_json})"));
    }

    fn paint(&mut self) {
        self.last_frame = Instant::now();
        let win_w = self.width_px();
        let win_h = self.height_px();
        let bar_h = self.bar_h;
        let (Some(engine), Some(chrome), Some(surface)) =
            (&self.engine, &self.chrome, &mut self.surface)
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
        if let Some(tab) = self.tabs.get(self.active) {
            tab.view.blit_into(&mut buf, win_w, win_h, bar_h);
        }
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
