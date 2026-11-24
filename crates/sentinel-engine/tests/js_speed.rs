//! Diagnostic: how fast is Ultralight's JavaScript? Times a 50M-iteration loop.
//! Tens of ms ⇒ a JIT is present; hundreds/thousands ⇒ interpreter-only.
//!
//! `#[ignore]` (needs the Ultralight SDK). Run with:
//!   cargo test -p sentinel-engine --features ultralight --test js_speed -- --ignored --nocapture

#![cfg(feature = "ultralight")]

#[test]
#[ignore = "requires Ultralight SDK; run with --ignored --nocapture"]
fn report_js_loop_speed() {
    let out = sentinel_engine::jsbridge::prove_js_speed().expect("js speed probe");
    println!("JS_SPEED {out}");
}
