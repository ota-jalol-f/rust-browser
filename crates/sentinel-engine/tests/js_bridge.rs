//! Verifies the JS↔Rust bridge: a Rust callback installed via JavaScriptCore must
//! be callable from page JavaScript. `#[ignore]` because it needs the Ultralight
//! SDK shared libraries at runtime; run with:
//!   `cargo test -p sentinel-engine --features ultralight -- --ignored`

#![cfg(feature = "ultralight")]

#[test]
#[ignore = "requires Ultralight SDK shared libraries at runtime; run with --ignored"]
fn native_bridge_callback_roundtrips_through_js() {
    let version = sentinel_engine::jsbridge::prove_native_bridge()
        .expect("bridge install + JS callback should succeed when the SDK is present");
    assert_eq!(
        version,
        sentinel_engine::jsbridge::BRIDGE_VERSION,
        "window.native.version must equal the Rust-provided BRIDGE_VERSION, proving \
         the JS call reached Rust and back"
    );
}
