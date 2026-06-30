//! The device backend: where requests actually touch hardware.
//!
//! [`DeviceBackend`] abstracts enumeration so the [`crate::host::DeviceHost`]
//! routing logic can be unit-tested against a mock, while [`SystemBackend`] talks
//! to real hardware via `nusb` (USB) and `serialport` (serial).

use sentinel_core::{Error, Result};
use sentinel_ipc::UsbDeviceInfo;

/// Hardware access used by the device host. Implemented for real by
/// [`SystemBackend`] and mocked in tests.
pub trait DeviceBackend {
    /// Enumerate connected USB devices.
    fn list_usb(&self) -> Result<Vec<UsbDeviceInfo>>;
    /// Enumerate available serial ports (by name, e.g. `COM3`).
    fn list_serial(&self) -> Result<Vec<String>>;
}

/// The real backend: `nusb` for USB, `serialport` for serial.
#[derive(Debug, Default)]
pub struct SystemBackend;

impl DeviceBackend for SystemBackend {
    fn list_usb(&self) -> Result<Vec<UsbDeviceInfo>> {
        use nusb::MaybeFuture;
        let devices = nusb::list_devices()
            .wait()
            .map_err(|e| Error::Device(format!("usb enumeration failed: {e}")))?;
        Ok(devices
            .map(|d| UsbDeviceInfo {
                vendor_id: d.vendor_id(),
                product_id: d.product_id(),
                serial: d.serial_number().map(str::to_string),
                manufacturer: d.manufacturer_string().map(str::to_string),
                product: d.product_string().map(str::to_string),
            })
            .collect())
    }

    fn list_serial(&self) -> Result<Vec<String>> {
        let ports = serialport::available_ports()
            .map_err(|e| Error::Device(format!("serial enumeration failed: {e}")))?;
        Ok(ports.into_iter().map(|p| p.port_name).collect())
    }
}
