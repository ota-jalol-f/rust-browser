//! Overlay UI plugin bridge: input capture (overlay -> host via `window.ui.send`)
//! and host -> overlay updates (`window.ui.onUpdate`). Proves a synthetic click
//! reaches `onclick` and that a pushed update updates the DOM.
//!
//! `#[ignore]` (needs the Ultralight SDK). Run with:
//!   cargo test -p sentinel-engine --features ultralight --test ui_bridge -- --ignored --nocapture

#![cfg(feature = "ultralight")]

// Exactly one #[test] here — Ultralight's Renderer is once-per-process.
#[test]
#[ignore = "requires Ultralight SDK; run with --ignored --nocapture"]
fn ui_bridge_click_and_update_round_trip() {
    let out = sentinel_engine::jsbridge::prove_ui_bridge_click().expect("ui bridge proof");
    println!("UI_BRIDGE {out}");
    // overlay -> host: the click reached onclick and called window.ui.send('ok').
    assert!(out.contains(r#""ok""#), "synthetic click did not reach onclick/send: {out}");
    // host -> overlay: the pushed update set the DOM text.
    assert!(out.contains(r#""status":"ready""#), "host->overlay update failed: {out}");
}
