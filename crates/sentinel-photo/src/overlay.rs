//! The document-photo overlay: a [`PluginUi`] that owns a
//! [`DocumentPhotoSession`] and renders `FormCaptureDocument`'s window in the
//! shell's full-window overlay.
//!
//! The reference opened a modal WPF window; the shell's overlay is its equivalent
//! (the camera needs a live view, so — unlike idbox — this plugin uses the UI
//! path). Host → overlay state is pushed with `window.ui.onUpdate`; overlay → host
//! actions arrive through `window.ui.send` ([`PluginUi::on_message`]): `capture`,
//! `save`, `reload`, `cancel`.

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use sentinel_plugin::{PluginResult, PluginUi, UiStep};

use sentinel_photo_core::camera::Camera;
use sentinel_photo_core::session::{DocumentPhotoSession, LiveViewState};
use sentinel_photo_core::view::DocumentView;
use sentinel_photo_core::{base64, pipeline, texts};

/// Shared host↔overlay state written by [`OverlayView`] (the session's view) and
/// drained by [`DocumentPhotoUi::poll`].
#[derive(Default)]
struct OverlayState {
    pane_b64: Option<String>,
    capture_visible: bool,
    capture_enabled: bool,
    save_enabled: bool,
    pending_boxes: Vec<String>,
    pending_toasts: Vec<(String, String)>,
    dirty: bool,
    closed_ok: bool,
}

impl OverlayState {
    fn new() -> Rc<RefCell<Self>> {
        Rc::new(RefCell::new(OverlayState {
            // WinForms defaults: sbCapture hidden but enabled, sbSave disabled.
            capture_visible: false,
            capture_enabled: true,
            save_enabled: false,
            dirty: true, // push an initial snapshot
            ..Default::default()
        }))
    }
}

/// The session's view: records window operations into the shared [`OverlayState`].
struct OverlayView(Rc<RefCell<OverlayState>>);

impl DocumentView for OverlayView {
    fn show_error_message(&mut self, text: &str) {
        let mut s = self.0.borrow_mut();
        s.pending_boxes.push(text.to_string());
        s.dirty = true;
    }
    fn show_toast(&mut self, caption: &str, text: &str) {
        let mut s = self.0.borrow_mut();
        s.pending_toasts.push((caption.to_string(), text.to_string()));
        s.dirty = true;
    }
    fn set_capture_visible(&mut self, visible: bool) {
        let mut s = self.0.borrow_mut();
        s.capture_visible = visible;
        s.dirty = true;
    }
    fn set_capture_enabled(&mut self, enabled: bool) {
        let mut s = self.0.borrow_mut();
        s.capture_enabled = enabled;
        s.dirty = true;
    }
    fn set_save_enabled(&mut self, enabled: bool) {
        let mut s = self.0.borrow_mut();
        s.save_enabled = enabled;
        s.dirty = true;
    }
    fn show_captured_image(&mut self, image: Option<&pipeline::Rgb>) {
        let encoded = image.and_then(|rgb| pipeline::to_jpeg(rgb).ok()).map(|j| base64::encode(&j));
        let mut s = self.0.borrow_mut();
        s.pane_b64 = encoded;
        s.dirty = true;
    }
    fn close_ok(&mut self) {
        self.0.borrow_mut().closed_ok = true;
    }
}

/// The document-photo overlay operation.
pub struct DocumentPhotoUi {
    session: DocumentPhotoSession,
    state: Rc<RefCell<OverlayState>>,
    /// Cleared on drop so a second `TakeDocumentPhoto` is no longer "busy".
    _busy: BusyGuard,
    cancelled: bool,
    finished: bool,
}

impl DocumentPhotoUi {
    /// Build the overlay for a camera + validated scale/currentPhoto, and run the
    /// session's `Start` (so the no-camera / no-SDK boxes are queued before the
    /// first frame, as the reference shows them before the window).
    pub fn new(
        camera: Box<dyn Camera>,
        scale_percent: i32,
        current_photo: Option<String>,
        busy: BusyGuard,
    ) -> Self {
        let state = OverlayState::new();
        let view = Box::new(OverlayView(state.clone()));
        let mut session =
            DocumentPhotoSession::new(view, camera, scale_percent, current_photo, Box::new(|_| {}));
        session.start();
        DocumentPhotoUi { session, state, _busy: busy, cancelled: false, finished: false }
    }

