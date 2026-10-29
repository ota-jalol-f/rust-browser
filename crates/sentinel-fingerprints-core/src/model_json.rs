//! `FormFingerprintsV2.GetFingerprints()` → `ModelFingerprint` → JSON, exactly as
//! pv-winform serialized it (`ObectsExtensions.ToJson(obj, ingoreNullValues: true)`
//! = Newtonsoft, nulls omitted, the model's property order, `byte[]` as base64,
//! `FingerStatus` as a number under the lower-camel `fingerStatusN`).
//!
//! Pinned byte-for-byte against the .NET Framework golden strings in the crate's
//! tests (`ModelFingerprintJsonTests`).

use crate::base64;
use crate::finger::{EnrolledFinger, FingerStatus};
use crate::texts;

/// `GetFingerprints` rejected an Excist finger with no WSQ — C#'s
/// `AsbtException(0, "Бармоқ излари тўлиқ берилмаган")`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncompleteError;

impl IncompleteError {
    /// The message C# showed (`AnswereComment`).
    pub fn message(&self) -> &'static str {
        texts::FINGERPRINTS_INCOMPLETE
    }
}

impl std::fmt::Display for IncompleteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.message())
    }
}

impl std::error::Error for IncompleteError {}

/// `Asbt.Media.Models.ModelFingerprint` (the fields `Enroll` emits). `Id`/`Table`
/// are always null (omitted); `fingerStatusN` defaults to 3 (Absent), as a fresh C#
/// model does.
#[derive(Debug, Clone)]
pub struct Dto {
    finger: [Option<Vec<u8>>; 10],
    quality: [Option<String>; 10],
    status: [i32; 10],
}

impl Default for Dto {
    fn default() -> Self {
        Dto {
            finger: Default::default(),
            quality: Default::default(),
            status: [3; 10],
        }
    }
}

impl Dto {
    /// A fresh model (every `fingerStatusN` = 3, everything else null).
    pub fn new() -> Self {
        Self::default()
    }

    fn set(&mut self, finger: i32, wsq: Option<Vec<u8>>, quality: String, status: i32) {
        if (1..=10).contains(&finger) {
            let i = (finger - 1) as usize;
            self.finger[i] = wsq;
            self.quality[i] = Some(quality);
            self.status[i] = status;
        }
    }
}

/// `GetFingerprints()`: throw [`IncompleteError`] when an Excist finger has no WSQ
/// (null or shorter than 2 bytes), else map every finger into a [`Dto`].
pub fn build(fingers: &[EnrolledFinger]) -> Result<Dto, IncompleteError> {
    let mut model = Dto::new();
    for item in fingers {
        if item.status == FingerStatus::Excist
            && item.wsq.as_ref().map(|w| w.len() < 2).unwrap_or(true)
        {
            return Err(IncompleteError);
        }
        // item.Quality.ToStr(): None → "", else the integer.
        let quality = item.quality.map(|q| q.to_string()).unwrap_or_default();
        model.set(item.number(), item.wsq.clone(), quality, item.status as i32);
    }
    Ok(model)
}

/// Serialize a [`Dto`] exactly as `JsonConvert.SerializeObject(model, {Ignore,None})`:
/// nulls omitted, `Id, Table, Finger1, FingerQuality1, fingerStatus1, …` order.
pub fn serialize(model: &Dto) -> String {
    let mut parts: Vec<String> = Vec::new();
    // Id, Table are always null → omitted.
    for n in 0..10 {
        if let Some(wsq) = &model.finger[n] {
            parts.push(format!("\"Finger{}\":\"{}\"", n + 1, base64::encode(wsq)));
        }
        if let Some(q) = &model.quality[n] {
            parts.push(format!("\"FingerQuality{}\":{}", n + 1, json_string(q)));
        }
        parts.push(format!("\"fingerStatus{}\":{}", n + 1, model.status[n]));
    }
    format!("{{{}}}", parts.join(","))
}

/// A JSON string literal (quotes + the escapes Newtonsoft/`serde` produce). base64
/// and quality values need none, but escape defensively.
fn json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
