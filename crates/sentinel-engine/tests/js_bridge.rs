//! End-to-end JS↔native bridge against the REAL combined host executor: a granted
//! page must be able to enumerate USB via `window.native.usb.list()` AND load a
//! real native DLL via `window.native.dll.load()`, both gated by permissions and
//! both returning the host's actual result back to page JavaScript.
//!
//! `#[ignore]` because it needs the Ultralight SDK at runtime and the built
//! test-plugin DLL. Run with:
//!   cargo build -p sentinel-test-plugin
//!   cargo test  -p sentinel-engine --features ultralight -- --ignored

#![cfg(feature = "ultralight")]

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use sentinel_bridge::BridgeRouter;
use sentinel_device_host::{DeviceHost, SystemBackend};
use sentinel_dll_host::DllHost;
use sentinel_engine::jsbridge::{run_bridge_probe, SharedExecutor, SharedRouter};
use sentinel_hosts::HostRouter;
use sentinel_permissions::{
    Capability, InMemoryBroker, Origin, PermissionBroker, PermissionState,
};

fn test_plugin_path() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let name = if cfg!(windows) {
        "sentinel_test_plugin.dll"
    } else if cfg!(target_os = "macos") {
        "libsentinel_test_plugin.dylib"
    } else {
        "libsentinel_test_plugin.so"
    };
    manifest.join("..").join("..").join("target").join("debug").join(name)
}

// Exactly one #[test] here — Ultralight's Renderer is once-per-process.
#[test]
#[ignore = "requires Ultralight SDK + built test-plugin; run with --ignored"]
fn granted_page_uses_usb_and_loads_real_dll() {
    let dll_path = test_plugin_path();
    assert!(
        dll_path.exists(),
        "test plugin not built: {} (run: cargo build -p sentinel-test-plugin)",
        dll_path.display()
    );

    // Combined executor: real device host + real DLL host.
    let executor: SharedExecutor<HostRouter<SystemBackend>> =
        Arc::new(Mutex::new(HostRouter::new(DeviceHost::new(SystemBackend), DllHost::new())));

    let router: SharedRouter<InMemoryBroker> =
        Arc::new(Mutex::new(BridgeRouter::new(InMemoryBroker::new())));
    let origin = Origin::parse("https://test.example").unwrap();
    {
        let mut g = router.lock().unwrap();
        g.broker_mut().set(&origin, Capability::Usb, PermissionState::Granted);
        g.broker_mut().set(&origin, Capability::Dll, PermissionState::Granted);
    }

    // Escape the path into a JS string literal (backslashes on Windows).
    let js_path = dll_path.to_string_lossy().replace('\\', "\\\\");
    // The DLL host now requires a real integrity hash for page-driven loads, so
    // pass the plugin's actual SHA-256 — proving the gate passes end to end.
    let dll_hash = sentinel_dll_host::sha256_hex(&std::fs::read(&dll_path).expect("read test plugin"));
    let probe = format!(
        "(function(){{\
           const u = window.native.usb.list();\
           const d = window.native.dll.load('{js_path}', '{dll_hash}');\
           return u.status + '/' + u.response.result + '|' + d.status + '/' + d.response.result;\
         }})()"
    );

    let result = run_bridge_probe(router, executor, origin, &probe).expect("probe");
    assert_eq!(
        result, "ok/usb_devices|ok/opened",
        "granted page must enumerate USB and load a real DLL (returning a handle) end to end"
    );
}
