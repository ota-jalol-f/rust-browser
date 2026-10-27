//! The device host's request router.
//!
//! [`DeviceHost`] turns an IPC [`Request`] into a [`Response`] using a
//! [`DeviceBackend`]. Enumeration is implemented now; per-device I/O (open /
//! transfer / close) is stubbed with a clear error until the hardware I/O path
//! and its handle table land — kept honest rather than faked.

use sentinel_ipc::{Request, RequestExecutor, Response};

use crate::backend::DeviceBackend;

/// Routes device requests to a backend and formats responses.
///
/// Generic over [`DeviceBackend`] so tests drive it with a mock and production
/// uses [`crate::backend::SystemBackend`].
pub struct DeviceHost<B: DeviceBackend> {
    backend: B,
}

impl<B: DeviceBackend> DeviceHost<B> {
    /// Wrap a backend.
    pub fn new(backend: B) -> Self {
        Self { backend }
    }

    /// Handle one request, returning the response to send back to the main process.
    ///
    /// Enumeration requests (`UsbList`, `SerialList`) are served; all other
    /// (stateful I/O) requests currently return [`Response::Error`] with a
    /// "not yet implemented" message.
    pub fn handle(&mut self, request: Request) -> Response {
        match request {
            Request::UsbList => match self.backend.list_usb() {
                Ok(devices) => Response::UsbDevices(devices),
                Err(e) => Response::Error { message: e.to_string() },
            },
            Request::SerialList => match self.backend.list_serial() {
                Ok(ports) => Response::SerialPorts(ports),
                Err(e) => Response::Error { message: e.to_string() },
            },
            // Stateful device I/O is not wired yet; report honestly.
            Request::UsbOpen { .. }
            | Request::UsbTransferIn { .. }
            | Request::UsbTransferOut { .. }
            | Request::UsbClose { .. }
            | Request::SerialOpen { .. }
            | Request::SerialWrite { .. }
            | Request::SerialRead { .. }
            | Request::SerialClose { .. } => Response::Error {
                message: "device I/O (open/transfer/close) not yet implemented; \
                          enumeration only"
                    .to_string(),
            },
            // DLL requests are not this host's responsibility.
            Request::DllLoad { .. } | Request::DllCall { .. } | Request::DllUnload { .. } => {
                Response::Error {
                    message: "DLL requests are handled by the DLL host, not the device host"
                        .to_string(),
                }
            }
            // Plugin passthrough is handled by a plugin-hosting helper, not here.
            Request::Plugin { .. } => Response::Error {
                message: "plugin requests are handled by a plugin helper, not the device host"
                    .to_string(),
            },
        }
    }
}

impl<B: DeviceBackend> RequestExecutor for DeviceHost<B> {
    /// Execute via [`DeviceHost::handle`] — the device host *is* a request executor.
    fn execute(&mut self, request: Request) -> Response {
        self.handle(request)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::DeviceBackend;
    use sentinel_core::{Error, Result};
    use sentinel_ipc::UsbDeviceInfo;

    /// A backend with canned data, for testing routing without hardware.
    struct MockBackend {
        usb: Vec<UsbDeviceInfo>,
        serial: Vec<String>,
        fail: bool,
    }

    impl DeviceBackend for MockBackend {
        fn list_usb(&self) -> Result<Vec<UsbDeviceInfo>> {
            if self.fail {
                return Err(Error::Device("boom".into()));
            }
            Ok(self.usb.clone())
        }
        fn list_serial(&self) -> Result<Vec<String>> {
            if self.fail {
                return Err(Error::Device("boom".into()));
            }
            Ok(self.serial.clone())
        }
    }

    fn device(vid: u16, pid: u16) -> UsbDeviceInfo {
        UsbDeviceInfo { vendor_id: vid, product_id: pid, serial: None, manufacturer: None, product: None }
    }

    #[test]
    fn usb_list_returns_devices() {
        let mut host = DeviceHost::new(MockBackend {
            usb: vec![device(0x1234, 0x5678)],
            serial: vec![],
            fail: false,
        });
        match host.handle(Request::UsbList) {
            Response::UsbDevices(d) => {
                assert_eq!(d.len(), 1);
                assert_eq!(d[0].vendor_id, 0x1234);
            }
            other => panic!("expected UsbDevices, got {other:?}"),
        }
    }

    #[test]
    fn serial_list_returns_ports() {
        let mut host = DeviceHost::new(MockBackend {
            usb: vec![],
            serial: vec!["COM3".into(), "COM7".into()],
            fail: false,
        });
        match host.handle(Request::SerialList) {
            Response::SerialPorts(p) => assert_eq!(p, vec!["COM3", "COM7"]),
            other => panic!("expected SerialPorts, got {other:?}"),
        }
    }

    #[test]
    fn backend_error_becomes_response_error() {
        let mut host = DeviceHost::new(MockBackend { usb: vec![], serial: vec![], fail: true });
        assert!(matches!(host.handle(Request::UsbList), Response::Error { .. }));
    }

    #[test]
    fn unimplemented_io_returns_error() {
        let mut host = DeviceHost::new(MockBackend { usb: vec![], serial: vec![], fail: false });
        let r = host.handle(Request::UsbOpen { vendor_id: 1, product_id: 2 });
        assert!(matches!(r, Response::Error { .. }));
    }

    #[test]
    fn dll_request_is_rejected_by_device_host() {
        let mut host = DeviceHost::new(MockBackend { usb: vec![], serial: vec![], fail: false });
        let r = host.handle(Request::DllLoad { path: "x.dll".into(), sha256: "ab".into() });
        match r {
            Response::Error { message } => assert!(message.contains("DLL host")),
            other => panic!("expected Error, got {other:?}"),
        }
    }
}
