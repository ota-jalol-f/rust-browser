//! Built-in Rust plugins registered into the shell's [`PluginRegistry`], and the
//! factory that assembles them. Hardware plugins (fingerprint, ID reader,
//! signature pad, camera) land here over the coming phases; for now a headless
//! `Asbt.System` plugin and a streaming `Asbt.Reader` demo prove the
//! `window.share` bridge (one-shot and streaming) end to end.

use sentinel_core::{Error, Result};
use sentinel_ipc::{Request, Response};
use sentinel_plugin::{
    Plugin, PluginDescriptor, PluginKind, PluginRegistry, PluginResult, PluginStream, PluginUi,
    StreamStep, UiStep,
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

/// A demo MODAL UI plugin: shows a confirm dialog in a full-window overlay and
/// resolves with `confirmed` / cancel. Proves `window.share.RunPluginUi` end to
/// end. Real UI plugins (signature pad, ID reader, camera) replace this.
pub struct ConfirmPlugin;

/// The overlay's HTML. Opaque full window (no backdrop blending needed), a
/// centred card, a `#msg` filled from the host via `window.ui.onUpdate`
/// (`textContent`, never HTML), and OK / Cancel buttons that `window.ui.send`.
const CONFIRM_HTML: &str = r##"<!DOCTYPE html>
<html><head><meta charset="utf-8"><style>
  html, body { margin: 0; height: 100%; background: rgba(13,17,23,0.001);
    font-family: "Segoe UI", Arial, sans-serif; }
  #scrim { position: fixed; inset: 0; background: #0d1117e6;
    display: flex; align-items: center; justify-content: center; }
  #card { width: 360px; background: #20242b; color: #e6edf3; border: 1px solid #444c56;
    border-radius: 10px; padding: 20px; box-shadow: 0 10px 40px #000a; }
  #msg { font-size: 15px; line-height: 1.5; margin: 0 0 18px; min-height: 22px; }
  .row { display: flex; gap: 10px; justify-content: flex-end; }
  button { height: 34px; padding: 0 16px; border-radius: 6px; cursor: pointer; font-size: 13px;
    border: 1px solid #444c56; background: #2d333b; color: #cdd9e5; }
  button:hover { background: #373e47; }
  #ok { background: #d9772b; border-color: #d9772b; color: #fff; }
</style></head><body>
  <div id="scrim"><div id="card">
    <p id="msg"></p>
    <div class="row">
      <button id="cancel" onclick="window.ui && window.ui.send('cancel')">Bekor</button>
      <button id="ok" onclick="window.ui && window.ui.send('ok')">OK</button>
    </div>
  </div></div>
<script>
  window.ui = window.ui || {};
  window.ui.onUpdate = function (j) {
    try { document.getElementById('msg').textContent = JSON.parse(j).message || ''; } catch (e) {}
  };
</script></body></html>
"##;

struct ConfirmUi {
    answer: Option<PluginResult>,
    pushed: bool,
}

impl PluginUi for ConfirmUi {
    fn html(&self) -> String {
        CONFIRM_HTML.to_string()
    }
    fn on_message(&mut self, message: &str) {
        self.answer = Some(match message {
            "ok" => PluginResult::ok("confirmed"),
            _ => PluginResult::cancel(),
        });
    }
    fn poll(&mut self) -> UiStep {
        if let Some(r) = self.answer.take() {
            return UiStep::Done(r);
        }
        if !self.pushed {
            self.pushed = true;
            // Data reaches the overlay only through this channel, never embedded
            // in html() — so it can't inject markup.
            return UiStep::Update(r#"{"message":"Davom etilsinmi?"}"#.to_string());
        }
        UiStep::Pending
    }
}

impl Plugin for ConfirmPlugin {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor {
            id: "Asbt.Confirm".to_string(),
            name: "confirm".to_string(),
            version: "1.0.0".to_string(),
            kind: PluginKind::Ui,
        }
    }
    fn execute(&mut self, _request: &str) -> PluginResult {
        PluginResult::fail("Asbt.Confirm is a UI plugin; use RunPluginUi")
    }
    fn execute_ui(&mut self, _request: &str) -> Option<Box<dyn PluginUi>> {
        Some(Box::new(ConfirmUi { answer: None, pushed: false }))
    }
}

/// Build the registry of built-in plugins the shell ships with.
pub fn builtin_registry() -> PluginRegistry {
    let mut registry = PluginRegistry::new();
    // Distinct ids — registration cannot clash; ignore the Result.
    let _ = registry.register(Box::new(SystemPlugin));
    let _ = registry.register(Box::new(ReaderPlugin));
    let _ = registry.register(Box::new(ConfirmPlugin));
    // idbox runs out of process (an x86 helper, for the real Elyctis chip SDK).
    // Register a proxy that forwards to the helper; skip if it can't be spawned.
    match spawn_idbox_proxy() {
        Ok(proxy) => {
            let _ = registry.register(Box::new(proxy));
        }
        Err(e) => eprintln!("[plugins] idbox helper unavailable: {e}"),
    }
    registry
}

/// A host-side proxy for a plugin that actually runs in an out-of-process helper.
/// `execute` forwards the request over the pipe and returns the helper's envelope.
/// Owns the helper so its lifetime matches the registry.
///
/// NOTE: `execute` blocks on the pipe round-trip. That is fine for fast calls
/// (and the current fake device); a long modal op (real ReadDocument) will need
/// the async helper-call path so it doesn't stall the frame loop.
struct IdBoxProxy {
    helper: sentinel_ipc_pipe::Helper,
}

impl Plugin for IdBoxProxy {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor {
            id: "Asbt.IdBox".to_string(),
            name: "idbox".to_string(),
            version: "1.0.0".to_string(),
            kind: PluginKind::Ui,
        }
    }

    fn execute(&mut self, request: &str) -> PluginResult {
        let call = Request::Plugin { id: "Asbt.IdBox".to_string(), request: request.to_string() };
        match self.helper.client().call(call) {
            Ok(Response::Plugin { envelope }) => serde_json::from_str::<PluginResult>(&envelope)
                .unwrap_or_else(|_| PluginResult::fail("idbox: bad helper envelope")),
            Ok(other) => PluginResult::fail(format!("idbox: unexpected helper response: {other:?}")),
            Err(e) => PluginResult::fail(format!("idbox helper error: {e}")),
        }
    }
}

/// Spawn the idbox helper (`sentinel-ipc-helper`, next to the running exe) and
/// wrap it as a proxy plugin.
fn spawn_idbox_proxy() -> Result<IdBoxProxy> {
    let exe =
        std::env::current_exe().map_err(|e| Error::Engine(format!("locate current exe: {e}")))?;
    let dir = exe.parent().ok_or_else(|| Error::Engine("exe has no parent dir".to_string()))?;
    let helper_name =
        if cfg!(windows) { "sentinel-ipc-helper.exe" } else { "sentinel-ipc-helper" };
    let helper = sentinel_ipc_pipe::spawn_helper(&dir.join(helper_name))?;
    Ok(IdBoxProxy { helper })
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

    #[test]
    fn confirm_plugin_is_a_ui_op_that_resolves_on_ok() {
        let mut reg = builtin_registry();
        let mut ui = reg.dispatch_ui("Asbt.Confirm", "{}").expect("should be a UI op");
        assert!(ui.html().contains("window.ui"));
        // plugin -> overlay: one update is pushed first.
        assert!(matches!(ui.poll(), UiStep::Update(_)));
        // overlay -> plugin: OK resolves it with "confirmed".
        ui.on_message("ok");
        match ui.poll() {
            UiStep::Done(r) => assert_eq!(r, PluginResult::ok("confirmed")),
            _ => panic!("expected completion after ok"),
        }
    }
}
