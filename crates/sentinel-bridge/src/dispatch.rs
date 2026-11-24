//! JSON-in / JSON-out dispatch: the boundary the JavaScript bridge calls.
//!
//! Page JS hands the native entry point a JSON [`sentinel_ipc::Request`]; this
//! module parses it, routes it through a [`BridgeRouter`], and returns a JSON
//! [`DispatchReport`] describing what happened. Keeping this as a pure
//! `&str -> String` function means the whole policy path is unit-tested without
//! any JavaScript engine.

use serde::Serialize;
use sentinel_ipc::{Request, RequestExecutor, Response};
use sentinel_permissions::{Capability, Origin, PermissionBroker};

use crate::router::{BridgeOutcome, BridgeRouter};

/// The JSON-serializable result returned to page JavaScript for a native call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum DispatchReport {
    /// Permission was granted and the request executed; carries the host response.
    Ok {
        /// The host's response to the executed request.
        response: Response,
    },
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

/// Parse `request_json`, route it for `origin`, execute it on `executor` if
/// permitted, and return a JSON [`DispatchReport`].
///
/// Never panics and never returns an `Err`: a malformed request becomes
/// [`DispatchReport::BadRequest`], so the JS side always receives valid JSON.
pub fn dispatch_json<B: PermissionBroker, E: RequestExecutor>(
    router: &BridgeRouter<B>,
    executor: &mut E,
    origin: &Origin,
    request_json: &str,
) -> String {
    let report = match serde_json::from_str::<Request>(request_json) {
        Ok(request) => match router.dispatch(origin, request) {
            BridgeOutcome::Forward(request) => {
                DispatchReport::Ok { response: executor.execute(request) }
            }
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
    use sentinel_ipc::Response;
    use sentinel_permissions::{InMemoryBroker, PermissionState};

    /// A canned executor: records the last request and returns a fixed response,
    /// so dispatch's execute path is observable without real hardware.
    struct StubExecutor {
        response: Response,
        last: Option<Request>,
    }
    impl RequestExecutor for StubExecutor {
        fn execute(&mut self, request: Request) -> Response {
            self.last = Some(request);
            self.response.clone()
        }
    }
    fn stub(response: Response) -> StubExecutor {
        StubExecutor { response, last: None }
    }

    fn origin() -> Origin {
        Origin::parse("https://app.example").unwrap()
    }

    fn router() -> BridgeRouter<InMemoryBroker> {
        BridgeRouter::new(InMemoryBroker::new())
    }

    #[test]
    fn unknown_origin_yields_needs_consent_json() {
        let mut exec = stub(Response::Ack);
        let json = dispatch_json(&router(), &mut exec, &origin(), r#"{"op":"usb_list"}"#);
        assert_eq!(json, r#"{"status":"needs_consent","capability":"usb"}"#);
        assert!(exec.last.is_none(), "executor must NOT run without a grant");
    }

    #[test]
    fn granted_capability_executes_and_returns_response() {
        let mut r = router();
        r.broker_mut().set(&origin(), Capability::Usb, PermissionState::Granted);
        let mut exec = stub(Response::UsbDevices(vec![]));
        let json = dispatch_json(&r, &mut exec, &origin(), r#"{"op":"usb_list"}"#);
        assert_eq!(json, r#"{"status":"ok","response":{"result":"usb_devices","data":[]}}"#);
        assert_eq!(exec.last, Some(Request::UsbList), "the request must reach the executor");
    }

    #[test]
    fn denied_capability_yields_denied_and_skips_executor() {
        let mut r = router();
        r.broker_mut().set(&origin(), Capability::Dll, PermissionState::Denied);
        let mut exec = stub(Response::Ack);
        let req = r#"{"op":"dll_load","path":"x.dll","sha256":"ab"}"#;
        let json = dispatch_json(&r, &mut exec, &origin(), req);
        assert_eq!(json, r#"{"status":"denied","capability":"dll"}"#);
        assert!(exec.last.is_none());
    }

    #[test]
    fn serial_request_maps_to_serial_capability() {
        let mut exec = stub(Response::Ack);
        let req = r#"{"op":"serial_open","path":"COM3","baud_rate":9600}"#;
        let json = dispatch_json(&router(), &mut exec, &origin(), req);
        assert_eq!(json, r#"{"status":"needs_consent","capability":"serial"}"#);
    }

    #[test]
    fn malformed_json_yields_bad_request() {
        let mut exec = stub(Response::Ack);
        let json = dispatch_json(&router(), &mut exec, &origin(), "not json");
        assert!(json.starts_with(r#"{"status":"bad_request""#), "got: {json}");
    }

    #[test]
    fn unknown_op_yields_bad_request() {
        let mut exec = stub(Response::Ack);
        let json = dispatch_json(&router(), &mut exec, &origin(), r#"{"op":"launch_missiles"}"#);
        assert!(json.starts_with(r#"{"status":"bad_request""#), "got: {json}");
    }
}