    /// The current update snapshot, draining the pending boxes/toasts.
    fn snapshot(&mut self, live_frame_b64: Option<String>) -> String {
        let live = match self.session.live_view_state() {
            LiveViewState::On => "on",
            LiveViewState::Off => "off",
            LiveViewState::NoSession => "nosession",
        };
        let mut s = self.state.borrow_mut();
        let boxes = std::mem::take(&mut s.pending_boxes);
        let toasts = std::mem::take(&mut s.pending_toasts);
        let value = serde_json::json!({
            "live": live,
            "liveFrame": live_frame_b64,
            "pane": s.pane_b64,
            "captureVisible": s.capture_visible,
            "captureEnabled": s.capture_enabled,
            "saveEnabled": s.save_enabled,
            "boxes": boxes,
            "toasts": toasts.iter().map(|(c, t)| serde_json::json!({"caption": c, "text": t})).collect::<Vec<_>>(),
            // The UI labels, sourced from the pinned texts so there is no drift.
            "labels": {
                "liveNull": texts::LIVE_NULL_TEXT,
                "captNull": texts::CAPTURED_NULL_TEXT,
                "label": texts::CAPTION_LINE,
                "capture": texts::CAPTURE,
                "save": texts::SAVE,
            },
        });
        s.dirty = false;
        value.to_string()
    }
}

impl PluginUi for DocumentPhotoUi {
    fn html(&self) -> String {
        OVERLAY_HTML.to_string()
    }

    fn on_message(&mut self, message: &str) {
        match message {
            "capture" => self.session.capture(),
            "save" => self.session.save(),
            "reload" => self.session.init_camera(),
            "cancel" => self.cancelled = true,
            _ => {}
        }
    }

    fn poll(&mut self) -> UiStep {
        if self.finished {
            return UiStep::Pending;
        }

        // Completion: Save resolved (close_ok) → the saved base64; cancel → empty.
        if self.state.borrow().closed_ok {
            self.finished = true;
            let result = self.session.result_base64().map(|s| s.to_string()).unwrap_or_default();
            self.session.close();
            return UiStep::Done(PluginResult::ok(result));
        }
        if self.cancelled {
            self.finished = true;
            self.session.close();
            return UiStep::Done(PluginResult::cancel());
        }

        // Camera events (download / download-failed / shutdown).
        self.session.pump();

        // One live-view frame, if the camera has one ready.
        let mut live_frame_b64 = None;
        match self.session.next_live_view_frame() {
            Ok(Some(frame)) => live_frame_b64 = Some(base64::encode(&frame)),
            Ok(None) => {}
            Err(e) => self.session.live_view_failed(&[e.message()]),
        }

        if self.state.borrow().dirty || live_frame_b64.is_some() {
            UiStep::Update(self.snapshot(live_frame_b64))
        } else {
            UiStep::Pending
        }
    }

    fn on_close(&mut self) -> PluginResult {
        // Esc / the tab navigating away: close the camera and cancel.
        if !self.finished {
            self.finished = true;
            self.session.close();
        }
        PluginResult::cancel()
    }
}

/// A token that marks the plugin "busy" while an overlay is open, cleared on drop
/// (so an Esc or a tab navigation can never lock Photo until restart).
pub struct BusyGuard(Weak<std::cell::Cell<bool>>);

