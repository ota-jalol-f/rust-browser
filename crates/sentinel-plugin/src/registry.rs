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

/// One method of a plugin's JS host object. The reference browser registered each
/// plugin as a WebView2 host object under its `name` (e.g.
/// `chrome.webview.hostObjects.idbox`) exposing typed methods (`ReadChipUid`,
/// `ReadDocument`, …); pv-next calls those methods directly, NOT only
/// `share.RunPlugin`. The bridge synthesizes the same surface: each method becomes
/// a function that forwards `{"method":"<js>","args":[…]}` to [`Plugin::execute`]
/// (or, when [`HostMethod::ui`], the modal UI path) and resolves the page Promise
/// with the plugin's return value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HostMethod {
    /// The exact JS member name pv-next calls (e.g. `"ReadChipUid"`), case-sensitive.
    pub js: String,
    /// Whether invoking it opens a modal window (routes through the UI overlay),
    /// as opposed to a plain one-shot call.
    pub ui: bool,
}

impl HostMethod {
    /// A plain (non-UI) host method.
    pub fn run(js: impl Into<String>) -> Self {
        Self { js: js.into(), ui: false }
    }
    /// A host method that opens a modal window.
    pub fn ui(js: impl Into<String>) -> Self {
        Self { js: js.into(), ui: true }
    }
}

/// A plugin's JS host object: its `name` (the `chrome.webview.hostObjects.<name>`
/// key and the `GetPlugins().name` pv-next matches on), its dispatch `id`, and its
/// methods. Serialized to the spec the engine builds the host objects from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HostObjectSpec {
    /// Host-object key / `GetPlugins().name` (e.g. `"idbox"`).
    pub name: String,
    /// Dispatch id passed back to `RunPlugin` (e.g. `"Asbt.IdBox"`).
    pub id: String,
    /// The methods exposed on the host object.
    pub methods: Vec<HostMethod>,
}

/// One step of a streaming plugin operation, produced by polling it.
///
/// A streaming plugin (idbox chip-read status, live camera preview, progress)
/// emits any number of [`StreamStep::Chunk`]s before a final [`StreamStep::Done`].
/// [`StreamStep::Pending`] means "nothing this tick, poll me again" — the host
/// polls once per frame, so a long operation never blocks the UI.
pub enum StreamStep {
    /// A chunk to deliver to the page's `onChunk` callback (opaque JSON string).
    Chunk(String),
    /// The operation finished; deliver this as the final result and stop polling.
    Done(PluginResult),
    /// Nothing to report this tick; poll again next frame.
    Pending,
}

/// A running streaming operation the host polls once per frame until it returns
/// [`StreamStep::Done`] (after which it is dropped and never polled again).
pub trait PluginStream {
    /// Advance the operation by one step. Must not panic or block.
    fn poll(&mut self) -> StreamStep;
}

/// One step of a modal UI plugin operation, produced by polling it while its
/// overlay window is open.
pub enum UiStep {
    /// Push a state update (opaque JSON) to the overlay via `window.ui.onUpdate`
    /// — e.g. idbox status, a live-view frame, a countdown. The overlay renders it
    /// with `textContent` (never as HTML), so request/page data cannot inject
    /// markup.
    Update(String),
    /// The operation finished; close the overlay and resolve the call with this.
    Done(PluginResult),
    /// Nothing this tick; poll again next frame.
    Pending,
}

/// A running modal UI plugin operation. The host shows [`PluginUi::html`] in a
/// full-window overlay, routes the overlay's messages to [`PluginUi::on_message`],
/// pushes [`PluginUi::poll`] updates into the overlay, and closes it on
/// [`UiStep::Done`] or when the host force-closes it ([`PluginUi::on_close`] — Esc,
/// or the originating tab navigating away).
pub trait PluginUi {
    /// The overlay's HTML document. MUST NOT embed request/page data (that would
    /// let a page inject markup that auto-confirms the dialog); deliver data via
    /// [`UiStep::Update`] and render it with `textContent`.
    fn html(&self) -> String;

    /// Handle a message from the overlay JS (`window.ui.send(msg)`): update state
    /// only. The outcome is reported by the next [`PluginUi::poll`].
    fn on_message(&mut self, message: &str);

    /// Advance the operation by one step (polled once per frame).
    fn poll(&mut self) -> UiStep;

    /// The result when the host force-closes the overlay (Esc, or the originating
    /// tab closed / navigated). Defaults to a cancel.
    fn on_close(&mut self) -> PluginResult {
        PluginResult::cancel()
    }
}

/// An in-process plugin. A one-shot call is [`Plugin::execute`]; a streaming call
/// is [`Plugin::execute_stream`]; a modal UI call is [`Plugin::execute_ui`]. All
/// return the [`PluginResult`] envelope. Not required to be `Send`: plugins run on
/// the single Ultralight/UI thread, like the reference's UI plugins.
pub trait Plugin {
    /// This plugin's identity and metadata.
    fn descriptor(&self) -> PluginDescriptor;

