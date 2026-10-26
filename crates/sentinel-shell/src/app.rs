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
use sentinel_engine::jsbridge::{
    emit_chunk, install_async_bridge, install_ui_bridge, push_ui_update, resolve_call, PendingCall,
    PendingCalls, UiMessages,
};
use sentinel_engine::page::{Engine, PageView, SpecialKey};
use sentinel_plugin::{PluginRegistry, PluginResult, PluginStream, PluginUi, StreamStep, UiStep};
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
    /// Async plugin calls THIS tab's page enqueued (`window.share`). Drained and
    /// resolved only on this tab's own view, so a result can never cross to
    /// another tab or document.
    plugin_pending: PendingCalls,
    /// Document load counter, bumped on each completed load. A stream captures it
    /// at start; a later mismatch means the tab navigated → the stale stream is
    /// dropped before it can deliver to a different document.
    generation: u64,
}

/// A running streaming plugin call: polled once per frame; its chunks and final
/// result are delivered to the originating tab's view.
struct ActiveStream {
    /// Index of the tab whose page made the call (kept valid across tab close).
    tab: usize,
    /// The JS call id to emit/resolve against.
    call_id: f64,
    /// Origin that started the stream; delivery is dropped if the tab's current
    /// document origin no longer matches (cross-origin navigation guard).
    origin: Origin,
    /// Tab generation at start; a bump (new document loaded) means the tab
    /// navigated — including same-origin reloads — so the stream is dropped.
    generation: u64,
    /// The running operation.
    stream: Box<dyn PluginStream>,
}

