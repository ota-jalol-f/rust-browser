//! pv-next's REAL, unmodified bridge (`bridge-hTflbAeO.js`, shipped 2026-11-20)
//! driving our host bridge. Only its trailing ES `export{…}` is rewritten so it
//! runs as a classic script; its own `a8` deferred allowlist, `Proxy`/`then`-trap,
//! `InvokeDeferred` call and `mybrowser:deferred` listener are exercised as-is.
//!
//! Proves three paths at once:
//! - deferred `idbox.ReadChipUid()` goes through `share.InvokeDeferred` and comes
//!   back via the `mybrowser:deferred` message, unwrapped to the UID;
//! - direct `system.isDebug()` (not in `a8`) stays a direct host-object call and
//!   returns the raw, JSON-decoded value (boolean `true`), NOT via InvokeDeferred;
//! - direct `system.getEnv()` returns the whole envelope object (default shape).
//!
//! `#[ignore]` (needs the Ultralight SDK). Run with:
//!   cargo test -p sentinel-engine --features ultralight --test idbox_hostobject -- --ignored --nocapture

#![cfg(feature = "ultralight")]

/// pv-next's real bridge bytes, checked in as a fixture (byte-identical to the dist).
const PVNEXT_BRIDGE: &str = include_str!("fixtures/pvnext_bridge.js");

#[test]
#[ignore = "requires Ultralight SDK; run with --ignored --nocapture"]
fn pv_next_real_bridge_deferred_and_direct() {
    let out = sentinel_engine::jsbridge::prove_pvnext_bridge(PVNEXT_BRIDGE)
        .expect("pv-next bridge proof");
    println!("PVNEXT_BRIDGE_OUT {out}");
    let v: serde_json::Value = serde_json::from_str(&out).expect("summary is JSON");

    // Deferred: idbox.ReadChipUid unwrapped to exactly the envelope's data.
    assert_eq!(v["uid"], "04a1b2c3d4e5f6", "deferred ReadChipUid did not unwrap to the UID: {out}");
    // Exactly one InvokeDeferred call — the deferred method; the two direct ones must not use it.
    assert_eq!(v["idef"], 1, "expected exactly one InvokeDeferred call: {out}");
    // Direct raw: a real boolean, not the envelope and not the string "true".
    assert_eq!(v["dbg"], true, "direct raw isDebug did not return boolean true: {out}");
    assert_eq!(v["dbgType"], "boolean", "direct raw isDebug was not a boolean: {out}");
    // Direct envelope: the whole PluginResult object reached the caller.
    assert_eq!(v["envHasSuccess"], true, "direct envelope method lost its envelope: {out}");
}
