//! Sentinel browser entry point.
//!
//! With the `ultralight` feature it opens the window, loads the home URL, and runs
//! the event loop. Without the feature (e.g. CI without the SDK) it builds and
//! prints how to enable the GUI, so the binary target always compiles.

#[cfg(feature = "ultralight")]
fn main() -> sentinel_core::Result<()> {
    use sentinel_core::BrowserConfig;
    use sentinel_shell::dashboard::DASHBOARD_HTML;

    let config = BrowserConfig::default();
    config.validate()?;

    let browser = sentinel_shell::browser::Browser::new(&config)?;
    // Open the built-in dashboard: a visible page proving rendering + the live
    // window.native bridge (device list + JS console) the user can verify by eye.
    browser.show_html(DASHBOARD_HTML)?;
    browser.run(); // blocks until the window closes
    Ok(())
}

#[cfg(not(feature = "ultralight"))]
fn main() {
    eprintln!(
        "sentinel was built without the `ultralight` feature, so the GUI is unavailable.\n\
         Rebuild with:  cargo run -p sentinel-shell --features ultralight\n\
         (requires the Ultralight SDK shared libraries on the path at runtime — see\n\
         crates/sentinel-engine/README.md)."
    );
}
