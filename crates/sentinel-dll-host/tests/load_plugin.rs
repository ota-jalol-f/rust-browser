//! Loads the real `sentinel-test-plugin` DLL and invokes it through [`DllHost`],
//! proving the end-to-end native path: read → hash → dlopen → call → free.
//!
//! `#[ignore]` because it requires `sentinel-test-plugin` to be built first:
//!   cargo build -p sentinel-test-plugin
//!   cargo test  -p sentinel-dll-host -- --ignored

use std::path::PathBuf;

use sentinel_dll_host::{sha256_hex, DllHost};

/// Locate the built test-plugin DLL next to the other build artifacts.
fn plugin_path() -> PathBuf {
    // tests run with CARGO_MANIFEST_DIR = crates/sentinel-dll-host
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let target = manifest.join("..").join("..").join("target").join("debug");
    let name = if cfg!(windows) {
        "sentinel_test_plugin.dll"
    } else if cfg!(target_os = "macos") {
        "libsentinel_test_plugin.dylib"
    } else {
        "libsentinel_test_plugin.so"
    };
    target.join(name)
}

#[test]
#[ignore = "build sentinel-test-plugin first; run with --ignored"]
fn loads_and_invokes_real_plugin() {
    let path = plugin_path();
    assert!(path.exists(), "plugin not built: {} (run: cargo build -p sentinel-test-plugin)", path.display());

    let mut host = DllHost::new();
    let handle = host.load(&path, None).expect("load plugin");
    assert_eq!(host.len(), 1);

    let out = host.invoke(handle, b"hello, native world").expect("invoke");
    assert_eq!(out, b"HELLO, NATIVE WORLD", "plugin should uppercase its input");

    assert!(host.unload(handle));
    assert!(host.is_empty());
}

#[test]
#[ignore = "build sentinel-test-plugin first; run with --ignored"]
fn correct_hash_gate_allows_load() {
    let path = plugin_path();
    assert!(path.exists(), "plugin not built: {}", path.display());
    let bytes = std::fs::read(&path).unwrap();
    let digest = sha256_hex(&bytes);

    let mut host = DllHost::new();
    // Loading with the *correct* hash must succeed.
    let handle = host.load(&path, Some(&digest)).expect("load with correct hash");
    let out = host.invoke(handle, b"abc").unwrap();
    assert_eq!(out, b"ABC");
}
