//! `sentinel-hosts`: the combined [`RequestExecutor`] the main process hands to the
//! bridge.
//!
//! The bridge needs a single executor, but requests split across two isolated
//! hosts: USB/serial go to the [`DeviceHost`], DLL requests go to the [`DllHost`].
//! [`HostRouter`] owns both and dispatches by request kind, so the bridge stays
//! oblivious to the split.
//!
//! (When the hosts move out of process, this same `RequestExecutor` impl becomes
//! the client side that serializes each request over the `sentinel-ipc` frame
//! protocol — callers of [`HostRouter`] do not change.)

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use sentinel_device_host::{DeviceBackend, DeviceHost};
use sentinel_dll_host::DllHost;
use sentinel_ipc::{Request, RequestExecutor, Response};

/// Routes each [`Request`] to the host that owns the corresponding resource.
pub struct HostRouter<B: DeviceBackend> {
    /// USB + serial host.
    pub device: DeviceHost<B>,
    /// Native DLL host.
    pub dll: DllHost,
}

impl<B: DeviceBackend> HostRouter<B> {
    /// Combine a device host and a DLL host.
    pub fn new(device: DeviceHost<B>, dll: DllHost) -> Self {
        Self { device, dll }
    }
}

impl<B: DeviceBackend> RequestExecutor for HostRouter<B> {
    fn execute(&mut self, request: Request) -> Response {
        match request {
            Request::DllLoad { .. } | Request::DllCall { .. } | Request::DllUnload { .. } => {
                self.dll.execute(request)
            }
            // Everything else (USB + serial) belongs to the device host.
            other => self.device.execute(other),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sentinel_device_host::DeviceBackend;
    use sentinel_core::Result;
    use sentinel_ipc::UsbDeviceInfo;

    struct MockBackend;
    impl DeviceBackend for MockBackend {
        fn list_usb(&self) -> Result<Vec<UsbDeviceInfo>> {
            Ok(vec![UsbDeviceInfo {
                vendor_id: 0xABCD,
                product_id: 0x0001,
                serial: None,
                manufacturer: None,
                product: None,
            }])
        }
        fn list_serial(&self) -> Result<Vec<String>> {
            Ok(vec!["COM9".into()])
        }
    }

    fn router() -> HostRouter<MockBackend> {
        HostRouter::new(DeviceHost::new(MockBackend), DllHost::new())
    }

    #[test]
    fn usb_request_routes_to_device_host() {
        let mut r = router();
        match r.execute(Request::UsbList) {
            Response::UsbDevices(d) => assert_eq!(d[0].vendor_id, 0xABCD),
            other => panic!("expected UsbDevices, got {other:?}"),
        }
    }

    #[test]
    fn serial_request_routes_to_device_host() {
        let mut r = router();
        assert!(matches!(r.execute(Request::SerialList), Response::SerialPorts(_)));
    }

    #[test]
    fn dll_request_routes_to_dll_host() {
        let mut r = router();
        // Unknown handle reaches the DLL host (which reports the error), proving
        // the request was routed there rather than to the device host.
        match r.execute(Request::DllCall { handle: 1, symbol: "sentinel_invoke".into(), args: vec![] }) {
            Response::Error { message } => assert!(message.contains("handle")),
            other => panic!("expected DLL-host error, got {other:?}"),
        }
    }
}
