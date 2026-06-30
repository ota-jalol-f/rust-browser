//! Verifies the JS↔Rust bridge end to end: a page call to `window.native.usb.list()`
//! must route through the permission gate and, with no grant, report needs-consent.
//! `#[ignore]` because it needs the Ultralight SDK at runtime; run with:
//!   `cargo test -p sentinel-engine --features ultralight -- --ignored`

#![cfg(feature = "ultralight")]

#[test]
#[ignore = "requires Ultralight SDK shared libraries at runtime; run with --ignored"]
fn native_call_routes_through_permission_gate() {
    let result = sentinel_engine::jsbridge::prove_native_dispatch()
        .expect("bridge install + dispatch should succeed when the SDK is present");
    assert_eq!(
        result, "needs_consent:usb",
        "an ungranted origin calling window.native.usb.list() must be gated to needs_consent, \
         proving JS -> Rust dispatch -> permission router -> JSON all executed"
    );
}
