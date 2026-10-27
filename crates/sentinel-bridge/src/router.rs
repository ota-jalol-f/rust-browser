//! The security-critical core of the native bridge: deciding whether a page's
//! request may proceed, based on the per-origin permission grants.
//!
//! This is pure logic with no JS or device I/O, so it is fully unit-tested. The
//! JavaScript-facing glue (in [`crate::jsapi`]) and the host transport call into
//! a [`BridgeRouter`]; they never make policy decisions themselves.

use sentinel_ipc::Request;
use sentinel_permissions::{Capability, Origin, PermissionBroker, PermissionState};

/// Which capability a given request requires.
///
/// Centralizing this mapping ensures every request is gated by exactly the right
/// permission — a new request variant must be classified here or it will not
/// compile against an exhaustive match.
pub fn required_capability(request: &Request) -> Capability {
    match request {
        Request::UsbList
        | Request::UsbOpen { .. }
        | Request::UsbTransferIn { .. }
        | Request::UsbTransferOut { .. }
        | Request::UsbClose { .. } => Capability::Usb,

        Request::SerialList
        | Request::SerialOpen { .. }
        | Request::SerialWrite { .. }
        | Request::SerialRead { .. }
        | Request::SerialClose { .. } => Capability::Serial,

        Request::DllLoad { .. } | Request::DllCall { .. } | Request::DllUnload { .. } => {
            Capability::Dll
        }

        // A plugin passthrough (helper transport) is gated by the plugin capability.
        Request::Plugin { .. } => Capability::Plugin,
    }
}

/// The result of routing a page request through the permission gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BridgeOutcome {
    /// Permission is granted; forward this request to the relevant host process.
    Forward(Request),
    /// No decision exists yet; the UI must prompt the user for this capability,
    /// then re-dispatch on grant.
    NeedsConsent(Capability),
    /// Permission is explicitly denied; reject without prompting.
    Denied(Capability),
}

/// Routes page requests through the permission broker.
///
/// Generic over the [`PermissionBroker`] so tests use an in-memory broker and
/// production can use a persistent one without changing this logic.
pub struct BridgeRouter<B: PermissionBroker> {
    broker: B,
}

impl<B: PermissionBroker> BridgeRouter<B> {
    /// Wrap a permission broker.
    pub fn new(broker: B) -> Self {
        Self { broker }
    }

    /// Decide what to do with `request` from `origin`.
    ///
    /// Looks up the capability the request needs, then consults the broker:
    /// `Granted` → [`BridgeOutcome::Forward`], `Ask` → [`BridgeOutcome::NeedsConsent`],
    /// `Denied` → [`BridgeOutcome::Denied`].
    pub fn dispatch(&self, origin: &Origin, request: Request) -> BridgeOutcome {
        let capability = required_capability(&request);
        match self.broker.check(origin, capability) {
            PermissionState::Granted => BridgeOutcome::Forward(request),
            PermissionState::Ask => BridgeOutcome::NeedsConsent(capability),
            PermissionState::Denied => BridgeOutcome::Denied(capability),
        }
    }

    /// Read-only access to the broker (e.g. to list grants in settings UI).
    pub fn broker(&self) -> &B {
        &self.broker
    }

    /// Mutable access to the broker, e.g. to record the user's consent answer
    /// before re-dispatching the original request.
    pub fn broker_mut(&mut self) -> &mut B {
        &mut self.broker
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sentinel_permissions::InMemoryBroker;

    fn origin() -> Origin {
        Origin::parse("https://app.example").unwrap()
    }

    fn router() -> BridgeRouter<InMemoryBroker> {
        BridgeRouter::new(InMemoryBroker::new())
    }

    #[test]
    fn capability_mapping_is_correct() {
        assert_eq!(required_capability(&Request::UsbList), Capability::Usb);
        assert_eq!(
            required_capability(&Request::SerialOpen { path: "COM3".into(), baud_rate: 9600 }),
            Capability::Serial
        );
        assert_eq!(
            required_capability(&Request::DllLoad { path: "x.dll".into(), sha256: "ab".into() }),
            Capability::Dll
        );
    }

    #[test]
    fn unknown_origin_needs_consent() {
        let r = router();
        assert_eq!(r.dispatch(&origin(), Request::UsbList), BridgeOutcome::NeedsConsent(Capability::Usb));
    }

    #[test]
    fn granted_capability_forwards_request() {
        let mut r = router();
        r.broker_mut().set(&origin(), Capability::Usb, PermissionState::Granted);
        assert_eq!(r.dispatch(&origin(), Request::UsbList), BridgeOutcome::Forward(Request::UsbList));
    }

    #[test]
    fn denied_capability_is_rejected() {
        let mut r = router();
        r.broker_mut().set(&origin(), Capability::Dll, PermissionState::Denied);
        let req = Request::DllLoad { path: "x.dll".into(), sha256: "ab".into() };
        assert_eq!(r.dispatch(&origin(), req), BridgeOutcome::Denied(Capability::Dll));
    }

    #[test]
    fn grant_is_capability_scoped() {
        // Granting USB must NOT let serial requests through.
        let mut r = router();
        r.broker_mut().set(&origin(), Capability::Usb, PermissionState::Granted);
        let req = Request::SerialList;
        assert_eq!(r.dispatch(&origin(), req), BridgeOutcome::NeedsConsent(Capability::Serial));
    }

    #[test]
    fn grant_is_origin_scoped() {
        // A grant for one origin must not apply to another.
        let mut r = router();
        r.broker_mut().set(&origin(), Capability::Usb, PermissionState::Granted);
        let other = Origin::parse("https://evil.example").unwrap();
        assert_eq!(r.dispatch(&other, Request::UsbList), BridgeOutcome::NeedsConsent(Capability::Usb));
    }
}
