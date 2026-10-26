//! Streaming plugin bridge: a `share.RunPluginStream(...)` call receives any
//! number of `__sentinelEmit` chunks before the final `__sentinelResolve` — the
//! capability the reference .NET bridge lacked.
//!
//! `#[ignore]` (needs the Ultralight SDK). Run with:
//!   cargo test -p sentinel-engine --features ultralight --test async_stream -- --ignored --nocapture

#![cfg(feature = "ultralight")]

// Exactly one #[test] here — Ultralight's Renderer is once-per-process.
#[test]
#[ignore = "requires Ultralight SDK; run with --ignored"]
fn run_plugin_stream_delivers_chunks_then_final() {
    let out = sentinel_engine::jsbridge::prove_async_stream_roundtrip().expect("stream roundtrip");
    assert!(out.contains("status-1"), "missing first chunk: {out}");
    assert!(out.contains("status-2"), "missing second chunk: {out}");
    assert!(out.contains(r#""final":"done""#), "missing final result: {out}");
}
