//! Sentinel browser entry point.
//!
//! With the `ultralight` feature it opens a winit window that presents the engine's
//! renderer. Without the feature it builds and prints how to enable the GUI, so the
//! binary target always compiles.

#[cfg(feature = "ultralight")]
fn main() -> sentinel_core::Result<()> {
    use sentinel_core::BrowserConfig;
    use sentinel_shell::app;
    use sentinel_shell::dashboard::{DASHBOARD_HTML, DEVICES_PLACEHOLDER};

    let config = BrowserConfig::default();
    config.validate()?;

    // Enumerate real devices in Rust and bake the snapshot into the dashboard, so
    // the page shows live hardware immediately.
    let devices = app::device_snapshot_json();
    let html = DASHBOARD_HTML.replace(DEVICES_PLACEHOLDER, &devices);

    app::run(&config, html)
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