/// A modal plugin window shown as a full-window, opaque Ultralight HTML overlay.
/// While open, all input is routed to it and it is composited on top of
/// everything. Only one exists at a time (modal).
struct Overlay {
    /// The overlay view (full window).
    view: PageView,
    /// The running UI op driving it.
    plugin: Box<dyn PluginUi>,
    /// Overlay -> op messages (`window.ui.send`), drained each frame.
    messages: UiMessages,
    /// Whether the UI bridge has been installed (after the overlay page loads).
    installed: bool,
    /// The originating call to resolve when the overlay closes, and the document
    /// identity it belongs to (so a navigated-away caller never gets a result).
    tab: usize,
    call_id: f64,
    origin: Origin,
    generation: u64,
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

/// If `request_json` is a modal UI run (`{kind:"run", ui:true, id, request}`),
/// return `(id, request)`; otherwise `None`.
fn parse_ui_run(request_json: &str) -> Option<(String, String)> {
    let value: serde_json::Value = serde_json::from_str(request_json).ok()?;
    let is_ui = value.get("ui").and_then(|s| s.as_bool()).unwrap_or(false);
    if value.get("kind").and_then(|k| k.as_str()) == Some("run") && is_ui {
        let id = value.get("id").and_then(|x| x.as_str()).unwrap_or("").to_string();
        let request = value.get("request").and_then(|x| x.as_str()).unwrap_or("").to_string();
        Some((id, request))
    } else {
        None
    }
}

/// If `request_json` is a streaming run (`{kind:"run", stream:true, id, request}`),
/// return `(id, request)`; otherwise `None` (handled as a one-shot call).
fn parse_stream_run(request_json: &str) -> Option<(String, String)> {
    let value: serde_json::Value = serde_json::from_str(request_json).ok()?;
    let is_stream = value.get("stream").and_then(|s| s.as_bool()).unwrap_or(false);
    if value.get("kind").and_then(|k| k.as_str()) == Some("run") && is_stream {
        let id = value.get("id").and_then(|x| x.as_str()).unwrap_or("").to_string();
        let request = value.get("request").and_then(|x| x.as_str()).unwrap_or("").to_string();
        Some((id, request))
    } else {
        None
    }
}

/// Parse a `window.share` request (`{kind:'run'|'list', id?, request?}`) and
/// service it against `registry`, returning the JSON string to resolve the page
/// Promise with: a `PluginResult` envelope for `run`, or the descriptor array for
/// `list`.
fn dispatch_plugin_request(registry: &mut PluginRegistry, request_json: &str) -> String {
    let value: serde_json::Value = match serde_json::from_str(request_json) {
        Ok(v) => v,
        Err(_) => return PluginResult::fail("bad plugin request").to_json(),
    };
    match value.get("kind").and_then(|k| k.as_str()) {
        Some("list") => {
            serde_json::to_string(&registry.descriptors()).unwrap_or_else(|_| "[]".to_string())
        }
        Some("run") => {
            let id = value.get("id").and_then(|x| x.as_str()).unwrap_or("");
            let request = value.get("request").and_then(|x| x.as_str()).unwrap_or("");
            registry.dispatch(id, request).to_json()
        }
        _ => PluginResult::fail("unknown plugin request kind").to_json(),
    }
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
        registry: crate::plugins::builtin_registry(),
        active_streams: Vec::new(),
        overlay: None,
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
    /// Built-in plugins (`share.RunPlugin` / `GetPlugins` dispatch target).
    registry: PluginRegistry,
    /// Running streaming plugin calls (`share.RunPluginStream`), polled each frame.
    active_streams: Vec<ActiveStream>,
    /// The open modal plugin overlay, if any (`share.RunPluginUi`).
    overlay: Option<Overlay>,
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
        self.tabs = vec![Tab {
            view: content,
            is_home: true,
            loading: true,
            plugin_pending: Arc::new(Mutex::new(Vec::new())),
            generation: 0,
        }];
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
                if let Some(ov) = self.overlay.as_mut() {
                    ov.view.resize(win_w, win_h);
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
                if let Some(ov) = &self.overlay {
                    ov.view.scroll(dx, dy);
                } else if self.cursor.y as i32 >= self.bar_h as i32 {
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
                if let Some(ov) = &self.overlay {
                    ov.view.mouse_move(x, y);
                } else if y < self.bar_h as i32 {
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
                    if let Some(ov) = &self.overlay {
                        ov.view.mouse_button(x, y, pressed);
                    } else if y < self.bar_h as i32 {
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
                // While a modal overlay is open, input is captured by it. Esc always
                // force-closes (cancel) — the host owns the escape hatch.
                if self.overlay.is_some() {
                    let pressed = event.state == ElementState::Pressed;
                    if pressed {
                        if let Key::Named(NamedKey::Escape) = event.logical_key {
                            let r = self.overlay.as_mut().map(|ov| ov.plugin.on_close());
                            if let Some(result) = r {
                                self.close_overlay(Some(result));
                            }
                            return;
                        }
                    }
                    if let Some(ov) = &self.overlay {
                        if let Key::Named(named) = &event.logical_key {
                            if let Some(sk) = map_named_key(named) {
                                ov.view.key_special(sk, pressed);
                            }
                        }
                        if pressed {
                            if let Some(text) = event.text.as_ref() {
                                if !text.chars().all(char::is_control) {
                                    ov.view.key_char(text);
                                }
                            }
                        }
                    }
                    return;
                }
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
        self.process_plugin_calls();
        self.poll_streams();
        self.process_overlay();
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
    /// The current document origin of tab `i`: the home origin only for the
    /// pristine built-in dashboard (still `is_home` and no real URL), otherwise the
    /// view's parsed origin, or an opaque privilege-less origin for
    /// file:/data:/about:blank/unparseable URLs. Shared by bridge install and the
    /// streaming guard so both classify a document identically.
    fn tab_current_origin(&self, i: usize) -> Origin {
        let Some(t) = self.tabs.get(i) else {
            return Origin::opaque();
        };
        let url = t.view.current_url();
        let is_home = t.is_home
            && url.as_deref().is_none_or(|u| u.is_empty() || u.eq_ignore_ascii_case("about:blank"));
        if is_home {
            self.home_origin.clone()
        } else {
            url.as_deref().and_then(|u| Origin::parse(u).ok()).unwrap_or_else(Origin::opaque)
        }
    }

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
        let completed = was_loading && !loading;
        if completed {
            // Classify the loaded document's origin (home only for the pristine
            // dashboard; otherwise its parsed origin, or an opaque privilege-less
            // origin for file:/data:/about:blank/unparseable).
            let origin = self.tab_current_origin(self.active);
            if let (Some(tab), Some(router), Some(executor)) =
                (self.tabs.get(self.active), &self.router, &self.executor)
            {
                let _ = tab.view.install_native_now(router.clone(), executor.clone(), origin.clone());
                // window.share (async plugin bridge): bound to this document's origin
                // and to THIS tab's own pending queue.
                let _ = install_async_bridge(tab.view.view(), origin.clone(), tab.plugin_pending.clone());
            }
            // Re-render the address bar + tab titles for the completed load.
            self.needs_tab_sync = true;
        }
        if let Some(tab) = self.tabs.get_mut(self.active) {
            // A completed load is a NEW document: bump the generation so any stream
            // started under the previous document is dropped on its next poll.
            if completed {
                tab.generation = tab.generation.wrapping_add(1);
            }
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
        let mut navigated = false;
        for cmd in commands {
            // Per-tab navigation acts on the active tab. `nav`/`back`/`forward`
            // leave the dashboard, dropping its home-origin trust (see
            // `install_bridges_if_ready`).
            if let Some(url) = cmd.strip_prefix("nav ") {
                if let Some(tab) = self.tabs.get_mut(self.active) {
                    let _ = tab.view.load_url(&normalize_url(url));
                    tab.is_home = false;
                }
                navigated = true;
            } else if cmd == "back" {
                if let Some(tab) = self.tabs.get_mut(self.active) {
                    tab.view.go_back();
                    tab.is_home = false;
                }
                navigated = true;
            } else if cmd == "forward" {
                if let Some(tab) = self.tabs.get_mut(self.active) {
                    tab.view.go_forward();
                    tab.is_home = false;
                }
                navigated = true;
            } else if cmd == "reload" {
                if let Some(tab) = self.tabs.get(self.active) {
                    tab.view.reload();
                }
                navigated = true;
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
        // A document navigation abandons that document's in-flight plugin calls
        // (its JS context resets); drop the active tab's queue so no stale result
        // is delivered to the new document.
        if navigated {
            if let Some(tab) = self.tabs.get(self.active) {
                tab.plugin_pending.lock().unwrap_or_else(|p| p.into_inner()).clear();
            }
            // Abandon streaming calls for the navigating tab.
            let active = self.active;
            self.active_streams.retain(|s| s.tab != active);
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
        self.tabs.push(Tab {
            view,
            is_home: true,
            loading: true,
            plugin_pending: Arc::new(Mutex::new(Vec::new())),
            generation: 0,
        });
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
        // Abandon streams of the closed tab, and fix up indices of those after it.
        self.active_streams.retain(|s| s.tab != i);
        for s in &mut self.active_streams {
            if s.tab > i {
                s.tab -= 1;
            }
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

    /// Drain and service async plugin calls from `window.share`. Each tab's queue
    /// is drained and resolved ONLY on that tab's own view, so a result can never
    /// cross to another tab; each call is additionally gated by the origin that
    /// made it.
    fn process_plugin_calls(&mut self) {
        // Snapshot (tab index, its calls) so we don't hold a borrow of self.tabs
        // while dispatching (which borrows self.registry).
        let mut work: Vec<(usize, Vec<PendingCall>)> = Vec::new();
        for (i, tab) in self.tabs.iter().enumerate() {
            let calls: Vec<PendingCall> = {
                let mut q = tab.plugin_pending.lock().unwrap_or_else(|p| p.into_inner());
                std::mem::take(&mut *q)
            };
            if !calls.is_empty() {
                work.push((i, calls));
            }
        }
        for (i, calls) in work {
            for c in calls {
                // Gate by the ORIGIN captured at enqueue time.
                let granted = self.router.as_ref().is_some_and(|r| {
                    let g = r.lock().unwrap_or_else(|p| p.into_inner());
                    g.broker().check(&c.origin, Capability::Plugin) == PermissionState::Granted
                });
                if !granted {
                    if let Some(tab) = self.tabs.get(i) {
                        let msg = PluginResult::fail("plugin access not permitted for this origin")
                            .to_json();
                        let _ = resolve_call(tab.view.view(), c.call_id, &msg);
                    }
                    continue;
                }
                // A modal UI call opens an overlay; a streaming call starts a
                // long-running polled operation; everything else resolves one-shot.
                if let Some((id, req)) = parse_ui_run(&c.request) {
                    if self.overlay.is_some() {
                        if let Some(tab) = self.tabs.get(i) {
                            let busy =
                                PluginResult::fail("a plugin window is already open").to_json();
                            let _ = resolve_call(tab.view.view(), c.call_id, &busy);
                        }
                    } else {
                        match self.registry.dispatch_ui(&id, &req) {
                            Ok(ui) => self.open_overlay(i, c.call_id, c.origin.clone(), ui),
                            Err(result) => {
                                if let Some(tab) = self.tabs.get(i) {
                                    let _ =
                                        resolve_call(tab.view.view(), c.call_id, &result.to_json());
                                }
                            }
                        }
                    }
                } else if let Some((id, req)) = parse_stream_run(&c.request) {
                    match self.registry.dispatch_stream(&id, &req) {
                        Ok(stream) => {
                            let generation = self.tabs.get(i).map(|t| t.generation).unwrap_or(0);
                            self.active_streams.push(ActiveStream {
                                tab: i,
                                call_id: c.call_id,
                                origin: c.origin.clone(),
                                generation,
                                stream,
                            });
                        }
                        Err(result) => {
                            if let Some(tab) = self.tabs.get(i) {
                                let _ = resolve_call(tab.view.view(), c.call_id, &result.to_json());
                            }
                        }
                    }
                } else {
                    let result_json = dispatch_plugin_request(&mut self.registry, &c.request);
                    // The JS shim no-ops on an unknown id (e.g. that tab navigated),
                    // so there is no stale or cross-document delivery.
                    if let Some(tab) = self.tabs.get(i) {
                        let _ = resolve_call(tab.view.view(), c.call_id, &result_json);
                    }
                }
            }
        }
    }

    /// Advance each running streaming plugin call by one step: deliver a chunk, or
    /// resolve and drop it when it completes. Polled once per frame so a long
    /// operation never blocks the UI.
    fn poll_streams(&mut self) {
        let mut i = 0;
        while i < self.active_streams.len() {
            let tab_idx = self.active_streams[i].tab;
            // Drop a stream whose tab closed, navigated (generation bumped), or
            // whose current document origin no longer matches the one that started
            // it — never emit/resolve into a different document.
            let stale = match self.tabs.get(tab_idx) {
                None => true,
                Some(t) => {
                    t.generation != self.active_streams[i].generation
                        || self.tab_current_origin(tab_idx) != self.active_streams[i].origin
                }
            };
            if stale {
                self.active_streams.remove(i);
                continue;
            }
            let step = self.active_streams[i].stream.poll();
            let tab = self.active_streams[i].tab;
            let call_id = self.active_streams[i].call_id;
            match step {
                StreamStep::Chunk(chunk) => {
                    if let Some(t) = self.tabs.get(tab) {
                        let _ = emit_chunk(t.view.view(), call_id, &chunk);
                    }
                    i += 1;
                }
                StreamStep::Pending => {
                    i += 1;
                }
                StreamStep::Done(result) => {
                    if let Some(t) = self.tabs.get(tab) {
                        let _ = resolve_call(t.view.view(), call_id, &result.to_json());
                    }
                    self.active_streams.remove(i);
                }
            }
        }
    }

    /// Open a modal overlay for UI op `ui`, created by the call `(tab, call_id,
    /// origin)`. Full-window, opaque; the UI bridge installs once it loads.
    fn open_overlay(&mut self, tab: usize, call_id: f64, origin: Origin, ui: Box<dyn PluginUi>) {
        let win_w = self.width_px();
        let win_h = self.height_px();
        let view = match self.engine.as_ref().and_then(|e| e.create_page(win_w, win_h).ok()) {
            Some(v) => v,
            None => {
                if let Some(t) = self.tabs.get(tab) {
                    let err = PluginResult::fail("overlay unavailable").to_json();
                    let _ = resolve_call(t.view.view(), call_id, &err);
                }
                return;
            }
        };
        let _ = view.load_html(&ui.html());
        let generation = self.tabs.get(tab).map(|t| t.generation).unwrap_or(0);
        self.overlay = Some(Overlay {
            view,
            plugin: ui,
            messages: Arc::new(Mutex::new(Vec::new())),
            installed: false,
            tab,
            call_id,
            origin,
            generation,
        });
    }

    /// Drive the open overlay: install its bridge once loaded, force-close it if
    /// its originating document went away, feed it overlay messages, poll it, push
    /// updates, and resolve + close on completion.
    fn process_overlay(&mut self) {
        // Install the UI bridge once the overlay page has finished loading.
        let ready = match &self.overlay {
            Some(ov) => ov.installed || !ov.view.is_loading(),
            None => return,
        };
        if !ready {
            return;
        }
        if let Some(ov) = self.overlay.as_mut() {
            if !ov.installed {
                let _ = install_ui_bridge(ov.view.view(), ov.messages.clone());
                ov.installed = true;
            }
        }
        // Guard: the originating document must still be the one that opened it.
        let guard_ok = match &self.overlay {
            Some(ov) => {
                self.tabs.get(ov.tab).map(|t| t.generation) == Some(ov.generation)
                    && self.tab_current_origin(ov.tab) == ov.origin
            }
            None => return,
        };
        if !guard_ok {
            // Force-close without delivering: the caller's Promise is gone.
            if let Some(ov) = self.overlay.as_mut() {
                let _ = ov.plugin.on_close();
            }
            self.overlay = None;
            return;
        }
        // Overlay -> op messages.
        if let Some(ov) = self.overlay.as_mut() {
            let msgs: Vec<String> = {
                let mut q = ov.messages.lock().unwrap_or_else(|p| p.into_inner());
                std::mem::take(&mut *q)
            };
            for m in &msgs {
                ov.plugin.on_message(m);
            }
        }
        // Poll the op: push an update, or resolve + close on completion.
        let step = self.overlay.as_mut().map(|ov| ov.plugin.poll());
        match step {
            Some(UiStep::Update(json)) => {
                if let Some(ov) = self.overlay.as_ref() {
                    let _ = push_ui_update(ov.view.view(), &json);
                }
            }
            Some(UiStep::Done(result)) => self.close_overlay(Some(result)),
            _ => {}
        }
    }

    /// Close the overlay, delivering `deliver` to the originating call only if that
    /// document is still the one that opened it.
    fn close_overlay(&mut self, deliver: Option<PluginResult>) {
        let Some(ov) = self.overlay.take() else {
            return;
        };
        if let Some(result) = deliver {
            let still_valid = self.tabs.get(ov.tab).map(|t| t.generation) == Some(ov.generation)
                && self.tab_current_origin(ov.tab) == ov.origin;
            if still_valid {
                if let Some(tab) = self.tabs.get(ov.tab) {
                    let _ = resolve_call(tab.view.view(), ov.call_id, &result.to_json());
                }
            }
        }
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
        // The modal overlay (opaque, full-window) composites on top of everything.
        if let Some(ov) = &self.overlay {
            ov.view.blit_into(&mut buf, win_w, win_h, 0);
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
