//! `sentinel-system`: the `Asbt.System` host object (`system`), the in-process
//! Rust equivalent of the reference `Asbt.System.Plugin`'s `ModelClass`
//! (`E:/Experiments/browser/src/Plugins/Asbt.System.Plugin/ModelClass.cs`).
//!
//! # Contract
//! pv-next drives it through `chrome.webview.hostObjects.system`, matched by the
//! `GetPlugins().name == "system"` entry, via `system-plugin.service.ts`
//! (`ISystemPluginService`). That service uses each method's return value
//! **directly** — it does NOT unwrap the `{data,success,error_message}`
//! `PluginResult` envelope (it imports no `ap`). So every direct method is a
//! **raw** host method ([`HostMethod::run_raw`]): the page receives the
//! JSON-decoded `data`, so each raw method must JSON-encode its value in `data`
//! (`"true"` → `true`, `"\"x\""` → `"x"`, `"null"` → `null`).
//!
//! Only `mbox` is deferred/modal: the bridge's `DEFERRED_METHODS.system` is
//! exactly `['mbox']`, so `mbox` goes through `share.InvokeDeferred` and the
//! modal-overlay (UI) path; every other method is a plain direct call.
//!
//! # Scope
//! The methods pv-next actually consumes (per `ISystemPluginService` and the two
//! `getAppSetting` call sites — `RecordCountOnPage`, `UseReader`) are: `ping`,
//! `isDebug`, `GetGUID`, `mbox`, `UsbEnable`, `UsbDisable`, `GetAppSettings`,
//! `SetAppSettings`. The reference's `ToDate` and `info` have no pv-next caller
//! and are omitted (as `sentinel-widget` omits its absent Oracle backend).
//!
//! - `isDebug` → `true` (the reference is hard-coded `true`).
//! - `GetGUID` → a random v4 GUID in `"N"` format (32 lowercase hex, no dashes),
//!   like `Guid.NewGuid().ToString("N")`.
//! - `GetAppSettings`/`SetAppSettings` → a plaintext key/value JSON file next to
//!   the executable. The reference stop-gaps over `ConfigurationManager.AppSettings`
//!   (the exe config `<appSettings>`); pv-next's own comments treat it as the
//!   workstation key/value store. A missing key returns `null` (never a failure),
//!   so pv-next's defaults apply (`RecordCountOnPage` → 50, `UseReader` → false).
//! - `UsbEnable`/`UsbDisable` → a documented parity stub (resolve `null`). The
//!   reference flips `HKLM\SYSTEM\…\USBSTOR` `Start` = 3/4 (and swallows every
//!   error — it needs admin), but no pv-next caller exercises these, and the
//!   obvious `winreg` crate pulls `windows-sys 0.48`, whose import lib
//!   (`windows.0.48.5.lib`) fails to link for the Win7 `-Zbuild-std` target.
//!   Win7 support is a hard requirement, so the registry write is deferred to a
//!   future Win7-safe path (raw `windows-sys` at a version already linked, behind
//!   a small `unsafe` block) if a caller ever appears.
//! - `mbox` → a modal message overlay with a single OK button (the reference's
//!   WPF `MessageBox.Show`); resolves when acknowledged.
//! - `ping` → no pv-next caller and a faithful version needs raw ICMP + gateway
//!   discovery, so this is a documented parity stub returning an "unavailable"
//!   message (JSON-encoded string), registered so the host-object surface matches.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::path::PathBuf;

use rand_core::{OsRng, RngCore};
use sentinel_plugin::{HostMethod, Plugin, PluginDescriptor, PluginKind, PluginResult, PluginUi, UiStep};
use serde_json::Value;

/// The `ping` parity-stub body (JSON-encoded as the raw return). No pv-next caller
/// exercises `ping`; a faithful implementation needs raw ICMP + gateway lookup.
const PING_UNAVAILABLE: &str = "Тармоқ ҳолати маълумоти MyBrowser'да мавжуд эмас.";

/// Default settings file name, kept next to the executable.
const SETTINGS_FILE: &str = "app-settings.json";

/// The `Asbt.System` plugin (`system` host object).
pub struct SystemPlugin {
    /// The key/value settings file `GetAppSettings`/`SetAppSettings` read and write.
    settings_path: PathBuf,
}

