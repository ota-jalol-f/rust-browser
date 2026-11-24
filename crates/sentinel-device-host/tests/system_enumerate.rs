//! Exercises the real [`SystemBackend`] against this machine. `#[ignore]` because
//! results depend on what hardware is attached (and USB needs the WinUSB driver),
//! but it must not error: run with `cargo test -p sentinel-device-host -- --ignored`.

use sentinel_device_host::{DeviceBackend, DeviceHost, SystemBackend};
use sentinel_ipc::{Request, Response};

#[test]
#[ignore = "depends on attached hardware; run with --ignored"]
fn system_backend_enumerates_without_error() {
    let backend = SystemBackend;

    // Serial enumeration should always succeed (possibly empty).
    let ports = backend.list_serial().expect("serial enumeration must not error");
    println!("serial ports: {ports:?}");

    // USB enumeration should always succeed (possibly empty).
    let devices = backend.list_usb().expect("usb enumeration must not error");
    println!("usb devices: {} found", devices.len());

    // The same through the host router.
    let mut host = DeviceHost::new(SystemBackend);
    assert!(matches!(host.handle(Request::SerialList), Response::SerialPorts(_)));
    assert!(matches!(host.handle(Request::UsbList), Response::UsbDevices(_)));
}
