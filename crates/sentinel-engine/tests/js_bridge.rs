//! End-to-end JS↔native bridge tests against a REAL device host executor:
//! `window.native.usb.list()` must be permission-gated, and once granted must
//! return the host's actual enumeration result back to page JavaScript.
//! `#[ignore]` because it needs the Ultralight SDK at runtime; run with:
//!   `cargo test -p sentinel-engine --features ultralight -- --ignored`

#![cfg(feature = "ultralight")]

use std::sync::{Arc, Mutex};

use sentinel_bridge::BridgeRouter;
use sentinel_device_host::{DeviceHost, SystemBackend};
use sentinel_engine::jsbridge::{run_bridge_probe, SharedExecutor, SharedRouter};
use sentinel_permissions::{
    Capability, InMemoryBroker, Origin, PermissionBroker, PermissionState,
};

fn shared() -> (SharedRouter<InMemoryBroker>, SharedExecutor<DeviceHost<SystemBackend>>, Origin) {
    let router = Arc::new(Mutex::new(BridgeRouter::new(InMemoryBroker::new())));
    let executor = Arc::new(Mutex::new(DeviceHost::new(SystemBackend)));
    let origin = Origin::parse("https://test.example").unwrap();
    (router, executor, origin)
}

// NOTE: exactly one #[test] here — Ultralight's Renderer is once-per-process, and
// run_bridge_probe creates one. The ungranted/needs_consent path is covered by the
// sentinel-bridge unit tests; this proves the granted *execution* path end to end.
#[test]
#[ignore = "requires Ultralight SDK shared libraries at runtime; run with --ignored"]
fn granted_usb_list_executes_real_enumeration() {
    let (router, executor, origin) = shared();
    router
        .lock()
        .unwrap()
        .broker_mut()
        .set(&origin, Capability::Usb, PermissionState::Granted);

    // After granting, the page call runs the real DeviceHost enumeration and gets
    // a usb_devices response back. (Device count varies by machine, so assert the
    // shape, not a number.)
    let probe = "(function(){ const r = window.native.usb.list(); \
                 return r.status + ':' + (r.response && r.response.result) + ':' + \
                 (Array.isArray(r.response.data) ? 'arr' : typeof r.response.data); })()";
    let result = run_bridge_probe(router, executor, origin, probe).expect("probe");
    assert_eq!(
        result, "ok:usb_devices:arr",
        "granted call must execute the device host and return a usb_devices array"
    );
}
