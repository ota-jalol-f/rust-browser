//! The E-Kalit PIN overlay: a [`PluginUi`] that collects the token PIN (the
//! reference `PinWindow`), then runs the signing operation through the [`Signer`]
//! and resolves with the `Answere` JSON.
//!
//! Host → overlay: labels via `window.ui.onUpdate`. Overlay → host: `window.ui.send`
//! — `pin:<value>` on submit, `cancel` on close.

use std::cell::Cell;
use std::rc::Rc;

use sentinel_ekalit_core::answere::answere_json;
use sentinel_ekalit_core::models::ModelSignature;
use sentinel_ekalit_core::signer::Signer;
use sentinel_ekalit_core::{parse_certificate, sign_string_v2, texts};
use sentinel_plugin::{PluginResult, PluginUi, UiStep};

/// Which signing operation runs after the PIN is entered.
pub enum PinOp {
    /// `SignStringV1(value)`: sign the string unchanged.
    SignV1(String),
    /// `SignStringV2(data)`: canonicalise the JSON, then sign.
    SignV2(String),
    /// `GetCertificateInfo()`: sign the current time, then parse the certificate.
    Cert,
}

/// The PIN overlay operation.
pub struct PinUi {
    op: PinOp,
    signer: Box<dyn Signer>,
    outcome: Option<PluginResult>,
    finished: bool,
    labels_sent: bool,
    _busy: BusyGuard,
}

impl PinUi {
    /// Build the overlay for `op`, signing through `signer`.
    pub fn new(op: PinOp, signer: Box<dyn Signer>, busy: BusyGuard) -> Self {
        PinUi { op, signer, outcome: None, finished: false, labels_sent: false, _busy: busy }
    }

    fn cancel_answere() -> PluginResult {
        PluginResult::ok(answere_json(10, texts::CANCELLED, "", None))
    }

    /// Run the signing operation with the entered PIN, producing the Answere JSON.
    fn run(&mut self, pin: &str) -> PluginResult {
        if pin.is_empty() {
            // C#: an empty PIN is a cancel (GetPinCode returns null).
            return Self::cancel_answere();
        }
        match &self.op {
            PinOp::SignV1(value) => {
                let value = value.clone();
                self.do_sign(pin, &value, value.clone(), 0)
            }
            PinOp::SignV2(raw) => match sign_string_v2(raw) {
                Ok(canonical) => self.do_sign(pin, &canonical, canonical.clone(), 2),
                Err(e) => PluginResult::ok(answere_json(0, texts::SIGN_ERROR, &e.to_string(), None)),
            },
            PinOp::Cert => match self.signer.sign(pin, &now_iso()) {
                Ok(o) => match parse_certificate(&o.certificate_der) {
                    Some(model) => {
                        PluginResult::ok(answere_json(1, texts::OK, "", Some(model.to_json())))
                    }
                    None => PluginResult::ok(answere_json(
                        0,
                        texts::CERT_INFO_ERROR,
                        "certificate parse failed",
                        None,
                    )),
                },
                Err(e) => PluginResult::ok(answere_json(0, texts::CERT_INFO_ERROR, &e, None)),
            },
        }
    }

    /// `SignUSB` + build the `Answere<ModelSignature>` JSON.
    fn do_sign(&mut self, pin: &str, value_to_sign: &str, model_to_sign: String, version: i32) -> PluginResult {
        match self.signer.sign(pin, value_to_sign) {
            Ok(o) => {
                let model = ModelSignature {
                    model_to_sign,
                    public_key: o.certificate_der,
                    signed_data: o.signed_base64,
                    signed_date: now_iso(),
                    version,
                };
                PluginResult::ok(answere_json(1, texts::OK, &o.user_id, Some(model.to_json())))
            }
            Err(e) => PluginResult::ok(answere_json(0, texts::SIGN_ERROR, &e, None)),
        }
    }
}

impl PluginUi for PinUi {
    fn html(&self) -> String {
        PIN_HTML.to_string()
    }

    fn on_message(&mut self, message: &str) {
        if message == "cancel" {
            self.outcome = Some(Self::cancel_answere());
        } else if let Some(pin) = message.strip_prefix("pin:") {
            let result = self.run(pin);
            self.outcome = Some(result);
        }
    }

