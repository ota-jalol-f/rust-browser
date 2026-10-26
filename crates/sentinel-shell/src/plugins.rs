//! Built-in Rust plugins registered into the shell's [`PluginRegistry`], and the
//! factory that assembles them. Hardware plugins (fingerprint, ID reader,
//! signature pad, camera) land here over the coming phases; for now a single
//! `Asbt.System` plugin proves the `window.share` bridge end to end.

use sentinel_plugin::{Plugin, PluginDescriptor, PluginKind, PluginRegistry, PluginResult};

/// A minimal built-in plugin mirroring the reference `Asbt.System`: answers
/// `ping` with `pong` and echoes anything else. Headless (no UI).
pub struct SystemPlugin;

impl Plugin for SystemPlugin {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor {
            id: "Asbt.System".to_string(),
            name: "system".to_string(),
            version: "1.0.0".to_string(),
            kind: PluginKind::Extension,
        }
    }

    fn execute(&mut self, request: &str) -> PluginResult {
        match request.trim() {
            "ping" => PluginResult::ok("pong"),
            other => PluginResult::ok(other),
        }
    }
}

/// Build the registry of built-in plugins the shell ships with.
pub fn builtin_registry() -> PluginRegistry {
    let mut registry = PluginRegistry::new();
    // Registration cannot clash here (distinct ids); ignore the Result.
    let _ = registry.register(Box::new(SystemPlugin));
    registry
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_plugin_pings_and_echoes() {
        let mut reg = builtin_registry();
        assert!(reg.contains("Asbt.System"));
        assert_eq!(reg.dispatch("Asbt.System", "ping"), PluginResult::ok("pong"));
        assert_eq!(reg.dispatch("Asbt.System", "hi"), PluginResult::ok("hi"));
    }
}
