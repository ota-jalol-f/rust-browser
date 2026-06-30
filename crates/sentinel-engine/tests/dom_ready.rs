//! Reproduces the shell's live-GUI install path headlessly: install window.native
//! from inside a DOM-ready callback. If this crashes here, the bug is reproducible
//! with the CPU renderer; if it passes, the shell crash is AppCore/GPU-specific.
//! Run with: cargo test -p sentinel-engine --features ultralight --test dom_ready -- --ignored

#![cfg(feature = "ultralight")]

use std::sync::{Arc, Mutex};

use sentinel_bridge::BridgeRouter;
use sentinel_device_host::{DeviceHost, SystemBackend};
use sentinel_engine::jsbridge::{prove_dom_ready_install, BRIDGE_VERSION};
use sentinel_permissions::{InMemoryBroker, Origin};

#[test]
#[ignore = "requires Ultralight SDK at runtime; run with --ignored"]
fn install_from_dom_ready_callback_does_not_crash() {
    let router = Arc::new(Mutex::new(BridgeRouter::new(InMemoryBroker::new())));
    let executor = Arc::new(Mutex::new(DeviceHost::new(SystemBackend)));
    let origin = Origin::parse("https://test.example").unwrap();

    let version = prove_dom_ready_install(router, executor, origin)
        .expect("dom-ready install + probe should succeed");
    assert_eq!(version, BRIDGE_VERSION, "window.native must be installed from the DOM-ready hook");
}
