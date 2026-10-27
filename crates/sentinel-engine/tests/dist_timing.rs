//! Diagnostic: load pv-next's built `dist` and measure time-to-mount plus the
//! per-pump CPU render cost. Large `maxUpdateMs`/`avgUpdateMs` ⇒ CPU-render is the
//! heavy-page bottleneck; `mountedMs:-1` ⇒ the page never mounted (a block).
//!
//! Needs the SDK and a static server serving `dist` (e.g. on localhost:8173).
//! Run with:
//!   PROBE_URL=http://localhost:8173/ cargo test -p sentinel-engine --features ultralight --test dist_timing -- --ignored --nocapture

#![cfg(feature = "ultralight")]

use std::time::Duration;

#[test]
#[ignore = "requires Ultralight SDK + a server serving pv-next dist; run with --ignored --nocapture"]
fn dist_mount_and_render_timing() {
    let url = std::env::var("PROBE_URL").unwrap_or_else(|_| "http://localhost:8173/".to_string());
    let secs: u64 = std::env::var("PROBE_SECS").ok().and_then(|s| s.parse().ok()).unwrap_or(25);
    let out = sentinel_engine::headless::probe_dist_timing(&url, Duration::from_secs(secs))
        .expect("probe_dist_timing should run with the SDK present");
    println!("DIST_TIMING {out}");
}
