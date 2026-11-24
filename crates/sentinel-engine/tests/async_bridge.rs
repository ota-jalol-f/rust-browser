//! Async plugin bridge: the callId -> __sentinelResolve round trip. A page JS
//! `share.RunPlugin(...)` Promise is settled later by the host.
//!
//! `#[ignore]` (needs the Ultralight SDK). Run with:
//!   cargo test -p sentinel-engine --features ultralight --test async_bridge -- --ignored --nocapture

#![cfg(feature = "ultralight")]

// Exactly one #[test] here — Ultralight's Renderer is once-per-process.
#[test]
#[ignore = "requires Ultralight SDK; run with --ignored"]
fn run_plugin_promise_resolves_from_host() {
    // The proof echoes the enqueued request back as the resolved value, so the
    // Promise observes the shim's wrapped request JSON.
    let out = sentinel_engine::jsbridge::prove_async_roundtrip("hello").expect("roundtrip");
    assert!(out.contains(r#""kind":"run""#), "missing kind: {out}");
    assert!(out.contains(r#""id":"Echo""#), "missing id: {out}");
    assert!(out.contains(r#""request":"hello""#), "missing request: {out}");
}
