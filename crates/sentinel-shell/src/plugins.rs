//! Built-in Rust plugins registered into the shell's [`PluginRegistry`], and the
//! factory that assembles them. Hardware plugins (fingerprint, ID reader,
//! signature pad, camera) land here over the coming phases; for now a headless
//! `Asbt.System` plugin and a streaming `Asbt.Reader` demo prove the
//! `window.share` bridge (one-shot and streaming) end to end.

use sentinel_plugin::{
    Plugin, PluginDescriptor, PluginKind, PluginRegistry, PluginResult, PluginStream, StreamStep,
};

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

/// A demo STREAMING plugin, shaped like the idbox chip read: it emits a few
/// status chunks, then a final result. Proves `window.share.RunPluginStream`
/// end to end. Real hardware plugins replace this with device status / preview.
pub struct ReaderPlugin;

/// The running operation behind [`ReaderPlugin`]. One step per poll (one frame).
struct ReaderStream {
    step: u32,
}

impl PluginStream for ReaderStream {
    fn poll(&mut self) -> StreamStep {
        self.step += 1;
        match self.step {
            1 => StreamStep::Chunk(r#"{"status":"connecting"}"#.to_string()),
            2 => StreamStep::Chunk(r#"{"status":"reading"}"#.to_string()),
            3 => StreamStep::Chunk(r#"{"status":"complete"}"#.to_string()),
            _ => StreamStep::Done(PluginResult::ok(r#"{"read":true}"#)),
        }
    }
}

impl Plugin for ReaderPlugin {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor {
            id: "Asbt.Reader".to_string(),
            name: "reader".to_string(),
            version: "1.0.0".to_string(),
            kind: PluginKind::Ui,
        }
    }

    fn execute(&mut self, _request: &str) -> PluginResult {
        PluginResult::fail("Asbt.Reader is a streaming plugin; use RunPluginStream")
    }

    fn execute_stream(&mut self, _request: &str) -> Option<Box<dyn PluginStream>> {
        Some(Box::new(ReaderStream { step: 0 }))
    }
}

/// Build the registry of built-in plugins the shell ships with.
pub fn builtin_registry() -> PluginRegistry {
    let mut registry = PluginRegistry::new();
    // Distinct ids — registration cannot clash; ignore the Result.
    let _ = registry.register(Box::new(SystemPlugin));
    let _ = registry.register(Box::new(ReaderPlugin));
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

    #[test]
    fn reader_plugin_streams_status_then_final() {
        let mut reg = builtin_registry();
        let mut stream = reg.dispatch_stream("Asbt.Reader", "go").expect("should stream");
        let mut chunks = Vec::new();
        let final_result = loop {
            match stream.poll() {
                StreamStep::Chunk(c) => chunks.push(c),
                StreamStep::Pending => {}
                StreamStep::Done(r) => break r,
            }
        };
        assert_eq!(chunks.len(), 3);
        assert!(chunks[0].contains("connecting"));
        assert_eq!(final_result, PluginResult::ok(r#"{"read":true}"#));
    }
}
