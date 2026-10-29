//! The signature overlay: a [`PluginUi`] that captures a signature on an HTML
//! `<canvas>` and runs the [`SignPadSession`] over it.
//!
//! The reference opened a modal window hosting a Topaz pad; the shell's overlay is
//! its equivalent, with the canvas standing in for the pad (there is no native
//! Topaz SDK and .NET is forbidden). Host → overlay goes through
//! `window.ui.onUpdate` (labels, the too-small warning); overlay → host through
//! `window.ui.send`: `clear`, `cancel`, and `save:<base64 PNG>`.

use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use sentinel_plugin::{PluginResult, PluginUi, UiStep};
use sentinel_signpad_core::image::Image;
use sentinel_signpad_core::session::{SaveOutcome, SignPadSession, Tablet};
use sentinel_signpad_core::{base64, texts};

/// The signature canvas's pixels, shared between the overlay and its [`Tablet`].
type Canvas = Rc<RefCell<Option<Image>>>;

/// The `Tablet` backed by the overlay canvas: `get_image` returns the last PNG the
/// overlay sent on save (or a blank white pad), so the session's logic is unchanged.
struct CanvasTablet {
    current: Canvas,
}

impl Tablet for CanvasTablet {
    fn open(&mut self) {}
    fn clear(&mut self) {
        *self.current.borrow_mut() = None;
    }
    fn get_image(&mut self) -> Image {
        self.current.borrow().clone().unwrap_or_else(|| Image::white(495, 285))
    }
    fn close(&mut self) {}
}

/// The signature overlay operation.
pub struct SignatureUi {
    session: SignPadSession,
    current: Canvas,
    _busy: BusyGuard,
    outcome: Option<PluginResult>,
    pending_warning: bool,
    labels_sent: bool,
}

impl SignatureUi {
    /// Open the overlay (runs `FormSignature_Load` via the session).
    pub fn new(busy: BusyGuard) -> Self {
        let current: Canvas = Rc::new(RefCell::new(None));
        let mut session = SignPadSession::new(Box::new(CanvasTablet { current: current.clone() }));
        session.open();
        SignatureUi {
            session,
            current,
            _busy: busy,
            outcome: None,
            pending_warning: false,
            labels_sent: false,
        }
    }
}

impl PluginUi for SignatureUi {
    fn html(&self) -> String {
        OVERLAY_HTML.to_string()
    }

    fn on_message(&mut self, message: &str) {
        if message == "clear" {
            self.session.clear();
        } else if message == "cancel" {
            self.session.close();
            self.outcome = Some(PluginResult::cancel());
        } else if let Some(b64) = message.strip_prefix("save:") {
            // Decode the canvas PNG into the tablet, then run sbSave_Click.
            if let Some(image) = base64::decode(b64).and_then(|p| sentinel_signpad_core::decode_png(&p)) {
                *self.current.borrow_mut() = Some(image);
            }
            match self.session.save() {
                SaveOutcome::TooSmall => self.pending_warning = true,
                SaveOutcome::Accepted => {
                    let data = self.session.jpeg().map(base64::encode).unwrap_or_default();
                    self.outcome = Some(PluginResult::ok(data));
                }
            }
        }
    }

    fn poll(&mut self) -> UiStep {
        if let Some(result) = self.outcome.take() {
            return UiStep::Done(result);
        }
        if !self.labels_sent {
            self.labels_sent = true;
            return UiStep::Update(
                serde_json::json!({
                    "labels": {
                        "title": texts::WINDOW_TITLE,
                        "caption": texts::CAPTION_LINE,
                        "clear": texts::CLEAR,
                        "save": texts::SAVE,
                    }
                })
                .to_string(),
            );
        }
        if self.pending_warning {
            self.pending_warning = false;
            return UiStep::Update(
                serde_json::json!({
                    "warning": texts::TOO_SMALL_MESSAGE,
                    "caption": texts::TOO_SMALL_CAPTION,
                })
                .to_string(),
            );
        }
        UiStep::Pending
    }

    fn on_close(&mut self) -> PluginResult {
        self.session.close();
        PluginResult::cancel()
    }
}

/// Marks the plugin busy while the overlay is open; cleared on drop (so Esc / a tab
/// navigation can never lock SignPad).
pub struct BusyGuard(Weak<Cell<bool>>);

