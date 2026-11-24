//! Engine capability probe: does Ultralight support the JS features pv-next's
//! hardware flows rely on — WebAssembly (faceworksv2), and media capture
//! (`getUserMedia` / `mediaDevices` / WebRTC) for the camera?
//!
//! `#[ignore]` because it needs the Ultralight SDK at runtime. Run with:
//!   cargo test -p sentinel-engine --features ultralight --test engine_capabilities -- --ignored --nocapture

#![cfg(feature = "ultralight")]

use std::time::Duration;

#[test]
#[ignore = "requires Ultralight SDK; run with --ignored --nocapture"]
fn report_media_and_wasm_capabilities() {
    let out = sentinel_engine::headless::probe_capabilities(Duration::from_secs(6))
        .expect("probe_capabilities should run when the SDK is present");
    println!("CAPS {out}");
}
