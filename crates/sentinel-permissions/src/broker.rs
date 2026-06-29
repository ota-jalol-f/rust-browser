//! The permission broker: stores decisions and answers "may this origin use this
//! capability?".
//!
//! [`PermissionBroker`] is the interface the bridge calls. [`InMemoryBroker`] is a
//! complete, tested implementation; a persistent (encrypted SQLite) broker can
//! implement the same trait later without changing callers.

use std::collections::HashMap;

use crate::model::{Capability, Origin, PermissionState};

/// Answers permission questions and records decisions.
///
/// The secure default is [`PermissionState::Ask`]: any (origin, capability) the
/// broker has never been told about must prompt the user.
pub trait PermissionBroker {
    /// The current decision for `origin` + `capability`.
    fn check(&self, origin: &Origin, capability: Capability) -> PermissionState;

    /// Record a decision (typically the user's answer to a prompt).
    fn set(&mut self, origin: &Origin, capability: Capability, state: PermissionState);

    /// Forget any decision for `origin` + `capability`, reverting it to `Ask`.
    /// Returns `true` if a decision existed.
    fn revoke(&mut self, origin: &Origin, capability: Capability) -> bool;

    /// All recorded decisions, for a settings/permissions UI.
    fn list(&self) -> Vec<(Origin, Capability, PermissionState)>;

    /// Convenience: is this capability currently allowed without prompting?
    fn is_granted(&self, origin: &Origin, capability: Capability) -> bool {
        self.check(origin, capability) == PermissionState::Granted
    }
}

/// An in-memory [`PermissionBroker`] backed by a hash map. Decisions live only for
/// the process lifetime — suitable for tests and as the logic core a persistent
/// broker can delegate to.
#[derive(Debug, Default)]
pub struct InMemoryBroker {
    decisions: HashMap<(Origin, Capability), PermissionState>,
}

impl InMemoryBroker {
    /// Create an empty broker (every query returns `Ask`).
    pub fn new() -> Self {
        Self::default()
    }
}

impl PermissionBroker for InMemoryBroker {
    fn check(&self, origin: &Origin, capability: Capability) -> PermissionState {
        self.decisions
            .get(&(origin.clone(), capability))
            .copied()
            .unwrap_or(PermissionState::Ask)
    }

    fn set(&mut self, origin: &Origin, capability: Capability, state: PermissionState) {
        self.decisions.insert((origin.clone(), capability), state);
    }

    fn revoke(&mut self, origin: &Origin, capability: Capability) -> bool {
        self.decisions.remove(&(origin.clone(), capability)).is_some()
    }

    fn list(&self) -> Vec<(Origin, Capability, PermissionState)> {
        self.decisions
            .iter()
            .map(|((o, c), s)| (o.clone(), *c, *s))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn origin(s: &str) -> Origin {
        Origin::parse(s).unwrap()
    }

    #[test]
    fn unknown_pair_defaults_to_ask() {
        let b = InMemoryBroker::new();
        assert_eq!(b.check(&origin("https://a.com"), Capability::Usb), PermissionState::Ask);
        assert!(!b.is_granted(&origin("https://a.com"), Capability::Usb));
    }

    #[test]
    fn set_and_check_roundtrip() {
        let mut b = InMemoryBroker::new();
        let o = origin("https://a.com");
        b.set(&o, Capability::Serial, PermissionState::Granted);
        assert_eq!(b.check(&o, Capability::Serial), PermissionState::Granted);
        assert!(b.is_granted(&o, Capability::Serial));
    }

    #[test]
    fn decisions_are_isolated_by_origin_and_capability() {
        let mut b = InMemoryBroker::new();
        let a = origin("https://a.com");
        let c = origin("https://c.com");
        b.set(&a, Capability::Usb, PermissionState::Granted);
        // Different capability, same origin: still Ask.
        assert_eq!(b.check(&a, Capability::Dll), PermissionState::Ask);
        // Different origin, same capability: still Ask.
        assert_eq!(b.check(&c, Capability::Usb), PermissionState::Ask);
    }

    #[test]
    fn denied_is_not_granted() {
        let mut b = InMemoryBroker::new();
        let o = origin("https://a.com");
        b.set(&o, Capability::Dll, PermissionState::Denied);
        assert_eq!(b.check(&o, Capability::Dll), PermissionState::Denied);
        assert!(!b.is_granted(&o, Capability::Dll));
    }

    #[test]
    fn revoke_reverts_to_ask() {
        let mut b = InMemoryBroker::new();
        let o = origin("https://a.com");
        b.set(&o, Capability::Usb, PermissionState::Granted);
        assert!(b.revoke(&o, Capability::Usb));
        assert_eq!(b.check(&o, Capability::Usb), PermissionState::Ask);
        // Revoking again reports nothing to remove.
        assert!(!b.revoke(&o, Capability::Usb));
    }

    #[test]
    fn list_reports_all_decisions() {
        let mut b = InMemoryBroker::new();
        b.set(&origin("https://a.com"), Capability::Usb, PermissionState::Granted);
        b.set(&origin("https://b.com"), Capability::Dll, PermissionState::Denied);
        let mut listed = b.list();
        listed.sort_by_key(|(o, c, _)| (o.as_str().to_string(), c.to_string()));
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].2, PermissionState::Granted);
        assert_eq!(listed[1].2, PermissionState::Denied);
    }
}
