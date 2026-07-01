//! The windowed browser: a winit window that presents the engine's CPU-rendered
//! page via softbuffer, and forwards input to it.
//!
//! This replaces the AppCore path (whose overlay views crash on the JS-bridge
//! callback). Here the shell owns the window and drives a [`PageView`], so the
//! `window.native` bridge — which works on low-level Renderer views — can be
//! installed later. Compiled only with the `ultralight` feature.
//!
//! Stage 1 scope: create the window, present the page every frame, forward mouse
//! move/click, and quit on close. Keyboard, the live bridge, and the address-bar
//! chrome build on top of this.

use std::num::NonZeroU32;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use sentinel_bridge::BridgeRouter;
use sentinel_core::{BrowserConfig, Error, Result};
use sentinel_device_host::{DeviceHost, SystemBackend};
use sentinel_dll_host::DllHost;
use sentinel_engine::page::PageView;
use sentinel_hosts::HostRouter;
use sentinel_permissions::{Capability, InMemoryBroker, Origin, PermissionBroker, PermissionState};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition};
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowId};

/// Shared permission router (broker is `Ask` by default).
type Router = Arc<Mutex<BridgeRouter<InMemoryBroker>>>;
/// Shared combined host executor (USB/serial via device host, DLL via DLL host).
type Executor = Arc<Mutex<HostRouter<SystemBackend>>>;

/// Origin used for the built-in dashboard (no real URL); pre-granted every
/// capability so its console can call `window.native.*` live.
const BUILTIN_ORIGIN: &str = "sentinel://home";

/// Enumerate USB + serial devices in Rust and return a JSON snapshot
/// `{"usb": <Response>, "serial": <Response>}` for injection into the dashboard —
/// so the page shows live hardware without needing the JS bridge.
pub fn device_snapshot_json() -> String {
    use sentinel_device_host::{DeviceHost, SystemBackend};
    use sentinel_dll_host::DllHost;
    use sentinel_hosts::HostRouter;
    use sentinel_ipc::{Request, RequestExecutor};

    let mut hosts = HostRouter::new(DeviceHost::new(SystemBackend), DllHost::new());
    let usb = hosts.execute(Request::UsbList);
    let serial = hosts.execute(Request::SerialList);
    let usb_json = serde_json::to_string(&usb).unwrap_or_else(|_| "null".into());
    let serial_json = serde_json::to_string(&serial).unwrap_or_else(|_| "null".into());
    format!("{{\"usb\":{usb_json},\"serial\":{serial_json}}}")
}

/// Run the browser event loop, showing `html`. Blocks until the window closes.
///
/// # Errors
/// [`Error::Engine`] if the event loop or window cannot be created.
pub fn run(config: &BrowserConfig, html: String) -> Result<()> {
    let event_loop =
        EventLoop::new().map_err(|e| Error::Engine(format!("winit event loop: {e}")))?;
    let mut app = App {
        html,
        width: config.window_width,
        height: config.window_height,
        window: None,
        surface: None,
        page: None,
        cursor: PhysicalPosition::new(0.0, 0.0),
        router: None,
        executor: None,
    };
    event_loop
        .run_app(&mut app)
        .map_err(|e| Error::Engine(format!("winit run: {e}")))
}

struct App {
    html: String,
    width: u32,
    height: u32,
    window: Option<Rc<Window>>,
    surface: Option<softbuffer::Surface<Rc<Window>, Rc<Window>>>,
    page: Option<PageView>,
    cursor: PhysicalPosition<f64>,
    // Kept alive for the page callback / future consent UI.
    router: Option<Router>,
    executor: Option<Executor>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return; // already initialized
        }
        let attrs = Window::default_attributes()
            .with_title("Sentinel")
            .with_inner_size(LogicalSize::new(self.width, self.height))
            .with_resizable(false);
        let window = match event_loop.create_window(attrs) {
            Ok(w) => Rc::new(w),
            Err(e) => {
                eprintln!("failed to create window: {e}");
                event_loop.exit();
                return;
            }
        };

        // softbuffer presents our CPU pixel buffer to the window.
        let context = match softbuffer::Context::new(window.clone()) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("softbuffer context: {e}");
                event_loop.exit();
                return;
            }
        };
        let surface = match softbuffer::Surface::new(&context, window.clone()) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("softbuffer surface: {e}");
                event_loop.exit();
                return;
            }
        };

        // Render at the physical pixel size so the page fills the window crisply.
        let size = window.inner_size();
        let (pw, ph) = (size.width.max(1), size.height.max(1));
        let page = match PageView::new(pw, ph) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("engine page: {e}");
                event_loop.exit();
                return;
            }
        };
        if let Err(e) = page.load_html(&self.html) {
            eprintln!("load_html: {e}");
        }

        // Install the live window.native bridge on each page's DOM-ready. This is
        // the payoff of the Renderer path: the callback + JS install work here.
        let router: Router = Arc::new(Mutex::new(BridgeRouter::new(InMemoryBroker::new())));
        let executor: Executor =
            Arc::new(Mutex::new(HostRouter::new(DeviceHost::new(SystemBackend), DllHost::new())));
        let fallback = Origin::parse(BUILTIN_ORIGIN).expect("BUILTIN_ORIGIN is valid");
        {
            let mut g = router.lock().unwrap_or_else(|p| p.into_inner());
            for cap in Capability::ALL {
                g.broker_mut().set(&fallback, cap, PermissionState::Granted);
            }
        }
        page.install_bridge(router.clone(), executor.clone(), fallback);

        self.window = Some(window);
        self.surface = Some(surface);
        self.page = Some(page);
        self.router = Some(router);
        self.executor = Some(executor);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::RedrawRequested => self.paint(),
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = position;
                if let Some(page) = &self.page {
                    page.mouse_move(position.x as i32, position.y as i32);
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if button == MouseButton::Left {
                    if let Some(page) = &self.page {
                        page.mouse_button(
                            self.cursor.x as i32,
                            self.cursor.y as i32,
                            state == ElementState::Pressed,
                        );
                    }
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        // Drive continuous rendering (the page loads/animates over many frames).
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}

impl App {
    fn paint(&mut self) {
        let (Some(page), Some(surface)) = (&self.page, &mut self.surface) else {
            return;
        };
        page.update();

        let Some((w, h, row_bytes, bgra)) = page.snapshot_bgra() else {
            return;
        };
        let (Some(nw), Some(nh)) = (NonZeroU32::new(w), NonZeroU32::new(h)) else {
            return;
        };
        if surface.resize(nw, nh).is_err() {
            return;
        }
        let Ok(mut buf) = surface.buffer_mut() else {
            return;
        };

        // Convert premultiplied BGRA rows to softbuffer's 0x00RRGGBB u32 pixels.
        for y in 0..h {
            let row = (y * row_bytes) as usize;
            for x in 0..w {
                let i = row + (x * 4) as usize;
                if i + 2 >= bgra.len() {
                    continue;
                }
                let b = bgra[i] as u32;
                let g = bgra[i + 1] as u32;
                let r = bgra[i + 2] as u32;
                buf[(y * w + x) as usize] = (r << 16) | (g << 8) | b;
            }
        }
        let _ = buf.present();
    }
}