    /// The methods this plugin exposes as a JS host object
    /// (`chrome.webview.hostObjects.<name>.<method>`). The default is none, meaning
    /// the plugin is reached only through `share.RunPlugin(id, …)`. A plugin that
    /// pv-next drives via named host-object methods (idbox, system, …) overrides
    /// this; each method's call is forwarded to [`Plugin::execute`] (or the UI path)
    /// as `{"method":"<js>","args":[…]}`.
    fn host_methods(&self) -> Vec<HostMethod> {
        Vec::new()
    }

    /// Handle one request (an opaque JSON string, plugin-defined) and return the
    /// result envelope. Must not panic; surface problems as
    /// [`PluginResult::fail`].
    fn execute(&mut self, request: &str) -> PluginResult;

    /// Begin a streaming call. The default returns `None`, meaning this plugin
    /// does not stream — the host then falls back to a single [`Plugin::execute`]
    /// result. A streaming plugin overrides this to return a [`PluginStream`] the
    /// host polls each frame.
    fn execute_stream(&mut self, _request: &str) -> Option<Box<dyn PluginStream>> {
        None
    }

    /// Begin a modal UI call. The default returns `None` (not a UI plugin). A UI
    /// plugin overrides this to return a [`PluginUi`] the host shows in an overlay.
    fn execute_ui(&mut self, _request: &str) -> Option<Box<dyn PluginUi>> {
        None
    }
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