impl BusyGuard {
    /// Take the busy flag (sets it true). The caller must have checked it was free.
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

/// The overlay document. The signature is drawn on a white 495×285 canvas; on save
/// the canvas PNG is sent to the host. All text arrives via `window.ui.onUpdate`.
const OVERLAY_HTML: &str = r##"<!DOCTYPE html>
<html><head><meta charset="utf-8"><style>
  html,body{margin:0;height:100%;font-family:"Segoe UI",Arial,sans-serif;background:#0d1117;color:#e6edf3}
  #wrap{position:fixed;inset:0;display:flex;flex-direction:column;align-items:center;justify-content:center;gap:12px;padding:18px;box-sizing:border-box}
  #title{font-size:15px}
  #pad{background:#fff;border:1px solid #30363d;border-radius:6px;touch-action:none;cursor:crosshair}
  #caption{color:#7d8590;font-size:12px;width:586px;text-align:right}
  .row{display:flex;gap:10px;width:586px;justify-content:space-between}
  button{height:38px;padding:0 18px;border-radius:6px;border:1px solid #444c56;background:#2d333b;color:#cdd9e5;font-size:13px;cursor:pointer}
  button:hover{background:#373e47}
  #save{background:#d9772b;border-color:#d9772b;color:#fff}
  #exit{padding:0 14px}
  #box{position:fixed;inset:0;display:none;align-items:center;justify-content:center;background:#0008}
  #boxCard{background:#20242b;border:1px solid #444c56;border-radius:10px;padding:18px 20px;max-width:440px}
  #boxText{margin:0 0 14px}
</style></head><body>
  <div id="wrap">
    <div id="title"></div>
    <canvas id="pad" width="495" height="285"></canvas>
    <div id="caption"></div>
    <div class="row">
      <button id="clear" onclick="clearPad()"></button>
      <div style="display:flex;gap:10px">
        <button id="save" onclick="S('save:'+pngB64())"></button>
        <button id="exit" onclick="S('cancel')">&#10005;</button>
      </div>
    </div>
  </div>
  <div id="box"><div id="boxCard"><p id="boxText"></p><div class="row" style="width:auto;justify-content:flex-end"><button onclick="hideBox()">OK</button></div></div></div>
<script>
  var pad = document.getElementById('pad'), ctx = pad.getContext('2d');
  function wipe(){ ctx.fillStyle='#fff'; ctx.fillRect(0,0,495,285); ctx.strokeStyle='#000'; ctx.lineWidth=2.5; ctx.lineCap='round'; ctx.lineJoin='round'; }
  wipe();
  var drawing=false;
  function pos(e){ var r=pad.getBoundingClientRect(); return [e.clientX-r.left, e.clientY-r.top]; }
  pad.addEventListener('pointerdown', function(e){ drawing=true; var p=pos(e); ctx.beginPath(); ctx.moveTo(p[0],p[1]); pad.setPointerCapture(e.pointerId); });
  pad.addEventListener('pointermove', function(e){ if(!drawing)return; var p=pos(e); ctx.lineTo(p[0],p[1]); ctx.stroke(); });
  window.addEventListener('pointerup', function(){ drawing=false; });
  function clearPad(){ wipe(); S('clear'); }
  function pngB64(){ return pad.toDataURL('image/png').split(',')[1]; }
  function S(m){ window.ui && window.ui.send(m); }
  function hideBox(){ document.getElementById('box').style.display='none'; }
  window.ui = window.ui || {};
  window.ui.onUpdate = function(j){
    var s; try { s = JSON.parse(j); } catch(e){ return; }
    if (s.labels){
      document.getElementById('title').textContent = s.labels.title;
      document.getElementById('caption').textContent = s.labels.caption;
      document.getElementById('clear').textContent = s.labels.clear;
      document.getElementById('save').textContent = s.labels.save;
    }
    if (s.warning){
      document.getElementById('boxText').textContent = s.warning;
      document.getElementById('box').style.display = 'flex';
    }
  };
  document.addEventListener('keydown', function(e){
    if (e.key==='Enter'){ S('save:'+pngB64()); } else if (e.key==='Escape'){ S('cancel'); }
  });
</script></body></html>
"##;
