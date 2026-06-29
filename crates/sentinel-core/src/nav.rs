//! In-memory navigation history (back / forward / reload) state machine.
//!
//! Public surface: [`History`]. This is a pure data structure with no Ultralight
//! dependency — the engine drives the renderer *from* it, and it is fully
//! unit-tested in isolation. One [`History`] instance models one tab.

use crate::error::{Error, Result};

/// A linear navigation history with a movable cursor, like a single browser tab.
///
/// Invariant: `cursor` is `None` iff `entries` is empty; otherwise `cursor`
/// indexes a valid element of `entries`.
#[derive(Debug, Default, Clone)]
pub struct History {
    entries: Vec<String>,
    /// Index of the current entry, or `None` when history is empty.
    cursor: Option<usize>,
}

impl History {
    /// Create an empty history with no current entry.
    pub fn new() -> Self {
        Self { entries: Vec::new(), cursor: None }
    }

    /// The current URL, if any.
    pub fn current(&self) -> Option<&str> {
        self.cursor.map(|i| self.entries[i].as_str())
    }

    /// Navigate to `url`.
    ///
    /// Any "forward" entries ahead of the cursor are discarded (standard browser
    /// behavior: navigating from a back-stepped position forks history), then the
    /// new entry is pushed and becomes current.
    pub fn navigate(&mut self, url: impl Into<String>) {
        if let Some(i) = self.cursor {
            self.entries.truncate(i + 1);
        }
        self.entries.push(url.into());
        self.cursor = Some(self.entries.len() - 1);
    }

    /// True if there is an entry behind the cursor.
    pub fn can_go_back(&self) -> bool {
        matches!(self.cursor, Some(i) if i > 0)
    }

    /// True if there is an entry ahead of the cursor.
    pub fn can_go_forward(&self) -> bool {
        matches!(self.cursor, Some(i) if i + 1 < self.entries.len())
    }

    /// Move the cursor back one entry and return the new current URL.
    ///
    /// # Errors
    /// Returns [`Error::Navigation`] when there is no entry behind the cursor.
    pub fn back(&mut self) -> Result<&str> {
        if !self.can_go_back() {
            return Err(Error::Navigation("no back entry".into()));
        }
        let i = self.cursor.expect("can_go_back guarantees Some") - 1;
        self.cursor = Some(i);
        Ok(self.entries[i].as_str())
    }

    /// Move the cursor forward one entry and return the new current URL.
    ///
    /// # Errors
    /// Returns [`Error::Navigation`] when there is no entry ahead of the cursor.
    pub fn forward(&mut self) -> Result<&str> {
        if !self.can_go_forward() {
            return Err(Error::Navigation("no forward entry".into()));
        }
        let i = self.cursor.expect("can_go_forward guarantees Some") + 1;
        self.cursor = Some(i);
        Ok(self.entries[i].as_str())
    }

    /// Number of entries in history.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True when there are no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_history_is_empty() {
        let h = History::new();
        assert!(h.is_empty());
        assert_eq!(h.current(), None);
        assert!(!h.can_go_back() && !h.can_go_forward());
    }

    #[test]
    fn navigate_sets_current() {
        let mut h = History::new();
        h.navigate("https://a.com");
        assert_eq!(h.current(), Some("https://a.com"));
        assert_eq!(h.len(), 1);
        assert!(!h.can_go_back() && !h.can_go_forward());
    }

    #[test]
    fn back_and_forward_traverse() {
        let mut h = History::new();
        h.navigate("https://a.com");
        h.navigate("https://b.com");
        assert!(h.can_go_back());
        assert_eq!(h.back().unwrap(), "https://a.com");
        assert_eq!(h.current(), Some("https://a.com"));
        assert!(h.can_go_forward());
        assert_eq!(h.forward().unwrap(), "https://b.com");
    }

    #[test]
    fn navigate_truncates_forward() {
        let mut h = History::new();
        h.navigate("https://a.com");
        h.navigate("https://b.com");
        h.back().unwrap();
        h.navigate("https://c.com"); // should drop the forward entry b.com
        assert!(!h.can_go_forward());
        assert_eq!(h.current(), Some("https://c.com"));
        assert_eq!(h.len(), 2);
    }

    #[test]
    fn back_on_empty_errors() {
        let mut h = History::new();
        assert!(matches!(h.back(), Err(Error::Navigation(_))));
    }

    #[test]
    fn forward_at_tip_errors() {
        let mut h = History::new();
        h.navigate("https://a.com");
        assert!(matches!(h.forward(), Err(Error::Navigation(_))));
    }
}