    /// The host-object specs for every registered plugin that exposes host methods,
    /// in registration order. The engine builds `chrome.webview.hostObjects.<name>`
    /// from this; plugins with no host methods are omitted (reached only via
    /// `share.RunPlugin`).
    pub fn host_object_specs(&self) -> Vec<HostObjectSpec> {
        self.plugins
            .iter()
            .filter_map(|p| {
                let methods = p.host_methods();
                if methods.is_empty() {
                    return None;
                }
                let d = p.descriptor();
                Some(HostObjectSpec { name: d.name, id: d.id, methods })
            })
            .collect()
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

    /// Begin a streaming call (backs `share.RunPluginStream(id, request)`).
    ///
    /// Returns the running [`PluginStream`] when the plugin supports streaming.
    /// Otherwise returns `Err` with a [`PluginResult`] to resolve immediately: the
    /// plugin's one-shot `execute` result (graceful fallback for a non-streaming
    /// plugin called via the streaming API), or an unknown-plugin failure.
    pub fn dispatch_stream(
        &mut self,
        id: &str,
        request: &str,
    ) -> Result<Box<dyn PluginStream>, PluginResult> {
        match self.plugins.iter_mut().find(|p| p.descriptor().id == id) {
            Some(p) => match p.execute_stream(request) {
                Some(stream) => Ok(stream),
                None => Err(p.execute(request)),
            },
            None => Err(PluginResult::fail(format!("unknown plugin: {id}"))),
        }
    }

    /// Begin a modal UI call (backs `share.RunPluginUi(id, request)`). Returns the
    /// running [`PluginUi`] when the plugin has one; otherwise `Err` with a
    /// [`PluginResult`] to resolve immediately (a non-UI plugin called via the UI
    /// API falls back to its one-shot `execute`; an unknown id fails).
    pub fn dispatch_ui(
        &mut self,
        id: &str,
        request: &str,
    ) -> Result<Box<dyn PluginUi>, PluginResult> {
        match self.plugins.iter_mut().find(|p| p.descriptor().id == id) {
            Some(p) => match p.execute_ui(request) {
                Some(ui) => Ok(ui),
                None => Err(p.execute(request)),
            },
            None => Err(PluginResult::fail(format!("unknown plugin: {id}"))),
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

    /// A demo streaming operation: emits `total` chunks, then completes.
    struct CounterStream {
        emitted: u32,
        total: u32,
    }

    impl PluginStream for CounterStream {
        fn poll(&mut self) -> StreamStep {
            if self.emitted < self.total {
                self.emitted += 1;
                StreamStep::Chunk(format!("chunk-{}", self.emitted))
            } else {
                StreamStep::Done(PluginResult::ok("complete"))
            }
        }
    }

    /// A demo streaming plugin (shape of idbox chip-read: status chunks then a
    /// final result).
    struct ReaderPlugin;

    impl Plugin for ReaderPlugin {
        fn descriptor(&self) -> PluginDescriptor {
            PluginDescriptor {
                id: "Asbt.Reader".into(),
                name: "reader".into(),
                version: "1.0.0".into(),
                kind: PluginKind::Ui,
            }
        }
        fn execute(&mut self, _request: &str) -> PluginResult {
            PluginResult::fail("Asbt.Reader is a streaming plugin; use RunPluginStream")
        }
        fn execute_stream(&mut self, _request: &str) -> Option<Box<dyn PluginStream>> {
            Some(Box::new(CounterStream { emitted: 0, total: 3 }))
        }
    }

    #[test]
    fn streaming_plugin_emits_chunks_then_final() {
        let mut reg = PluginRegistry::new();
        reg.register(Box::new(ReaderPlugin)).unwrap();
        let mut stream = reg.dispatch_stream("Asbt.Reader", "go").expect("should stream");
        let mut chunks = Vec::new();
        let final_result = loop {
            match stream.poll() {
                StreamStep::Chunk(c) => chunks.push(c),
                StreamStep::Pending => {}
                StreamStep::Done(r) => break r,
            }
        };
        assert_eq!(chunks, ["chunk-1", "chunk-2", "chunk-3"]);
        assert_eq!(final_result, PluginResult::ok("complete"));
    }

    #[test]
    fn non_streaming_plugin_via_stream_api_falls_back_to_one_shot() {
        let mut reg = PluginRegistry::new();
        reg.register(echo("Asbt.Echo")).unwrap();
        // EchoPlugin does not override execute_stream, so the streaming API yields
        // Err carrying its one-shot result.
        match reg.dispatch_stream("Asbt.Echo", "hi") {
            Err(r) => assert_eq!(r, PluginResult::ok("1:hi")),
            Ok(_) => panic!("echo is not a streaming plugin"),
        }
    }

    #[test]
    fn unknown_plugin_stream_errors() {
        let mut reg = PluginRegistry::new();
        match reg.dispatch_stream("Nope", "{}") {
            Err(r) => {
                assert!(!r.success);
                assert!(r.error_message.contains("unknown plugin"));
            }
            Ok(_) => panic!("unknown plugin must not produce a stream"),
        }
    }

    /// A demo modal UI op: pushes two tick updates (plugin -> overlay), then stays
    /// Pending until a message arrives (overlay -> plugin), then completes.
    struct ConfirmUi {
        answered: Option<PluginResult>,
        ticks: u32,
    }

    impl PluginUi for ConfirmUi {
        fn html(&self) -> String {
            // Static HTML only — no request/page data embedded.
            "<html><body><button onclick=\"window.ui.send('ok')\">OK</button></body></html>"
                .to_string()
        }
        fn on_message(&mut self, message: &str) {
            self.answered = Some(match message {
                "ok" => PluginResult::ok("confirmed"),
                _ => PluginResult::cancel(),
            });
        }
        fn poll(&mut self) -> UiStep {
            if let Some(r) = self.answered.take() {
                return UiStep::Done(r);
            }
            if self.ticks < 2 {
                self.ticks += 1;
                return UiStep::Update(format!("{{\"tick\":{}}}", self.ticks));
            }
            UiStep::Pending
        }
    }

    struct ConfirmPlugin;

    impl Plugin for ConfirmPlugin {
        fn descriptor(&self) -> PluginDescriptor {
            PluginDescriptor {
                id: "Asbt.Confirm".into(),
                name: "confirm".into(),
                version: "1.0.0".into(),
                kind: PluginKind::Ui,
            }
        }
        fn execute(&mut self, _request: &str) -> PluginResult {
            PluginResult::fail("Asbt.Confirm is a UI plugin; use RunPluginUi")
        }
        fn execute_ui(&mut self, _request: &str) -> Option<Box<dyn PluginUi>> {
            Some(Box::new(ConfirmUi { answered: None, ticks: 0 }))
        }
    }

    #[test]
    fn ui_plugin_updates_then_resolves_on_message() {
        let mut reg = PluginRegistry::new();
        reg.register(Box::new(ConfirmPlugin)).unwrap();
        let mut ui = reg.dispatch_ui("Asbt.Confirm", "{}").expect("should be a UI op");
        assert!(ui.html().contains("button"));
        // plugin -> overlay updates
        match ui.poll() {
            UiStep::Update(j) => assert!(j.contains("\"tick\":1")),
            _ => panic!("expected first update"),
        }
        match ui.poll() {
            UiStep::Update(j) => assert!(j.contains("\"tick\":2")),
            _ => panic!("expected second update"),
        }
        assert!(matches!(ui.poll(), UiStep::Pending));
        // overlay -> plugin message, then completion on the next poll
        ui.on_message("ok");
        match ui.poll() {
            UiStep::Done(r) => assert_eq!(r, PluginResult::ok("confirmed")),
            _ => panic!("expected completion"),
        }
    }

    #[test]
    fn non_ui_plugin_via_ui_api_falls_back() {
        let mut reg = PluginRegistry::new();
        reg.register(echo("Asbt.Echo")).unwrap();
        match reg.dispatch_ui("Asbt.Echo", "hi") {
            Err(r) => assert_eq!(r, PluginResult::ok("1:hi")),
            Ok(_) => panic!("echo is not a UI plugin"),
        }
    }
}