impl SystemPlugin {
    /// Create the plugin, storing app settings in [`SETTINGS_FILE`] next to the
    /// running executable (falling back to the current directory if the executable
    /// path cannot be resolved), matching the reference writing beside the exe.
    pub fn new() -> Self {
        Self { settings_path: default_settings_path() }
    }

    /// Create the plugin with an explicit settings-file path (for tests, so they
    /// never write next to the test binary).
    pub fn with_settings_path(path: impl Into<PathBuf>) -> Self {
        Self { settings_path: path.into() }
    }

    /// `GetAppSettings(name)`: the stored value for `name`, or `null` when unset —
    /// never a failure, so pv-next falls back to its own default. The value is
    /// JSON-encoded into `data` (a stored string → `"\"…\""`); a missing key →
    /// `"null"`.
    fn get_app_setting(&self, name: &str) -> PluginResult {
        let map = self.load_settings();
        match map.get(name) {
            Some(v) => PluginResult::ok(serde_json::to_string(v).unwrap_or_else(|_| "null".into())),
            None => PluginResult::ok("null"),
        }
    }

    /// `SetAppSettings(name, val)`: upsert `name` with `val` stringified the way the
    /// reference's `object.ToString()` would (`true` → `"True"`, numbers verbatim),
    /// then persist. Best-effort (the reference catches and logs write failures);
    /// always resolves `null` so the page never throws. Not on pv-next's path (it
    /// only reads).
    fn set_app_setting(&self, name: &str, val: &Value) -> PluginResult {
        let mut map = self.load_settings();
        map.insert(name.to_string(), Value::String(stringify_dotnet(val)));
        if let Err(e) = self.store_settings(&map) {
            eprintln!("[system] SetAppSettings persist failed: {e}");
        }
        PluginResult::ok("null")
    }

    /// Load the settings object, or an empty map on any error (no file, bad JSON,
    /// a non-object root) — exactly the reference's "missing → default" behaviour.
    fn load_settings(&self) -> serde_json::Map<String, Value> {
        std::fs::read_to_string(&self.settings_path)
            .ok()
            .and_then(|t| serde_json::from_str::<Value>(&t).ok())
            .and_then(|v| v.as_object().cloned())
            .unwrap_or_default()
    }

    /// Persist the settings object via a sibling temp file + atomic rename, so a
    /// crash mid-write cannot truncate the live file.
    fn store_settings(&self, map: &serde_json::Map<String, Value>) -> std::io::Result<()> {
        let mut tmp = self.settings_path.clone();
        tmp.set_extension("json.tmp");
        let body = serde_json::to_string_pretty(&Value::Object(map.clone()))
            .unwrap_or_else(|_| "{}".into());
        std::fs::write(&tmp, body)?;
        std::fs::rename(&tmp, &self.settings_path)
    }
}

impl Default for SystemPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl Plugin for SystemPlugin {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor {
            id: "Asbt.System".to_string(),
            name: "system".to_string(),
            version: "1.0.0".to_string(),
            kind: PluginKind::Ui,
        }
    }

    fn host_methods(&self) -> Vec<HostMethod> {
        // Every method except `mbox` is DIRECT and raw (the pv-next `system`
        // service uses the return value with no envelope unwrap). `mbox` is the
        // one deferred/modal method (`DEFERRED_METHODS.system == ['mbox']`).
        vec![
            HostMethod::run_raw("ping"),
            HostMethod::run_raw("isDebug"),
            HostMethod::run_raw("GetGUID"),
            HostMethod::ui("mbox"),
            HostMethod::run_raw("UsbEnable"),
            HostMethod::run_raw("UsbDisable"),
            HostMethod::run_raw("GetAppSettings"),
            HostMethod::run_raw("SetAppSettings"),
        ]
    }

    fn execute(&mut self, request: &str) -> PluginResult {
        let (method, args) = parse_request(request);
        match method.as_str() {
            m if m.eq_ignore_ascii_case("isDebug") => PluginResult::ok("true"),
            m if m.eq_ignore_ascii_case("GetGUID") => {
                PluginResult::ok(serde_json::to_string(&new_guid_n()).unwrap_or_else(|_| "null".into()))
            }
            m if m.eq_ignore_ascii_case("ping") => {
                PluginResult::ok(serde_json::to_string(PING_UNAVAILABLE).unwrap_or_else(|_| "null".into()))
            }
            // Parity stub (see the crate docs): the reference flips the USBSTOR
            // registry key, but no pv-next caller uses these and a registry dep
            // (winreg → windows-sys 0.48) breaks the Win7 link. Resolve null.
            m if m.eq_ignore_ascii_case("UsbEnable") || m.eq_ignore_ascii_case("UsbDisable") => {
                PluginResult::ok("null")
            }
            m if m.eq_ignore_ascii_case("GetAppSettings") => {
                self.get_app_setting(args.first().and_then(Value::as_str).unwrap_or(""))
            }
            m if m.eq_ignore_ascii_case("SetAppSettings") => {
                let name = args.first().and_then(Value::as_str).unwrap_or("");
                let val = args.get(1).cloned().unwrap_or(Value::Null);
                self.set_app_setting(name, &val)
            }
            // `mbox` is the modal path (execute_ui). If reached as a plain call
            // (e.g. a bare share.RunPlugin), there is no window to open — resolve null.
            m if m.eq_ignore_ascii_case("mbox") => PluginResult::ok("null"),
            other => PluginResult::fail(format!("Asbt.System: unknown method '{other}'")),
        }
    }

    fn execute_ui(&mut self, request: &str) -> Option<Box<dyn PluginUi>> {
        let (method, args) = parse_request(request);
        if !method.eq_ignore_ascii_case("mbox") {
            return None;
        }
        let text = args.first().and_then(Value::as_str).unwrap_or("").to_string();
        Some(Box::new(MboxUi { text, pushed: false, answered: false }))
    }
}

