//! The two JSON texts of a successful read (pv-winform `readerResult = Answere<ReaderResult>`):
//!
//! - [`to_plugin_data`]: what `idbox.ReadDocument()` returns in `PluginResult.data`
//!   (`AnswereId`/`AnswereMessage`/`AnswereComment`/`Data`/`AlertText`).
//! - [`to_csharp_json`]: `readerResult.ToJson()` exactly as pv-winform shows it as the
//!   body of the "Ҳужжатни ўқиш якунланди" alert (Newtonsoft, nulls omitted, `Data`
//!   first, enums as numbers, and a photo rendered as `"System.Drawing.Bitmap"`).
//!
//! Byte-for-byte against the .NET Framework output in `tests/fixtures/reference.json`.
//! Ported from `Asbt.IdBox.Plugin.Mrz.AnswereJson`. The object is assembled by hand (in
//! the C# insertion order) so the bytes do not depend on a JSON library's key ordering;
//! only string and number *values* go through `serde_json`, whose escaping and shortest
//! float formatting match Newtonsoft's defaults for this data (ASCII `<`/`/` and Cyrillic
//! left unescaped, no trailing zeros).

use crate::reader_result::ReaderResult;

/// C#'s `Bitmap` has a `TypeConverter`, so Newtonsoft writes its `ToString()`.
pub const BITMAP_TO_STRING: &str = "System.Drawing.Bitmap";

/// An ordered JSON object writer: fields render in insertion order, `{...}`, no spaces.
struct Obj {
    out: String,
}

impl Obj {
    fn new() -> Self {
        Obj { out: String::from("{") }
    }

    fn raw(&mut self, key: &str, raw_value: &str) {
        if self.out.len() > 1 {
            self.out.push(',');
        }
        self.out.push_str(&quote(key));
        self.out.push(':');
        self.out.push_str(raw_value);
    }

    fn str_field(&mut self, key: &str, value: &str) {
        self.raw(key, &quote(value));
    }

    fn null_field(&mut self, key: &str) {
        self.raw(key, "null");
    }

    fn finish(mut self) -> String {
        self.out.push('}');
        self.out
    }
}

/// A JSON string literal, escaped exactly as Newtonsoft does for our inputs.
fn quote(s: &str) -> String {
    serde_json::to_string(s).expect("string is always serializable")
}

/// A JSON number for a `f64`, shortest round-trip (matches Newtonsoft for our values).
fn number(f: f64) -> String {
    serde_json::to_string(&f).expect("finite reading time")
}

/// `DataObject`: the `Data` member, in the C# field order. `include_nulls` keeps null
/// string fields (`ToPluginData`) or drops them (`ToCSharpJson`); `photo` is the already
/// chosen `PersonPhoto` value (base64, `"System.Drawing.Bitmap"`, or `None`).
fn data_object(r: &ReaderResult, include_nulls: bool, photo: Option<&str>) -> String {
    let mut o = Obj::new();
    let add = |o: &mut Obj, name: &str, value: Option<&str>| match value {
        Some(v) => o.str_field(name, v),
        None if include_nulls => o.null_field(name),
        None => {}
    };

    add(&mut o, "Additional", r.additional.as_deref());
    add(&mut o, "BirthDate", r.birth_date.as_deref());
    add(&mut o, "DocumentNumber", r.document_number.as_deref());
    add(&mut o, "DocumentType", r.document_type.as_deref());
    add(&mut o, "ExpiryDate", r.expiry_date.as_deref());
    add(&mut o, "Issuer", r.issuer.as_deref());
    add(&mut o, "Nationality", r.nationality.as_deref());
    add(&mut o, "Citizenship", r.citizenship.as_deref());
    add(&mut o, "Pinpp", r.pinpp.as_deref());
    add(&mut o, "Surname", r.surname.as_deref());
    add(&mut o, "Name", r.name.as_deref());
    add(&mut o, "Sex", r.sex.as_deref());
    add(&mut o, "Line1", r.line1.as_deref());
    add(&mut o, "Line2", r.line2.as_deref());
    add(&mut o, "Line3", r.line3.as_deref());
    add(&mut o, "PersonPhoto", photo);
    o.raw("DocumentReadingTime", &number(r.document_reading_time));
    o.raw("InformationFrom", &r.information_from.as_i32().to_string());
    o.finish()
}

/// `ToPluginData`: the full envelope `idbox.ReadDocument()` returns in `PluginResult.data`.
/// Every `ReaderResult` member is present (null when C# has null); `InformationFrom` is 0
/// (Chip) or 1 (MRZ); `AlertText` is [`to_csharp_json`] as an escaped string.
pub fn to_plugin_data(answere_id: i64, answere_message: &str, answere_comment: &str, data: &ReaderResult) -> String {
    let mut root = Obj::new();
    root.raw("AnswereId", &answere_id.to_string());
    root.str_field("AnswereMessage", answere_message);
    root.str_field("AnswereComment", answere_comment);
    root.raw("Data", &data_object(data, true, data.person_photo.as_deref()));
    root.str_field(
        "AlertText",
        &to_csharp_json(answere_id, Some(answere_message), Some(answere_comment), Some(data)),
    );
    root.finish()
}

/// `Answere<ReaderResult>.ToJson()` of pv-winform, byte for byte: `Data` first (when
/// present, nulls omitted, the photo as `"System.Drawing.Bitmap"`), then `AnswereId`,
/// then `AnswereMessage`/`AnswereComment` when non-null.
pub fn to_csharp_json(
    answere_id: i64,
    answere_message: Option<&str>,
    answere_comment: Option<&str>,
    data: Option<&ReaderResult>,
) -> String {
    let mut root = Obj::new();
    if let Some(d) = data {
        let photo = d.person_photo.as_deref().map(|_| BITMAP_TO_STRING);
        root.raw("Data", &data_object(d, false, photo));
    }
    root.raw("AnswereId", &answere_id.to_string());
    if let Some(m) = answere_message {
        root.str_field("AnswereMessage", m);
    }
    if let Some(c) = answere_comment {
        root.str_field("AnswereComment", c);
    }
    root.finish()
}
