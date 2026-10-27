//! The request/response message types exchanged between the main process and the
//! out-of-process hosts (device host: USB + serial; DLL host: native code).
//!
//! Each call is wrapped in an envelope carrying a correlation `id` so the async
//! bridge can match a [`ResponseEnvelope`] to the [`RequestEnvelope`] that
//! triggered it. The types are transport-agnostic; framing lives in [`crate::wire`].

use serde::{Deserialize, Serialize};

/// A USB device as reported by enumeration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsbDeviceInfo {
    /// USB vendor id.
    pub vendor_id: u16,
    /// USB product id.
    pub product_id: u16,
    /// Serial number string, if the device exposes one.
    pub serial: Option<String>,
    /// Manufacturer string, if available.
    pub manufacturer: Option<String>,
    /// Product string, if available.
    pub product: Option<String>,
}

/// A request from the main process to a host process.
///
/// Handles (`u64`) are issued by the host on open/load and used to address the
/// resource on subsequent calls.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    /// Enumerate available USB devices.
    UsbList,
    /// Open a USB device by vendor/product id. Requires a prior permission grant.
    UsbOpen {
        /// USB vendor id of the device to open.
        vendor_id: u16,
        /// USB product id of the device to open.
        product_id: u16,
    },
    /// Read `length` bytes from an IN endpoint of an open device.
    UsbTransferIn {
        /// Handle of the open device.
        handle: u64,
        /// IN endpoint address to read from.
        endpoint: u8,
        /// Number of bytes to read.
        length: u32,
    },
    /// Write `data` to an OUT endpoint of an open device.
    UsbTransferOut {
        /// Handle of the open device.
        handle: u64,
        /// OUT endpoint address to write to.
        endpoint: u8,
        /// Bytes to write.
        data: Vec<u8>,
    },
    /// Close an open USB device.
    UsbClose {
        /// Handle of the device to close.
        handle: u64,
    },

    /// Enumerate available serial ports.
    SerialList,
    /// Open a serial port at `baud_rate`. Requires a prior permission grant.
    SerialOpen {
        /// OS path / name of the port (e.g. `COM3`).
        path: String,
        /// Baud rate to open the port at.
        baud_rate: u32,
    },
    /// Write bytes to an open serial port.
    SerialWrite {
        /// Handle of the open port.
        handle: u64,
        /// Bytes to write.
        data: Vec<u8>,
    },
    /// Read up to `length` bytes from an open serial port.
    SerialRead {
        /// Handle of the open port.
        handle: u64,
        /// Maximum number of bytes to read.
        length: u32,
    },
    /// Close an open serial port.
    SerialClose {
        /// Handle of the port to close.
        handle: u64,
    },

    /// Load a native DLL from `path`, verifying its SHA-256 matches `sha256`.
    /// Requires explicit per-origin consent (granting = running software).
    DllLoad {
        /// Filesystem path to the DLL to load.
        path: String,
        /// Expected lowercase hex SHA-256 of the DLL file; load fails on mismatch.
        sha256: String,
    },
    /// Call `symbol` in a loaded DLL with opaque `args` bytes.
    DllCall {
        /// Handle of the loaded DLL.
        handle: u64,
        /// Exported symbol name to call.
        symbol: String,
        /// Opaque argument bytes passed to the call (ABI defined out of band).
        args: Vec<u8>,
    },
    /// Unload a loaded DLL.
    DllUnload {
        /// Handle of the DLL to unload.
        handle: u64,
    },

    /// Run a plugin hosted in this (helper) process by id, passing an opaque
    /// request string and getting back its `PluginResult` envelope. This runs a
    /// plugin out of process — e.g. an x86 vendor-SDK plugin (idbox chip) in an
    /// x86 helper the x64 host can't load the SDK into.
    Plugin {
        /// Plugin id (e.g. `"Asbt.IdBox"`).
        id: String,
        /// Opaque request payload (plugin-defined JSON).
        request: String,
    },
}

