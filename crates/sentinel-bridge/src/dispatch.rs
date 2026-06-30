//! JSON-in / JSON-out dispatch: the boundary the JavaScript bridge calls.
//!
//! Page JS hands the native entry point a JSON [`sentinel_ipc::Request`]; this
//! module parses it, routes it through a [`BridgeRouter`], and returns a JSON
//! [`DispatchReport`] describing what happened. Keeping this as a pure
//! `&str -> String` function means the whole policy path is unit-tested without
//! any JavaScript engine.

use serde::Serialize;
use sentinel_ipc::Request;
use sentinel_permissions::{Capability, Origin, PermissionBroker};

use crate::router::{BridgeOutcome, BridgeRouter};

/// The JSON-serializable result returned to page JavaScript for a native call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum DispatchReport {
    /// Permission is granted; the request is (or would be) executed on a host.
    /// Carries no payload yet — device/DLL execution lands with the host crates.
    Forward,
    /// The user must be prompted for `capability` before this can proceed.
    NeedsConsent {
        /// Capability the page must be granted.
        capability: Capability,
    },
    /// The capability is explicitly denied for this origin.
    Denied {
        /// Capability that was denied.
        capability: Capability,
    },
    /// The request JSON was malformed or unknown.
    BadRequest {
        /// Why the request could not be parsed.
        message: String,
    },
}

/// Parse `request_json`, route it for `origin`, and return a JSON [`DispatchReport`].
///
/// Never panics and never returns an `Err`: a malformed request becomes
/// [`DispatchReport::BadRequest`], so the JS side always receives valid JSON.
pub fn dispatch_json<B: PermissionBroker>(
    router: &BridgeRouter<B>,
    origin: &Origin,
    request_json: &str,
) -> String {
    let report = match serde_json::from_str::<Request>(request_json) {
        Ok(request) => match router.dispatch(origin, request) {
            BridgeOutcome::Forward(_) => DispatchReport::Forward,
            BridgeOutcome::NeedsConsent(capability) => DispatchReport::NeedsConsent { capability },
            BridgeOutcome::Denied(capability) => DispatchReport::Denied { capability },
        },
        Err(e) => DispatchReport::BadRequest { message: e.to_string() },
    };
    // DispatchReport serializes infallibly (only enums/strings); fall back to a
    // hand-written error object on the impossible chance it does not.
    serde_json::to_string(&report).unwrap_or_else(|_| {
        r#"{"status":"bad_request","message":"failed to serialize report"}"#.to_string()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use sentinel_permissions::{InMemoryBroker, PermissionState};

    fn origin() -> Origin {
        Origin::parse("https://app.example").unwrap()
    }

    fn router() -> BridgeRouter<InMemoryBroker> {
        BridgeRouter::new(InMemoryBroker::new())
    }

    #[test]
    fn unknown_origin_yields_needs_consent_json() {
        let json = dispatch_json(&router(), &origin(), r#"{"op":"usb_list"}"#);
        assert_eq!(json, r#"{"status":"needs_consent","capability":"usb"}"#);
    }

    #[test]
    fn granted_capability_yields_forward() {
        let mut r = router();
        r.broker_mut().set(&origin(), Capability::Usb, PermissionState::Granted);
        let json = dispatch_json(&r, &origin(), r#"{"op":"usb_list"}"#);
        assert_eq!(json, r#"{"status":"forward"}"#);
    }

    #[test]
    fn denied_capability_yields_denied() {
        let mut r = router();
        r.broker_mut().set(&origin(), Capability::Dll, PermissionState::Denied);
        let req = r#"{"op":"dll_load","path":"x.dll","sha256":"ab"}"#;
        let json = dispatch_json(&r, &origin(), req);
        assert_eq!(json, r#"{"status":"denied","capability":"dll"}"#);
    }

    #[test]
    fn serial_request_maps_to_serial_capability() {
        let req = r#"{"op":"serial_open","path":"COM3","baud_rate":9600}"#;
        let json = dispatch_json(&router(), &origin(), req);
        assert_eq!(json, r#"{"status":"needs_consent","capability":"serial"}"#);
    }

    #[test]
    fn malformed_json_yields_bad_request() {
        let json = dispatch_json(&router(), &origin(), "not json");
        assert!(json.starts_with(r#"{"status":"bad_request""#), "got: {json}");
    }

    #[test]
    fn unknown_op_yields_bad_request() {
        let json = dispatch_json(&router(), &origin(), r#"{"op":"launch_missiles"}"#);
        assert!(json.starts_with(r#"{"status":"bad_request""#), "got: {json}");
    }
}