/// Parse a host-method request `{"method":"<js>","args":[…]}` into the method name
/// and its argument array. A malformed request yields an empty method (→ the
/// `execute` fallthrough) and no args.
fn parse_request(request: &str) -> (String, Vec<Value>) {
    let v: Value = serde_json::from_str(request).unwrap_or(Value::Null);
    let method = v.get("method").and_then(Value::as_str).unwrap_or("").to_string();
    let args = v.get("args").and_then(Value::as_array).cloned().unwrap_or_default();
    (method, args)
}

/// A random v4 GUID in .NET `"N"` format: 32 lowercase hex digits, no dashes, with
/// the version (`4`) and variant bits set so it reads as a real v4 GUID.
fn new_guid_n() -> String {
    let mut b = [0u8; 16];
    OsRng.fill_bytes(&mut b);
    b[6] = (b[6] & 0x0f) | 0x40; // version 4
    b[8] = (b[8] & 0x3f) | 0x80; // variant 1 (RFC 4122)
    let mut s = String::with_capacity(32);
    for byte in b {
        s.push_str(&format!("{byte:02x}"));
    }
    s
}

/// Stringify a JSON value the way the reference `object.ToString()` would, so a
/// value `SetAppSettings` stores round-trips as the `.NET` host would have stored
/// it: a string as-is, a bool as `"True"`/`"False"` (C# `Boolean.ToString()`), a
/// number verbatim, null as empty, and anything else as compact JSON.
fn stringify_dotnet(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Bool(b) => if *b { "True" } else { "False" }.to_string(),
        Value::Number(n) => n.to_string(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// Resolve the default settings path: [`SETTINGS_FILE`] next to the executable, or
/// in the current directory if the executable path is unavailable.
fn default_settings_path() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join(SETTINGS_FILE)))
        .unwrap_or_else(|| PathBuf::from(SETTINGS_FILE))
}

/// The `mbox` modal overlay: a message and a single OK button, mirroring the
/// reference's WPF `MessageBox.Show`. The message text is pushed via
/// [`UiStep::Update`] (rendered with `textContent`, never embedded in the HTML),
/// so page data cannot inject markup. Resolves when the user acknowledges.
struct MboxUi {
    text: String,
    pushed: bool,
    answered: bool,
}

