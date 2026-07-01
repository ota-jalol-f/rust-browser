//! Browser window controller.
//!
//! Two layers live here:
//! - [`normalize_url`] — pure address-bar input normalization, always compiled and
//!   unit-tested (no SDK needed).
//! - [`Browser`] — the windowed controller that owns the Ultralight `App`/`Window`/
//!   `Overlay` and drives them from the pure-Rust [`History`](sentinel_core::History)
//!   model. Compiled only with the `ultralight` feature; GUI behavior is verified
//!   manually on a machine with a display + the SDK.

/// Normalize raw address-bar input into a loadable URL.
///
/// - Empty/whitespace → `"about:blank"`.
/// - Already has a recognized scheme (`http:`, `https:`, `file:`, `about:`) → unchanged.
/// - Otherwise → prefixed with `https://` (privacy-first: never silently downgrade
///   a bare host to plaintext `http`).
pub fn normalize_url(input: &str) -> String {
    let s = input.trim();
    if s.is_empty() {
        return "about:blank".into();
    }
    let has_scheme = ["http:", "https:", "file:", "about:"]
        .iter()
        .any(|p| s.starts_with(p));
    if has_scheme {
        s.to_string()
    } else {
        format!("https://{s}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bare_host_gets_https() {
        assert_eq!(normalize_url("example.com"), "https://example.com");
    }

    #[test]
    fn known_schemes_preserved() {
        assert_eq!(normalize_url("http://x.test"), "http://x.test");
        assert_eq!(normalize_url("https://x.test"), "https://x.test");
        assert_eq!(normalize_url("about:blank"), "about:blank");
        assert_eq!(normalize_url("file:///c:/page.html"), "file:///c:/page.html");
    }

    #[test]
    fn empty_or_whitespace_is_blank() {
        assert_eq!(normalize_url(""), "about:blank");
        assert_eq!(normalize_url("   "), "about:blank");
    }

    #[test]
    fn surrounding_whitespace_trimmed() {
        assert_eq!(normalize_url("  example.com  "), "https://example.com");
    }
}

/// Diagnostic (temporary): append a startup step to `sentinel-startup.log` in the
/// CWD, flushed immediately, so that if the NEXT step crashes natively the last
/// logged line pinpoints exactly which setup call failed. Always active while we
/// isolate the GUI crash; remove once resolved.
pub fn startup_log(msg: &str) {
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("sentinel-startup.log")
    {
        let _ = writeln!(f, "{msg}");
        let _ = f.flush();
    }
}

#[cfg(feature = "ultralight")]
pub use gui::Browser;

#[cfg(feature = "ultralight")]
mod gui {
    use std::sync::{Arc, Mutex};

    use super::normalize_url;
    use sentinel_bridge::BridgeRouter;
    use sentinel_core::{BrowserConfig, Error, History, Result};
    use sentinel_device_host::{DeviceHost, SystemBackend};
    use sentinel_dll_host::DllHost;
    use sentinel_engine::jsbridge;
    use sentinel_hosts::HostRouter;
    use sentinel_permissions::{Capability, InMemoryBroker, Origin, PermissionBroker, PermissionState};
    use ul_next::{
        app::{App, Settings},
        window::{Window, WindowFlags},
        overlay::Overlay,
        Library,
    };

    /// Shared permission router (broker is `Ask` by default).
    type Router = Arc<Mutex<BridgeRouter<InMemoryBroker>>>;
    /// Shared combined host executor (USB/serial via device host, DLL via DLL host).
    type Executor = Arc<Mutex<HostRouter<SystemBackend>>>;

    /// Origin assigned to the built-in dashboard (which has no real URL). It is
    /// pre-granted every capability so the dashboard can show live device data.
    const BUILTIN_ORIGIN: &str = "sentinel://home";

    /// Append a line to `sentinel-bridge-debug.log` (CWD), flushed, for isolating
    /// the AppCore bridge-install crash. Only active when SENTINEL_BRIDGE is set
    /// (the same gate that enables the install), so normal runs write nothing.
    fn log_debug(msg: &str) {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open("sentinel-bridge-debug.log")
        {
            let _ = writeln!(f, "shell: {msg}");
            let _ = f.flush();
        }
    }

    /// A single-window, single-overlay browser driven by a [`History`] model, with
    /// the `window.native` bridge installed on every page's DOM-ready.
    ///
    /// Owns the Ultralight objects for their whole lifetime: dropping `App` tears
    /// down the renderer and run loop, so all are kept as fields.
    pub struct Browser {
        // Field order matters for drop: overlay before window before app.
        overlay: Overlay,
        // Held only to keep the native window alive; dropping it closes the
        // window. Not read after construction, hence the allow.
        #[allow(dead_code)]
        window: Window,
        app: App,
        history: History,
        // Permission router + host executor, shared with the page callback and kept
        // for a future consent UI / settings view.
        #[allow(dead_code)]
        router: Router,
        #[allow(dead_code)]
        executor: Executor,
    }

    impl Browser {
        /// Load Ultralight (+AppCore) at runtime, create the window and a
        /// full-window web overlay sized from `config`.
        ///
        /// # Errors
        /// [`Error::Engine`] if the libraries cannot be loaded or any Ultralight
        /// object fails to construct.
        pub fn new(config: &BrowserConfig) -> Result<Self> {
            super::startup_log("new: before Library::linked");
            // Linked mode: Ultralight + AppCore symbols are resolved at build time
            // against ul-next-sys's import library (the runtime GetProcAddress path
            // fails on this SDK build). The DLLs are staged next to the exe.
            let lib = Library::linked();
            super::startup_log("new: after Library::linked");

            // AppCore resolves `filesystem_path` RELATIVE TO THE EXECUTABLE on
            // Windows, so "." points at the exe directory — where build.rs stages
            // the SDK `resources/` folder (icudt67l.dat, cacert.pem). This is
            // CWD-independent: no need to change the process working directory.
            // Use AppCore's default GPU painting (forcing the CPU renderer here
            // crashes AppCore's overlay painting — AppCore manages GPU painting for
            // overlays itself, per its docs/examples).
            let settings = Settings::start()
                .app_name("Sentinel")
                .filesystem_path(".")
                .build(lib.clone())
                .ok_or_else(|| Error::Engine("settings build returned None".into()))?;

            super::startup_log("new: after Settings::build; before App::new");
            let app = App::new(lib.clone(), Some(settings), None)
                .map_err(|e| Error::Engine(format!("app create: {e:?}")))?;
            super::startup_log("new: after App::new; before create_window");

            let window = app
                .create_window(
                    config.window_width,
                    config.window_height,
                    false,
                    WindowFlags {
                        borderless: false,
                        titled: true,
                        resizable: true,
                        maximizable: true,
                        hidden: false,
                    },
                )
                .ok_or_else(|| Error::Engine("create_window returned None".into()))?;
            super::startup_log("new: after create_window; before create_overlay");

            let overlay = window
                .create_overlay(window.width(), window.height(), 0, 0)
                .ok_or_else(|| Error::Engine("create_overlay returned None".into()))?;
            super::startup_log("new: after create_overlay");

            // Permission router + combined host executor (USB/serial + DLL). The
            // built-in dashboard origin is pre-granted so it can show live data.
            let router: Router = Arc::new(Mutex::new(BridgeRouter::new(InMemoryBroker::new())));
            let executor: Executor =
                Arc::new(Mutex::new(HostRouter::new(DeviceHost::new(SystemBackend), DllHost::new())));
            if let Ok(builtin) = Origin::parse(BUILTIN_ORIGIN) {
                let mut g = router.lock().unwrap_or_else(|p| p.into_inner());
                for cap in Capability::ALL {
                    g.broker_mut().set(&builtin, cap, PermissionState::Granted);
                }
            }

            // Install `window.native` on every page's DOM-ready — but ONLY when
            // opted in via SENTINEL_BRIDGE, because doing so on an AppCore overlay
            // view currently causes a native crash we are still isolating. Off by
            // default, the browser runs fine (dashboard renders, JS console works);
            // window.native is proven by the headless engine tests. With the env var
            // set, install_native writes step markers to sentinel-bridge-debug.log so
            // the exact crashing native call can be identified.
            // DIAGNOSTIC: hard-disabled (`false &&`) to remove the env-var confound —
            // the user's SENTINEL_BRIDGE persists across shells. If the browser now
            // works, the DOM-ready bridge install is confirmed as the crash. Re-enable
            // by restoring the env check once the install path is fixed.
            if false && std::env::var_os("SENTINEL_BRIDGE").is_some() {
                let router = router.clone();
                let executor = executor.clone();
                overlay.view().set_dom_ready_callback(move |view, _frame_id, is_main_frame, url| {
                    if !is_main_frame {
                        return;
                    }
                    log_debug(&format!("dom_ready fired (main frame), url={url:?}"));
                    // Real pages use their own origin (Ask by default); the built-in
                    // dashboard has no URL, so it falls back to the trusted origin.
                    let origin = Origin::parse(&url)
                        .or_else(|_| Origin::parse(BUILTIN_ORIGIN))
                        .expect("BUILTIN_ORIGIN is a valid origin");
                    let router = router.clone();
                    let executor = executor.clone();
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        jsbridge::install_native(view, router, executor, origin)
                    }));
                    match result {
                        Ok(Ok(())) => log_debug("install_native returned Ok"),
                        Ok(Err(e)) => log_debug(&format!("install_native error: {e}")),
                        Err(_) => log_debug("install_native panicked (contained)"),
                    }
                });
            }

            super::startup_log("new: returning Browser (bridge registered or skipped)");
            Ok(Self { overlay, window, app, history: History::new(), router, executor })
        }

        /// Display a self-contained HTML string (used for the built-in dashboard).
        ///
        /// # Errors
        /// [`Error::Engine`] if the view rejects the HTML.
        pub fn show_html(&self, html: &str) -> Result<()> {
            self.overlay
                .view()
                .load_html(html)
                .map_err(|e| Error::Engine(format!("load_html failed: {e:?}")))
        }

        /// Load a URL directly (no history), for diagnostics/startup.
        ///
        /// # Errors
        /// [`Error::Engine`] if the view rejects the URL.
        #[allow(dead_code)]
        pub fn open_url(&self, url: &str) -> Result<()> {
            self.overlay
                .view()
                .load_url(url)
                .map_err(|e| Error::Engine(format!("load_url failed: {e:?}")))
        }

        /// Navigate to `input` (normalized), recording it in history.
        ///
        /// # Errors
        /// [`Error::Engine`] if the view fails to start loading the URL.
        pub fn go(&mut self, input: &str) -> Result<()> {
            let url = normalize_url(input);
            self.load(&url)?;
            self.history.navigate(url);
            Ok(())
        }

        /// Go back one entry and load it.
        ///
        /// # Errors
        /// [`Error::Navigation`] if there is no back entry; [`Error::Engine`] on load failure.
        pub fn back(&mut self) -> Result<()> {
            let url = self.history.back()?.to_string();
            self.load(&url)
        }

        /// Go forward one entry and load it.
        ///
        /// # Errors
        /// [`Error::Navigation`] if there is no forward entry; [`Error::Engine`] on load failure.
        pub fn forward(&mut self) -> Result<()> {
            let url = self.history.forward()?.to_string();
            self.load(&url)
        }

        /// Reload the current entry, if any.
        ///
        /// # Errors
        /// [`Error::Engine`] on load failure. A no-op (Ok) when history is empty.
        pub fn reload(&mut self) -> Result<()> {
            if let Some(url) = self.history.current() {
                let url = url.to_string();
                return self.load(&url);
            }
            Ok(())
        }

        /// Run the application event loop. Blocks until the window is closed.
        pub fn run(&self) {
            self.app.run();
        }

        /// Tell the overlay's view to load `url`.
        fn load(&self, url: &str) -> Result<()> {
            self.overlay
                .view()
                .load_url(url)
                .map_err(|e| Error::Engine(format!("load_url({url}): {e:?}")))
        }
    }
}