/// A response from a host process.
///
/// Adjacently tagged (`{"result": "...", "data": ...}`) so that tuple variants
/// like [`Response::UsbDevices`] and [`Response::Data`] — whose payloads serialize
/// as sequences — are representable (internal tagging rejects non-map payloads).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "result", content = "data", rename_all = "snake_case")]
pub enum Response {
    /// USB enumeration result.
    UsbDevices(Vec<UsbDeviceInfo>),
    /// Serial port enumeration result (port names).
    SerialPorts(Vec<String>),
    /// A resource was opened/loaded; carries its handle.
    Opened {
        /// Handle the host issued for the new resource.
        handle: u64,
    },
    /// Bytes read from a device/port, or a DLL call's return value.
    Data(Vec<u8>),
    /// A write completed; carries the number of bytes written.
    Wrote {
        /// Number of bytes actually written.
        bytes: u64,
    },
    /// A unit-success acknowledgement (e.g. close/unload).
    Ack,
    /// The operation failed; carries a human-readable message.
    Error {
        /// Human-readable failure description.
        message: String,
    },
    /// A plugin call's result: the plugin's `{data,success,error_message}` JSON
    /// envelope as a string.
    Plugin {
        /// The `PluginResult` envelope, serialized.
        envelope: String,
    },
}

/// A request tagged with a correlation id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestEnvelope {
    /// Correlation id, echoed in the matching [`ResponseEnvelope`].
    pub id: u64,
    /// The request payload.
    pub request: Request,
}

/// A response tagged with the correlation id of its request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResponseEnvelope {
    /// Correlation id copied from the originating [`RequestEnvelope`].
    pub id: u64,
    /// The response payload.
    pub response: Response,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_envelope_json_roundtrips() {
        let env = RequestEnvelope {
            id: 7,
            request: Request::UsbOpen { vendor_id: 0x1234, product_id: 0x5678 },
        };
        let json = serde_json::to_string(&env).unwrap();
        let back: RequestEnvelope = serde_json::from_str(&json).unwrap();
        assert_eq!(env, back);
    }

    #[test]
    fn response_error_roundtrips() {
        let env = ResponseEnvelope {
            id: 7,
            response: Response::Error { message: "device busy".into() },
        };
        let json = serde_json::to_string(&env).unwrap();
        let back: ResponseEnvelope = serde_json::from_str(&json).unwrap();
        assert_eq!(env, back);
    }

    #[test]
    fn binary_payload_survives_roundtrip() {
        let env = RequestEnvelope {
            id: 1,
            request: Request::UsbTransferOut { handle: 9, endpoint: 1, data: vec![0, 255, 13, 37] },
        };
        let json = serde_json::to_string(&env).unwrap();
        let back: RequestEnvelope = serde_json::from_str(&json).unwrap();
        assert_eq!(env, back);
    }

    #[test]
    fn tagged_representation_is_stable() {
        // The `op` tag is part of the wire contract; lock it down.
        let json = serde_json::to_string(&Request::UsbList).unwrap();
        assert_eq!(json, r#"{"op":"usb_list"}"#);
    }

    #[test]
    fn response_tuple_variants_serialize_and_roundtrip() {
        // Regression guard: tuple-payload variants must serialize (adjacent tag).
        let devices = Response::UsbDevices(vec![UsbDeviceInfo {
            vendor_id: 1,
            product_id: 2,
            serial: None,
            manufacturer: None,
            product: None,
        }]);
        let json = serde_json::to_string(&devices).unwrap();
        assert!(json.starts_with(r#"{"result":"usb_devices","data":["#), "got: {json}");
        let back: Response = serde_json::from_str(&json).unwrap();
        assert_eq!(devices, back);

        // Unit variant has no `data` field.
        assert_eq!(serde_json::to_string(&Response::Ack).unwrap(), r#"{"result":"ack"}"#);
    }
}