/// The overlay document: a centred card with a `#msg` paragraph (filled from the
/// host via `window.ui.onUpdate`) and an OK button that posts back.
const MBOX_HTML: &str = r##"<!DOCTYPE html>
<html><head><meta charset="utf-8"><style>
  html, body { margin: 0; height: 100%; background: rgba(13,17,23,0.001);
    font-family: "Segoe UI", Arial, sans-serif; }
  #scrim { position: fixed; inset: 0; background: #0d1117e6;
    display: flex; align-items: center; justify-content: center; }
  #card { width: 360px; max-width: 80vw; background: #20242b; color: #e6edf3;
    border: 1px solid #444c56; border-radius: 10px; padding: 20px;
    box-shadow: 0 10px 40px #000a; }
  #title { font-size: 13px; color: #9aa7b2; margin: 0 0 10px; }
  #msg { font-size: 15px; line-height: 1.5; margin: 0 0 18px; min-height: 22px;
    white-space: pre-wrap; word-break: break-word; }
  .row { display: flex; justify-content: flex-end; }
  button { height: 34px; padding: 0 20px; border-radius: 6px; cursor: pointer;
    font-size: 13px; border: 1px solid #d9772b; background: #d9772b; color: #fff; }
  button:hover { background: #c56a24; }
</style></head><body>
  <div id="scrim"><div id="card">
    <p id="title">Маълумотнома</p>
    <p id="msg"></p>
    <div class="row"><button id="ok" onclick="window.ui && window.ui.send('ok')">OK</button></div>
  </div></div>
<script>
  window.ui = window.ui || {};
  window.ui.onUpdate = function (j) {
    try { document.getElementById('msg').textContent = JSON.parse(j).message || ''; } catch (e) {}
  };
  document.addEventListener('keydown', function (e) {
    if ((e.key === 'Enter' || e.key === 'Escape') && window.ui) window.ui.send('ok');
  });
</script></body></html>
"##;

impl PluginUi for MboxUi {
    fn html(&self) -> String {
        MBOX_HTML.to_string()
    }

    fn on_message(&mut self, _message: &str) {
        // A message box has one action; any message acknowledges it.
        self.answered = true;
    }

    fn poll(&mut self) -> UiStep {
        if !self.pushed {
            self.pushed = true;
            let msg = Value::String(self.text.clone());
            return UiStep::Update(format!("{{\"message\":{}}}", msg));
        }
        if self.answered {
            return UiStep::Done(PluginResult::ok(""));
        }
        UiStep::Pending
    }

    fn on_close(&mut self) -> PluginResult {
        // Dismissing a message box (Esc / tab closed) is an acknowledgement, not a
        // failure — the reference `MessageBox.Show` returns either way and pv-next
        // ignores the result.
        PluginResult::ok("")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(method: &str, args: &str) -> String {
        format!(r#"{{"method":"{method}","args":{args}}}"#)
    }

    fn sys_in(dir: &std::path::Path) -> SystemPlugin {
        SystemPlugin::with_settings_path(dir.join("app-settings.json"))
    }

    fn tmp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("sentinel-system-test-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn descriptor_matches_the_reference() {
        let d = SystemPlugin::new().descriptor();
        assert_eq!(d.id, "Asbt.System");
        assert_eq!(d.name, "system");
        assert_eq!(d.version, "1.0.0");
        assert_eq!(d.kind, PluginKind::Ui);
    }

    #[test]
    fn host_methods_match_pv_next_contract() {
        let p = SystemPlugin::new();
        let methods: Vec<(String, bool, bool)> =
            p.host_methods().into_iter().map(|m| (m.js, m.ui, m.raw)).collect();
        assert_eq!(
            methods,
            vec![
                ("ping".into(), false, true),
                ("isDebug".into(), false, true),
                ("GetGUID".into(), false, true),
                ("mbox".into(), true, false),
                ("UsbEnable".into(), false, true),
                ("UsbDisable".into(), false, true),
                ("GetAppSettings".into(), false, true),
                ("SetAppSettings".into(), false, true),
            ]
        );
    }

    #[test]
    fn is_debug_data_is_json_boolean_true() {
        let mut p = SystemPlugin::new();
        let r = p.execute(&req("isDebug", "[]"));
        assert!(r.success && r.error_message.is_empty());
        // raw method: data must JSON-decode to a JS boolean `true`.
        assert_eq!(serde_json::from_str::<Value>(&r.data).unwrap(), Value::Bool(true));
    }

    #[test]
    fn get_guid_data_is_32_lowercase_hex_v4() {
        let mut p = SystemPlugin::new();
        let r = p.execute(&req("GetGUID", "[]"));
        let v = serde_json::from_str::<Value>(&r.data).unwrap();
        let guid = v.as_str().expect("GUID decodes to a JS string");
        assert_eq!(guid.len(), 32);
        assert!(guid.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
        assert_eq!(&guid[12..13], "4"); // version nibble
        assert!(matches!(&guid[16..17], "8" | "9" | "a" | "b")); // variant nibble
    }

    #[test]
    fn two_guids_differ() {
        let mut p = SystemPlugin::new();
        let a = p.execute(&req("GetGUID", "[]")).data;
        let b = p.execute(&req("GetGUID", "[]")).data;
        assert_ne!(a, b);
    }

    #[test]
    fn ping_data_is_a_json_string() {
        let mut p = SystemPlugin::new();
        let r = p.execute(&req("ping", "[]"));
        assert!(r.success);
        assert!(serde_json::from_str::<Value>(&r.data).unwrap().is_string());
    }

    #[test]
    fn usb_toggles_never_fail_and_resolve_null() {
        let mut p = SystemPlugin::new();
        for m in ["UsbEnable", "UsbDisable"] {
            let r = p.execute(&req(m, "[]"));
            assert!(r.success, "{m} must not fail");
            assert_eq!(serde_json::from_str::<Value>(&r.data).unwrap(), Value::Null);
        }
    }

    #[test]
    fn get_app_setting_missing_key_is_json_null() {
        let dir = tmp_dir("missing");
        let mut p = sys_in(&dir);
        let r = p.execute(&req("GetAppSettings", r#"["RecordCountOnPage"]"#));
        assert!(r.success && r.error_message.is_empty());
        assert_eq!(serde_json::from_str::<Value>(&r.data).unwrap(), Value::Null);
    }

    #[test]
    fn set_then_get_app_setting_roundtrips_as_a_string() {
        let dir = tmp_dir("roundtrip");
        let mut p = sys_in(&dir);
        assert!(p.execute(&req("SetAppSettings", r#"["RecordCountOnPage","100"]"#)).success);
        let r = p.execute(&req("GetAppSettings", r#"["RecordCountOnPage"]"#));
        // Raw: data JSON-decodes to the string "100", which pv-next parses to 100.
        assert_eq!(serde_json::from_str::<Value>(&r.data).unwrap(), Value::String("100".into()));
    }

    #[test]
    fn set_app_setting_stringifies_a_bool_dotnet_style() {
        let dir = tmp_dir("bool");
        let mut p = sys_in(&dir);
        assert!(p.execute(&req("SetAppSettings", r#"["UseReader",true]"#)).success);
        let r = p.execute(&req("GetAppSettings", r#"["UseReader"]"#));
        assert_eq!(serde_json::from_str::<Value>(&r.data).unwrap(), Value::String("True".into()));
    }

    #[test]
    fn set_app_setting_persists_across_instances() {
        let dir = tmp_dir("persist");
        {
            let mut p = sys_in(&dir);
            assert!(p.execute(&req("SetAppSettings", r#"["UseReader","True"]"#)).success);
        }
        let mut p2 = sys_in(&dir);
        let r = p2.execute(&req("GetAppSettings", r#"["UseReader"]"#));
        assert_eq!(serde_json::from_str::<Value>(&r.data).unwrap(), Value::String("True".into()));
    }

    #[test]
    fn unknown_method_fails() {
        let mut p = SystemPlugin::new();
        let r = p.execute(&req("Nope", "[]"));
        assert!(!r.success);
        assert!(r.error_message.contains("unknown method"));
    }

    #[test]
    fn mbox_opens_a_ui_op_that_resolves_on_acknowledgement() {
        let mut p = SystemPlugin::new();
        // mbox is the UI path, not execute().
        let mut ui = p.execute_ui(&req("mbox", r#"["Салом"]"#)).expect("mbox is a UI op");
        assert!(ui.html().contains("window.ui.send"));
        // First poll pushes the message text to the overlay.
        match ui.poll() {
            UiStep::Update(j) => {
                let v: Value = serde_json::from_str(&j).unwrap();
                assert_eq!(v.get("message").and_then(Value::as_str), Some("Салом"));
            }
            _ => panic!("expected the message update first"),
        }
        assert!(matches!(ui.poll(), UiStep::Pending));
        ui.on_message("ok");
        match ui.poll() {
            UiStep::Done(r) => assert!(r.success),
            _ => panic!("expected completion after acknowledgement"),
        }
    }

    #[test]
    fn non_mbox_has_no_ui_op() {
        let mut p = SystemPlugin::new();
        assert!(p.execute_ui(&req("isDebug", "[]")).is_none());
    }
}
