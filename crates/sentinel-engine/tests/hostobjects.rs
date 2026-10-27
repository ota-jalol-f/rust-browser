//! pv-next compatibility: its plugin bridge reaches the host via
//! `window.chrome.webview.hostObjects.share.RunPlugin/GetPlugins`. This proves
//! that path exists and round-trips through our async bridge.
//!
//! `#[ignore]` (needs the Ultralight SDK). Run with:
//!   cargo test -p sentinel-engine --features ultralight --test hostobjects -- --ignored --nocapture

#![cfg(feature = "ultralight")]

#[test]
#[ignore = "requires Ultralight SDK; run with --ignored --nocapture"]
fn pv_next_hostobjects_share_reaches_bridge() {
    let out = sentinel_engine::jsbridge::prove_hostobjects_shim().expect("shim proof");
    println!("HOSTOBJECTS {out}");
    assert!(out.contains(r#""hasShare":true"#), "hostObjects.share surface missing: {out}");
    // The echoed enqueued request (nested, so JSON-escaped in `result`) proves the
    // call reached the host and resolved back.
    assert!(
        out.contains("Echo") && out.contains("run"),
        "call did not resolve via the host: {out}"
    );
}
