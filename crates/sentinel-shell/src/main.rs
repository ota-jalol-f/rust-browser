//! Sentinel browser entry point.
//!
//! With the `ultralight` feature it opens the window, loads the home URL, and runs
//! the event loop. Without the feature (e.g. CI without the SDK) it builds and
//! prints how to enable the GUI, so the binary target always compiles.

#[cfg(feature = "ultralight")]
fn main() -> sentinel_core::Result<()> {
    use sentinel_core::BrowserConfig;
    use sentinel_shell::browser::startup_log;
    use sentinel_shell::dashboard::DASHBOARD_HTML;

    // Fresh diagnostic log each run (temporary, while isolating the GUI crash).
    let _ = std::fs::remove_file("sentinel-startup.log");
    startup_log("main: start");

    let config = BrowserConfig::default();
    config.validate()?;
    startup_log("main: config validated; before Browser::new");

    let browser = sentinel_shell::browser::Browser::new(&config)?;
    startup_log("main: after Browser::new; before show_html");

    // Enumerate real devices in Rust and bake the snapshot into the dashboard, so
    // the page shows live hardware without the (disabled) JS bridge.
    let devices = browser.device_snapshot_json();
    startup_log("main: device snapshot taken");
    let html = DASHBOARD_HTML.replace(sentinel_shell::dashboard::DEVICES_PLACEHOLDER, &devices);
    browser.show_html(&html)?;
    startup_log("main: after show_html; before app.run()");

    browser.run(); // blocks until the window closes
    startup_log("main: app.run() returned (window closed)");
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