    fn poll(&mut self) -> UiStep {
        if self.finished {
            return UiStep::Pending;
        }
        if let Some(result) = self.outcome.take() {
            self.finished = true;
            return UiStep::Done(result);
        }
        if !self.labels_sent {
            self.labels_sent = true;
            return UiStep::Update(
                serde_json::json!({
                    "title": texts::PIN_TITLE,
                    "prompt": texts::PIN_PROMPT,
                    "sign": texts::PIN_SIGN,
                })
                .to_string(),
            );
        }
        UiStep::Pending
    }

    fn on_close(&mut self) -> PluginResult {
        self.finished = true;
        Self::cancel_answere()
    }
}

/// Marks the plugin busy while the overlay is open; cleared on drop.
pub struct BusyGuard(std::rc::Weak<Cell<bool>>);

impl BusyGuard {
    /// Take the busy flag.
    pub fn acquire(flag: &Rc<Cell<bool>>) -> Self {
        flag.set(true);
        BusyGuard(Rc::downgrade(flag))
    }
}

impl Drop for BusyGuard {
    fn drop(&mut self) {
        if let Some(flag) = self.0.upgrade() {
            flag.set(false);
        }
    }
}

/// `DateTime.Now` as an ISO-8601 UTC string (`SignedDate`). Howard Hinnant's
/// days-from-civil, inverted, plus the time of day.
fn now_iso() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) as i64;
    let days = secs.div_euclid(86_400);
    let tod = secs.rem_euclid(86_400);
    let (h, mi, s) = (tod / 3600, (tod % 3600) / 60, tod % 60);
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = y + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}T{h:02}:{mi:02}:{s:02}Z")
}

/// The PIN overlay document. All text arrives through `window.ui.onUpdate`.
const PIN_HTML: &str = r##"<!DOCTYPE html>
<html><head><meta charset="utf-8"><style>
  html,body{margin:0;height:100%;font-family:"Segoe UI",Arial,sans-serif;background:rgba(13,17,23,0.001)}
  #scrim{position:fixed;inset:0;background:#0d1117e6;display:flex;align-items:center;justify-content:center}
  #card{width:300px;background:#20242b;color:#e6edf3;border:1px solid #444c56;border-radius:10px;padding:20px;box-shadow:0 10px 40px #000a;text-align:center}
  #title{font-size:13px;color:#7d8590;margin-bottom:14px}
  #prompt{font-size:14px;margin-bottom:12px}
  input{width:150px;height:34px;font-size:18px;font-weight:700;text-align:center;letter-spacing:4px;background:#161b22;border:1px solid #444c56;border-radius:6px;color:#e6edf3}
  .row{display:flex;gap:10px;justify-content:center;margin-top:16px}
  button{height:34px;padding:0 16px;border-radius:6px;border:1px solid #444c56;background:#2d333b;color:#cdd9e5;font-size:13px;cursor:pointer}
  #sign{background:#d9772b;border-color:#d9772b;color:#fff}
</style></head><body>
  <div id="scrim"><div id="card">
    <div id="title"></div>
    <div id="prompt"></div>
    <input id="pin" type="password" maxlength="10" autofocus>
    <div class="row">
      <button id="cancel" onclick="S('cancel')">&#10005;</button>
      <button id="sign" onclick="submit()"></button>
    </div>
  </div></div>
<script>
  function S(m){ window.ui && window.ui.send(m); }
  function submit(){ S('pin:' + document.getElementById('pin').value); }
  window.ui = window.ui || {};
  window.ui.onUpdate = function(j){
    var s; try { s = JSON.parse(j); } catch(e){ return; }
    document.title = s.title || 'E-Kalit';
    var set = function(id,v){ var e=document.getElementById(id); if(e&&v!=null) e.textContent=v; };
    set('title', s.title); set('prompt', s.prompt); set('sign', s.sign);
  };
  document.addEventListener('keydown', function(e){
    if (e.key==='Enter'){ submit(); } else if (e.key==='Escape'){ S('cancel'); }
  });
  setTimeout(function(){ var p=document.getElementById('pin'); if(p) p.focus(); }, 30);
</script></body></html>
"##;
