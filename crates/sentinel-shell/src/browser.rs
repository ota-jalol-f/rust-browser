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

#[cfg(feature = "ultralight")]
pub use gui::Browser;

#[cfg(feature = "ultralight")]
mod gui {
    use super::normalize_url;
    use sentinel_core::{BrowserConfig, Error, History, Result};
    use ul_next::{
        app::App,
        window::{Window, WindowFlags},
        overlay::Overlay,
        Library,
    };

    /// A single-window, single-overlay browser driven by a [`History`] model.
    ///
    /// Owns the Ultralight objects for their whole lifetime: dropping `App` tears
    /// down the renderer and run loop, so all three are kept as fields.
    pub struct Browser {
        // Field order matters for drop: overlay before window before app.
        overlay: Overlay,
        // Held only to keep the native window alive; dropping it closes the
        // window. Not read after construction, hence the allow.
        #[allow(dead_code)]
        window: Window,
        app: App,
        history: History,
    }

    impl Browser {
        /// Load Ultralight (+AppCore) at runtime, create the window and a
        /// full-window web overlay sized from `config`.
        ///
        /// # Errors
        /// [`Error::Engine`] if the libraries cannot be loaded or any Ultralight
        /// object fails to construct.
        pub fn new(config: &BrowserConfig) -> Result<Self> {
            // Make the executable's own directory the working directory so AppCore
            // finds the SDK `resources/` folder (staged next to the exe by build.rs)
            // regardless of where the browser was launched from.
            if let Ok(exe) = std::env::current_exe() {
                if let Some(dir) = exe.parent() {
                    let _ = std::env::set_current_dir(dir);
                }
            }

            // Linked mode: Ultralight + AppCore symbols are resolved at build time
            // against ul-next-sys's import library (the runtime GetProcAddress path
            // fails on this SDK build). The DLLs are staged next to the exe.
            let lib = Library::linked();

            let app = App::new(lib.clone(), None, None)
                .map_err(|e| Error::Engine(format!("app create: {e:?}")))?;

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

            let overlay = window
                .create_overlay(window.width(), window.height(), 0, 0)
                .ok_or_else(|| Error::Engine("create_overlay returned None".into()))?;

            Ok(Self { overlay, window, app, history: History::new() })
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
