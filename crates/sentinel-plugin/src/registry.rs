//! The [`Plugin`] trait, its [`PluginDescriptor`], and the [`PluginRegistry`]
//! that dispatches calls by id.

use serde::Serialize;

use crate::result::PluginResult;

/// What kind of plugin this is (mirrors the reference `IUiPlugin` /
/// `IBrowserExtension`). A `Ui` plugin opens a modal window; an `Extension` is
/// headless.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PluginKind {
    /// Opens its own modal UI (e.g. signature pad, ID reader, camera).
    Ui,
    /// Headless; returns a value with no UI.
    Extension,
}

/// A plugin's identity and metadata; serialized for `share.GetPlugins()`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PluginDescriptor {
    /// Stable id used by `share.RunPlugin(id, …)` (e.g. `"Asbt.SignPad"`).
    pub id: String,
    /// Host-object / display name (e.g. `"signpad"`).
    pub name: String,
    /// Semantic version string.
    pub version: String,
    /// Whether it is a UI or headless plugin.
    pub kind: PluginKind,
}

/// An in-process plugin. One call = one [`Plugin::execute`], returning the
/// [`PluginResult`] envelope. Not required to be `Send`: plugins run on the
/// single Ultralight/UI thread, like the reference's UI plugins.
pub trait Plugin {
    /// This plugin's identity and metadata.
    fn descriptor(&self) -> PluginDescriptor;

    /// Handle one request (an opaque JSON string, plugin-defined) and return the
    /// result envelope. Must not panic; surface problems as
    /// [`PluginResult::fail`].
    fn execute(&mut self, request: &str) -> PluginResult;
}

/// Errors from managing the registry.
#[derive(Debug, thiserror::Error)]
pub enum PluginError {
    /// A plugin with this id is already registered.
    #[error("duplicate plugin id: {0}")]
    DuplicateId(String),
}

/// Owns the registered plugins and dispatches calls by id — the core the `share`
/// JS bridge sits on top of. Registration order is preserved (it is the order
/// `share.GetPlugins()` reports).
#[derive(Default)]
pub struct PluginRegistry {
    plugins: Vec<Box<dyn Plugin>>,
}

impl PluginRegistry {
    /// An empty registry.
    pub fn new() -> Self {
        Self { plugins: Vec::new() }
    }

    /// Register `plugin`, rejecting a duplicate id.
    ///
    /// # Errors
    /// [`PluginError::DuplicateId`] if a plugin with the same id is present.
    pub fn register(&mut self, plugin: Box<dyn Plugin>) -> Result<(), PluginError> {
        let id = plugin.descriptor().id;
        if self.plugins.iter().any(|p| p.descriptor().id == id) {
            return Err(PluginError::DuplicateId(id));
        }
        self.plugins.push(plugin);
        Ok(())
    }

    /// Descriptors of all registered plugins, in registration order
    /// (backs `share.GetPlugins()`).
    pub fn descriptors(&self) -> Vec<PluginDescriptor> {
        self.plugins.iter().map(|p| p.descriptor()).collect()
    }

    /// Whether a plugin with `id` is registered.
    pub fn contains(&self, id: &str) -> bool {
        self.plugins.iter().any(|p| p.descriptor().id == id)
    }

    /// Dispatch `request` to the plugin with `id` (backs
    /// `share.RunPlugin(id, request)`). An unknown id returns a failing envelope
    /// rather than erroring, so page JS always gets a `PluginResult`.
    pub fn dispatch(&mut self, id: &str, request: &str) -> PluginResult {
        match self.plugins.iter_mut().find(|p| p.descriptor().id == id) {
            Some(p) => p.execute(request),
            None => PluginResult::fail(format!("unknown plugin: {id}")),
        }
    }

    /// Number of registered plugins.
    pub fn len(&self) -> usize {
        self.plugins.len()
    }

    /// Whether no plugins are registered.
    pub fn is_empty(&self) -> bool {
        self.plugins.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A trivial plugin: echoes the request back as `data`, and counts calls so
    /// tests can prove `&mut self` state is preserved across dispatch.
    struct EchoPlugin {
        id: String,
        calls: u32,
    }

    impl Plugin for EchoPlugin {
        fn descriptor(&self) -> PluginDescriptor {
            PluginDescriptor {
                id: self.id.clone(),
                name: "echo".into(),
                version: "1.0.0".into(),
                kind: PluginKind::Extension,
            }
        }
        fn execute(&mut self, request: &str) -> PluginResult {
            self.calls += 1;
            PluginResult::ok(format!("{}:{request}", self.calls))
        }
    }

    fn echo(id: &str) -> Box<dyn Plugin> {
        Box::new(EchoPlugin { id: id.into(), calls: 0 })
    }

    #[test]
    fn register_and_dispatch_roundtrip() {
        let mut reg = PluginRegistry::new();
        reg.register(echo("Asbt.Echo")).unwrap();
        assert!(reg.contains("Asbt.Echo"));
        let r = reg.dispatch("Asbt.Echo", "hello");
        assert_eq!(r, PluginResult::ok("1:hello"));
    }

    #[test]
    fn dispatch_preserves_plugin_state() {
        let mut reg = PluginRegistry::new();
        reg.register(echo("Asbt.Echo")).unwrap();
        assert_eq!(reg.dispatch("Asbt.Echo", "a").data, "1:a");
        assert_eq!(reg.dispatch("Asbt.Echo", "b").data, "2:b");
    }

    #[test]
    fn duplicate_id_is_rejected() {
        let mut reg = PluginRegistry::new();
        reg.register(echo("Asbt.Echo")).unwrap();
        let err = reg.register(echo("Asbt.Echo")).unwrap_err();
        assert!(matches!(err, PluginError::DuplicateId(ref id) if id == "Asbt.Echo"));
        assert_eq!(reg.len(), 1);
    }

    #[test]
    fn unknown_id_fails_gracefully() {
        let mut reg = PluginRegistry::new();
        let r = reg.dispatch("Nope", "{}");
        assert!(!r.success);
        assert!(r.error_message.contains("unknown plugin: Nope"));
    }

    #[test]
    fn descriptors_preserve_order_and_serialize() {
        let mut reg = PluginRegistry::new();
        reg.register(echo("A")).unwrap();
        reg.register(echo("B")).unwrap();
        let d = reg.descriptors();
        assert_eq!(d.iter().map(|x| x.id.as_str()).collect::<Vec<_>>(), ["A", "B"]);
        let json = serde_json::to_string(&d[0]).unwrap();
        assert_eq!(json, r#"{"id":"A","name":"echo","version":"1.0.0","kind":"extension"}"#);
    }
}
