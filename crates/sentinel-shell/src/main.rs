//! Sentinel browser entry point.
//!
//! With the `ultralight` feature it opens the window, loads the home URL, and runs
//! the event loop. Without the feature (e.g. CI without the SDK) it builds and
//! prints how to enable the GUI, so the binary target always compiles.

#[cfg(feature = "ultralight")]
fn main() -> sentinel_core::Result<()> {
    use sentinel_core::BrowserConfig;

    // Open Google on startup (overriding the about:blank default).
    let config = BrowserConfig {
        home_url: "https://www.google.com".into(),
        ..BrowserConfig::default()
    };
    config.validate()?;

    let mut browser = sentinel_shell::browser::Browser::new(&config)?;
    browser.go(&config.home_url)?;
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