impl BusyGuard {
    /// Take the busy flag (sets it true). The caller must have checked it was free.
    pub fn acquire(flag: &Rc<std::cell::Cell<bool>>) -> Self {
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

/// The overlay document. Static — all data arrives through `window.ui.onUpdate`
/// (rendered with `textContent` / `img.src`), so a page cannot inject markup.
const OVERLAY_HTML: &str = r##"<!DOCTYPE html>
<html><head><meta charset="utf-8"><style>
  html,body{margin:0;height:100%;font-family:"Segoe UI",Arial,sans-serif;background:#0d1117;color:#e6edf3}
  #wrap{position:fixed;inset:0;display:flex;gap:14px;align-items:center;justify-content:center;padding:18px;box-sizing:border-box}
  .pane{background:#161b22;border:1px solid #30363d;border-radius:8px;display:flex;align-items:center;justify-content:center;overflow:hidden;position:relative}
  #liveWrap{width:487px;height:587px}
  #captWrap{width:416px;height:503px}
  #live{max-width:100%;max-height:100%;transform:scaleX(-1)}
  #pane{max-width:100%;max-height:100%}
  .null{color:#7d8590;font-size:18px;text-align:center;padding:0 12px}
  #guides{position:absolute;inset:0;pointer-events:none}
  #col2{display:flex;flex-direction:column;gap:10px;align-items:stretch}
  .row{display:flex;gap:10px;justify-content:flex-end}
  button{height:38px;padding:0 16px;border-radius:6px;border:1px solid #444c56;background:#2d333b;color:#cdd9e5;font-size:13px;cursor:pointer}
  button:hover:not(:disabled){background:#373e47}
  button:disabled{opacity:.45;cursor:default}
  #capture{background:#d9772b;border-color:#d9772b;color:#fff}
  #label{color:#7d8590;font-size:12px;text-align:center}
  #reload{position:absolute;top:8px;right:8px;width:22px;height:22px;padding:0;border-radius:4px}
  #box{position:fixed;inset:0;display:none;align-items:center;justify-content:center;background:#0008}
  #boxCard{background:#20242b;border:1px solid #444c56;border-radius:10px;padding:18px 20px;max-width:420px}
  #boxText{margin:0 0 14px;white-space:pre-wrap}
  #toast{position:fixed;right:16px;bottom:16px;display:flex;flex-direction:column;gap:8px}
  .t{background:#20242b;border:1px solid #444c56;border-radius:6px;padding:10px 12px;font-size:13px;box-shadow:0 6px 20px #0008}
</style></head><body>
  <div id="wrap">
    <div class="pane" id="liveWrap">
      <div class="null" id="liveNull"></div>
      <img id="live" alt="" style="display:none">
      <svg id="guides" viewBox="0 0 487 587" preserveAspectRatio="none" style="display:none">
        <rect x="80" y="330" width="300" height="215" fill="none" stroke="#008000" stroke-width="1"/>
        <ellipse cx="230" cy="225" rx="90" ry="125" fill="none" stroke="#008000" stroke-width="1"/>
      </svg>
      <button id="reload" title="reload" onclick="S('reload')"></button>
    </div>
    <div id="col2">
      <div class="pane" id="captWrap">
        <div class="null" id="captNull"></div>
        <img id="pane" alt="" style="display:none">
      </div>
      <div id="label"></div>
      <div class="row">
        <button id="capture" onclick="S('capture')" style="display:none"></button>
        <button id="save" onclick="S('save')" disabled></button>
        <button id="exit" onclick="S('cancel')">&#10005;</button>
      </div>
    </div>
  </div>
  <div id="box"><div id="boxCard"><p id="boxText"></p><div class="row"><button onclick="hideBox()">OK</button></div></div></div>
  <div id="toast"></div>
<script>
  var labelsSet = false;
  function applyLabels(L){
    if (!L || labelsSet) return; labelsSet = true;
    document.getElementById('liveNull').textContent = L.liveNull;
    document.getElementById('captNull').textContent = L.captNull;
    document.getElementById('label').textContent = L.label;
    document.getElementById('capture').textContent = L.capture;
    document.getElementById('save').textContent = L.save;
  }
  function S(m){ window.ui && window.ui.send(m); }
  function hideBox(){ document.getElementById('box').style.display='none'; }
  function showBox(t){ document.getElementById('boxText').textContent=t; document.getElementById('box').style.display='flex'; }
  function toast(cap,txt){
    var d=document.createElement('div'); d.className='t'; d.textContent=cap;
    document.getElementById('toast').appendChild(d); setTimeout(function(){ d.remove(); }, 2000);
  }
  window.ui = window.ui || {};
  window.ui.onUpdate = function(j){
    var s; try { s = JSON.parse(j); } catch(e){ return; }
    applyLabels(s.labels);
    var live=document.getElementById('live'), liveNull=document.getElementById('liveNull'), guides=document.getElementById('guides');
    if (s.liveFrame){ live.src='data:image/jpeg;base64,'+s.liveFrame; }
    var on = s.live==='on';
    live.style.display = (on && live.getAttribute('src')) ? 'block':'none';
    guides.style.display = on ? 'block':'none';
    liveNull.style.display = (s.live==='nosession') ? 'block':'none';
    var pane=document.getElementById('pane'), captNull=document.getElementById('captNull');
    if (s.pane){ pane.src='data:image/jpeg;base64,'+s.pane; pane.style.display='block'; captNull.style.display='none'; }
    else { pane.style.display='none'; captNull.style.display='block'; }
    document.getElementById('capture').style.display = s.captureVisible ? 'inline-block':'none';
    document.getElementById('capture').disabled = !s.captureEnabled;
    document.getElementById('save').disabled = !s.saveEnabled;
    (s.boxes||[]).forEach(showBox);
    (s.toasts||[]).forEach(function(t){ toast(t.caption, t.text); });
  };
  document.addEventListener('keydown', function(e){
    if (e.key==='Enter'){ S('save'); } else if (e.key==='Escape'){ S('cancel'); }
  });
  document.getElementById('liveWrap').addEventListener('click', function(e){
    if (e.target.id!=='reload') S('capture');
  });
</script></body></html>
"##;
