//! `sentinel-widget`: the `Asbt.Widget` data plugin, the in-process Rust
//! equivalent of the reference `Asbt.Widget.Plugin`'s `WidgetExtension`
//! (`IBrowserExtension`, `Id = "Asbt.Widget"`, `Name = "widget"`).
//!
//! # What the reference does
//! `WidgetExtension.RunAsync(payload, user)` validates/decrypts the Oracle
//! connection string (`GetConnStr` → `CCrypt.DecryptRijndael`) and then would
//! query per-region protocol counts (`ADMPROTOKOL.PACK_TB_WIDGET.GET_PROT_COUNT_VIL`
//! via `RdmWorks`) into a `List<RepModel>`. That Oracle call is **commented out**
//! in the reference (`RdmWorks` is missing from `Asbt.MyUtils 1.1.36`), so the
//! reference's success path returns an **empty list**. Any exception is logged and
//! `RunAsync` returns `null`.
//!
//! The reference host bridge (`CallbackObjectForJs.RunPlugin`) serializes an
//! `IBrowserExtension` return value **raw**: `JsonSerializer.Serialize([])` → `"[]"`
//! (or `"null"` from the catch; the dev build's `MyBrowser.Host` has no
//! `OracleServerGUBDDMVD` connection string, so `GetConnStr` throws and the
//! reference actually answers `"null"` there).
//!
//! # What this port does
//! The Oracle query, `GetConnStr` and the `CCrypt` Rijndael decrypt are **server-
//! side config/DB** concerns for a client browser that has no Oracle, so they are
//! out of scope. The plugin returns the reference's **intended success shape** — an
//! empty widget-row array (`[]`), the correct representation of "no rows", rather
//! than reproducing the dev build's misconfiguration `null`.
//!
//! Unlike the reference bridge (which returned the `IBrowserExtension` value raw),
//! the Rust shell resolves **every** `share.RunPlugin` result through the
//! `PluginResult` envelope (`sentinel-shell`'s `dispatch_plugin_request`), the same
//! as every other ported plugin. That is harmless here: pv-next (dist at
//! `E:/work/vue/pv-next`) does **not** call this plugin — there is no
//! `widget-plugin.service` bundle, no `hostObjects.widget`, and no `Asbt.Widget`
//! reference anywhere in the front-end. The plugin is registered for parity/
//! completeness and so that `share.GetPlugins()` lists it exactly as the reference
//! did.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use serde::Serialize;
use sentinel_plugin::{HostMethod, Plugin, PluginDescriptor, PluginKind, PluginResult};

/// One widget report row, mirroring the reference `RepModel`
/// (`string Name; List<dynamic> Values`). Serialized to the reference's
/// `System.Text.Json` shape — PascalCase `Name` / `Values` — so the contract is
/// pinned if a data source is ever wired in.
#[derive(Debug, Clone, Serialize)]
pub struct RepModel {
    /// The report series name (`RepModel.Name`).
    #[serde(rename = "Name")]
    pub name: String,
    /// The report values (`RepModel.Values`, an array of arbitrary JSON scalars/
    /// objects — the reference's `List<dynamic>`).
    #[serde(rename = "Values")]
    pub values: Vec<serde_json::Value>,
}

/// The widget report rows. The Oracle-backed `GET_PROT_COUNT_VIL` query is not
/// available here (it is commented out in the reference too), so this is empty —
/// the reference's success shape with the Oracle backend absent.
pub fn widget_rows() -> Vec<RepModel> {
    Vec::new()
}

/// Serialize widget rows to the JSON array the reference returned
/// (`JsonSerializer.Serialize(List<RepModel>)`): `[]` when empty, otherwise
/// `[{"Name":…,"Values":[…]}]`.
pub fn rows_json(rows: &[RepModel]) -> String {
    serde_json::to_string(rows).unwrap_or_else(|_| "[]".to_string())
}

/// The `Asbt.Widget` data plugin.
pub struct WidgetPlugin;

impl WidgetPlugin {
    /// Create the widget plugin.
    pub fn new() -> Self {
        WidgetPlugin
    }
}

impl Default for WidgetPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl Plugin for WidgetPlugin {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor {
            id: "Asbt.Widget".to_string(),
            name: "widget".to_string(),
            version: "1.0.0".to_string(),
            kind: PluginKind::Extension,
        }
    }

    fn host_methods(&self) -> Vec<HostMethod> {
        // The reference `WidgetExtension` is an `IBrowserExtension` with an empty
        // `Init` — it registers NO WebView2 host object. So there is no
        // `hostObjects.widget`; it is reached only via `share.RunPlugin`.
        Vec::new()
    }

    fn execute(&mut self, _request: &str) -> PluginResult {
        // The reference ignores the payload on the success path (it is used only in
        // error logging) and returns the row list. The rows are empty (Oracle
        // query unavailable), so this is `"[]"`.
        PluginResult::ok(rows_json(&widget_rows()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptor_matches_the_reference() {
        let d = WidgetPlugin::new().descriptor();
        assert_eq!(d.id, "Asbt.Widget");
        assert_eq!(d.name, "widget");
        assert_eq!(d.version, "1.0.0");
        assert_eq!(d.kind, PluginKind::Extension);
    }

    #[test]
    fn no_host_object_like_the_reference_ibrowserextension() {
        // Empty Init in the reference → no host object; RunPlugin-only.
        assert!(WidgetPlugin::new().host_methods().is_empty());
    }

    #[test]
    fn run_returns_an_empty_row_array_and_succeeds() {
        let mut p = WidgetPlugin::new();
        // Payload is ignored (as in the reference success path).
        let r = p.execute(r#"{"method":"run","args":[]}"#);
        assert!(r.success);
        assert!(r.error_message.is_empty());
        assert_eq!(r.data, "[]");
    }

    #[test]
    fn empty_payload_also_returns_empty_array() {
        let mut p = WidgetPlugin::new();
        assert_eq!(p.execute("").data, "[]");
    }

    #[test]
    fn repmodel_serializes_to_the_reference_pascalcase_shape() {
        // Pins the System.Text.Json shape: PascalCase Name/Values, Values a raw
        // JSON array. (The live path never emits rows, but the contract is fixed.)
        let rows = vec![RepModel {
            name: "Toshkent".to_string(),
            values: vec![serde_json::json!(12), serde_json::json!(34)],
        }];
        assert_eq!(rows_json(&rows), r#"[{"Name":"Toshkent","Values":[12,34]}]"#);
    }

    #[test]
    fn empty_rows_serialize_to_bracket_pair() {
        assert_eq!(rows_json(&widget_rows()), "[]");
    }
}
