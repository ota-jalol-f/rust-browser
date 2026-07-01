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
    startup_log("main: after Browser::new; before content load");

    // Diagnostic content selector to isolate the app.run() crash:
    //   SENTINEL_CONTENT=url     -> load a real URL (proven path)
    //   SENTINEL_CONTENT=minimal -> load_html of a trivial page
    //   (unset)                  -> the full built-in dashboard
    let content = std::env::var("SENTINEL_CONTENT").unwrap_or_default();
    startup_log(&format!("main: content mode = {content:?}"));
    match content.as_str() {
        "url" => browser.open_url("https://example.com")?,
        "minimal" => browser.show_html(
            "<html><body style=\"background:#111;color:#eee;font-family:sans-serif\">\
             <h1>Sentinel minimal OK</h1><p>load_html works.</p></body></html>",
        )?,
        _ => browser.show_html(DASHBOARD_HTML)?,
    }
    startup_log("main: after content load; before app.run()");

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
