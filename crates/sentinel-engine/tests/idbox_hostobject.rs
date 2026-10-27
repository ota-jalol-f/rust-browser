//! pv-next's REAL idbox path: `hostObjects.idbox.ReadChipUid()`, driven through
//! pv-next's own bridge helpers (`o`/`s`/`c`) and `IdBoxPluginService.readChipUid`
//! pasted verbatim. Proves the per-plugin host-object surface reaches the plugin
//! and unwraps the `PluginResult` envelope to the UID — not just `share.RunPlugin`.
//!
//! `#[ignore]` (needs the Ultralight SDK). Run with:
//!   cargo test -p sentinel-engine --features ultralight --test idbox_hostobject -- --ignored --nocapture

#![cfg(feature = "ultralight")]

#[test]
#[ignore = "requires Ultralight SDK; run with --ignored --nocapture"]
fn pv_next_idbox_read_chip_uid_via_host_object() {
    let uid = sentinel_engine::jsbridge::prove_idbox_hostobject().expect("idbox host-object proof");
    println!("IDBOX_UID {uid}");
    // pv-next's readChipUid() must observe exactly the envelope's `data`.
    assert_eq!(uid, "04a1b2c3d4e5f6", "host-object ReadChipUid did not unwrap to the UID: {uid}");
}
