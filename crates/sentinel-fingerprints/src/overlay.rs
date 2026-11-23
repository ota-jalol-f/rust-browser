//! The enrollment overlay: a [`PluginUi`] that owns an [`EnrollmentSession`] and
//! renders `FormFingerprintsV2`'s window in the shell's full-window overlay.
//!
//! The overlay is a functional port of the window — status line, the two tabs
//! (enroll / settings), take-auto / stop, the ten finger tiles, per-finger radial
//! actions (damaged / absent / normal / single-enroll), the group clear/take
//! buttons, and the settings page (device fields, init/exit/calibrate, sensitivity)
//! — driven entirely by the session. The decorative SVG hands and the radial
//! popup's exact geometry are left for a cosmetic follow-up; every action is wired.
//!
//! Host → overlay state is pushed with `window.ui.onUpdate`; overlay → host actions
//! arrive through `window.ui.send`. The group-clear confirmation is shown by the
//! overlay itself (JS `confirm`), so the session's `show_question` always returns
//! `true` — it is only called after the user confirmed.

use std::cell::RefCell;
use std::rc::Rc;

use sentinel_fingerprints_core::finger::FingerPosition;
use sentinel_fingerprints_core::sdk::RealScanDevice;
use sentinel_fingerprints_core::session::{EnrollmentSession, FingerGroup};
use sentinel_fingerprints_core::view::{AlertKind, EnrollView};
use sentinel_fingerprints_core::texts;
use sentinel_plugin::{PluginResult, PluginUi, UiStep};

/// Transient view effects (the main state is read from the session directly).
#[derive(Default)]
struct OverlayState {
    pending_captures: i32,
    boxes: Vec<(String, String)>,
    toasts: Vec<(String, String)>,
    dirty: bool,
}

/// The session's view: records the window effects into the shared [`OverlayState`].
struct OverlayView(Rc<RefCell<OverlayState>>);

fn kind_str(k: AlertKind) -> &'static str {
    match k {
        AlertKind::Info => texts::CAPTION_INFO,
        AlertKind::Warning => texts::CAPTION_WARNING,
        AlertKind::Error => texts::CAPTION_ERROR,
        AlertKind::Question => texts::CAPTION_QUESTION,
    }
}

impl EnrollView for OverlayView {
    fn show_message(&mut self, message: &str, kind: AlertKind) {
        let mut s = self.0.borrow_mut();
        s.boxes.push((kind_str(kind).to_string(), message.to_string()));
        s.dirty = true;
    }
    fn show_alert(&mut self, caption: &str, text: &str, _kind: AlertKind) {
        let mut s = self.0.borrow_mut();
        s.toasts.push((caption.to_string(), text.to_string()));
        s.dirty = true;
    }
    fn show_question(&mut self, _message: &str) -> bool {
        // The overlay confirmed before sending the clear action.
        true
    }
    fn show_plain_message_box(&mut self, text: &str) {
        let mut s = self.0.borrow_mut();
        s.boxes.push((String::new(), text.to_string()));
        s.dirty = true;
    }
    fn start_capture(&mut self) {
        let mut s = self.0.borrow_mut();
        s.pending_captures += 1;
        s.dirty = true;
    }
    fn preview_handle(&self) -> usize {
        0
    }
    fn preview_client_size(&self) -> (i32, i32) {
        (452, 351)
    }
}

/// The enrollment overlay operation.
pub struct EnrollUi {
    session: EnrollmentSession,
    state: Rc<RefCell<OverlayState>>,
    outcome: Option<PluginResult>,
    finished: bool,
    labels_sent: bool,
}

impl EnrollUi {
    /// Open the overlay (runs `FormFingerprintsV2_Load`). A failed export preflight
    /// resolves the call at once with the error envelope, as C# did.
    pub fn new(device: Box<dyn RealScanDevice>) -> Self {
        let state = Rc::new(RefCell::new(OverlayState { dirty: true, ..Default::default() }));
        let view = Box::new(OverlayView(state.clone()));
        let mut session = EnrollmentSession::new(device, view);
        let outcome = match session.load() {
            Ok(()) => None,
            Err(e) => Some(PluginResult::fail(e.message())),
        };
        EnrollUi { session, state, outcome, finished: false, labels_sent: false }
    }

    fn drain_captures(&mut self) {
        loop {
            let has = self.state.borrow().pending_captures > 0;
            if !has {
                break;
            }
            self.state.borrow_mut().pending_captures -= 1;
            // NOTE: the stub returns at once. The real RS_SDK `take_image` blocks up
            // to 10 s, so the device build must run the worker off the UI thread and
            // feed `capture_process` on completion (the FFI increment).
            let outcome = self.session.do_auto_capture();
            self.session.capture_process(outcome);
            self.state.borrow_mut().dirty = true;
        }
    }

    fn snapshot(&mut self) -> String {
        let s = &self.session;
        let fingers: Vec<_> = s
            .fingers()
            .iter()
            .map(|f| {
                serde_json::json!({
                    "status": f.status as i32,
                    "quality": f.quality,
                    "tileRating": f.tile_rating,
                    "hasImage": f.tile_image.is_some(),
                    "tileBack": format!("{:?}", f.tile_back),
                })
            })
            .collect();
        let (boxes, toasts) = {
            let mut st = self.state.borrow_mut();
            st.dirty = false;
            (std::mem::take(&mut st.boxes), std::mem::take(&mut st.toasts))
        };
        let labels = if self.labels_sent {
            serde_json::Value::Null
        } else {
            self.labels_sent = true;
            labels_json()
        };
        serde_json::json!({
            "labels": labels,
            "status": s.status_text(),
            "takeAutoEnabled": s.take_auto_enabled(),
            "groupTakeEnabled": s.group_take_enabled(),
            "saveEnabled": s.save_enabled(),
            "saveToFileVisible": s.save_to_file_visible(),
            "initSdkEnabled": s.init_sdk_enabled(),
            "initDeviceEnabled": s.init_device_enabled(),
            "exitDeviceEnabled": s.exit_device_enabled(),
            "deviceList": s.device_list(),
            "selectedDeviceIndex": s.selected_device_index(),
            "deviceInfo": s.device_info(),
            "firmwareInfo": s.firmware_info(),
            "hardwareInfo": s.hardware_info(),
            "deviceId": s.device_id(),
            "sdkInfo": s.sdk_info(),
            "sensitivity": s.sensitivity(),
            "selectedPosition": s.selected_position().value(),
            "currentPosition": s.current_position().value(),
            "missing": s.missing_positions().iter().map(|p| p.value()).collect::<Vec<_>>(),
            "damaged": s.damaged_positions().iter().map(|p| p.value()).collect::<Vec<_>>(),
            "isRolled": s.is_rolled(),
            "fingers": fingers,
            "boxes": boxes.iter().map(|(c, t)| serde_json::json!({"caption": c, "text": t})).collect::<Vec<_>>(),
            "toasts": toasts.iter().map(|(c, t)| serde_json::json!({"caption": c, "text": t})).collect::<Vec<_>>(),
        })
        .to_string()
    }
}

impl PluginUi for EnrollUi {
    fn html(&self) -> String {
        OVERLAY_HTML.to_string()
    }

    fn on_message(&mut self, message: &str) {
        match message {
            "takeAuto" => self.session.take_auto_click(),
            "stop" => self.session.stop_click(),
            "save" => {
                if let Some(json) = self.session.save_click() {
                    self.outcome = Some(PluginResult::ok(json));
                }
            }
            "cancel" => {
                self.session.closing();
                self.outcome = Some(PluginResult::cancel());
            }
            "calibrate" => {
                if self.session.calibrate_click() {
                    self.session.closing();
                    self.outcome = Some(PluginResult::cancel());
                }
            }
            "initSdk" => self.session.init_sdk_click(),
            "initDevice" => self.session.init_device_click(),
            "exitDevice" => self.session.exit_device_click(),
            "tab" => self.session.tab_changed(),
            "damaged" => self.session.menu_damaged(),
            "absent" => self.session.menu_absent(),
            "normal" => self.session.menu_normal(),
            "enroll" => self.session.menu_enroll(),
            other => {
                if let Some(n) = other.strip_prefix("sensitivity:") {
                    if let Ok(v) = n.parse::<i32>() {
                        self.session.sensitivity_changed(v);
                    }
                } else if let Some(p) = other.strip_prefix("finger:") {
                    if let Some(pos) = p.parse::<i32>().ok().and_then(FingerPosition::from_value) {
                        self.session.finger_click(pos);
                    }
                } else if let Some(rest) = other.strip_prefix("groupTake:") {
                    if let Some(g) = parse_group(rest) {
                        self.session.group_button_click(g, 1);
                    }
                } else if let Some(rest) = other.strip_prefix("groupClear:") {
                    if let Some(g) = parse_group(rest) {
                        self.session.group_button_click(g, 0);
                    }
                }
            }
        }
        self.state.borrow_mut().dirty = true;
    }

    fn poll(&mut self) -> UiStep {
        if self.finished {
            return UiStep::Pending;
        }
        if let Some(result) = self.outcome.take() {
            self.finished = true;
            return UiStep::Done(result);
        }
        self.drain_captures();
        if let Some(result) = self.outcome.take() {
            self.finished = true;
            return UiStep::Done(result);
        }
        let dirty = self.state.borrow().dirty || !self.labels_sent;
        if dirty {
            UiStep::Update(self.snapshot())
        } else {
            UiStep::Pending
        }
    }

    fn on_close(&mut self) -> PluginResult {
        if !self.finished {
            self.finished = true;
            self.session.closing();
        }
        PluginResult::cancel()
    }
}

fn parse_group(s: &str) -> Option<FingerGroup> {
    match s {
        "right" => Some(FingerGroup::RightHand),
        "left" => Some(FingerGroup::LeftHand),
        "thumbs" => Some(FingerGroup::Thumbs),
        _ => None,
    }
}

fn labels_json() -> serde_json::Value {
    serde_json::json!({
        "title": texts::WINDOW_TITLE,
        "tabEnroll": texts::TAB_ENROLL,
        "tabSettings": texts::TAB_SETTINGS,
        "takeAuto": texts::TAKE_AUTO,
        "stop": texts::STOP,
        "save": texts::SAVE,
        "calibrate": texts::CALIBRATE,
        "initSdk": texts::INIT_SDK,
        "initDevice": texts::INIT_DEVICE,
        "exitDevice": texts::EXIT_DEVICE,
        "connectedScanner": texts::CONNECTED_SCANNER,
        "deviceInfoGroup": texts::DEVICE_INFO_GROUP,
        "infoModel": texts::INFO_MODEL,
        "infoFirmware": texts::INFO_FIRMWARE,
        "infoHardware": texts::INFO_HARDWARE,
        "infoSdkVersion": texts::INFO_SDK_VERSION,
        "infoDeviceId": texts::INFO_DEVICE_ID,
        "sensitivity": texts::SENSITIVITY,
        "sensTicks": [texts::SENSITIVITY_NORMAL, texts::SENSITIVITY_HIGH, texts::SENSITIVITY_HIGHER, texts::SENSITIVITY_DISABLED],
        "legendMissing": texts::LEGEND_MISSING,
        "legendDamaged": texts::LEGEND_DAMAGED,
        "legendScanning": texts::LEGEND_SCANNING,
        "groupRight": texts::GROUP_RIGHT_HAND,
        "groupLeft": texts::GROUP_LEFT_HAND,
        "groupThumbs": texts::GROUP_THUMBS,
        "headerClear": texts::HEADER_CLEAR,
        "headerTake": texts::HEADER_TAKE,
        "menuEnroll": texts::MENU_ENROLL_TIP_TITLE,
        "menuDamaged": texts::MENU_DAMAGED_TIP_TITLE,
        "menuAbsent": texts::MENU_ABSENT_TIP_TITLE,
        "menuNormal": texts::SAVE,
        "clearRight": texts::CLEAR_RIGHT_QUESTION,
        "clearLeft": texts::CLEAR_LEFT_QUESTION,
        "clearThumbs": texts::CLEAR_THUMBS_QUESTION,
        "fingerNames": texts::FINGER_POSITION_NAMES,
    })
}

/// The overlay document. All text/state arrives through `window.ui.onUpdate`.
const OVERLAY_HTML: &str = r##"<!DOCTYPE html>
<html><head><meta charset="utf-8"><style>
  html,body{margin:0;height:100%;font-family:"Segoe UI",Arial,sans-serif;background:#0d1117;color:#e6edf3;font-size:13px}
  #wrap{position:fixed;inset:0;display:flex;flex-direction:column;padding:14px;box-sizing:border-box;gap:10px}
  #title{font-size:15px;font-weight:600}
  #tabs{display:flex;gap:4px;border-bottom:1px solid #30363d}
  .tab{padding:7px 16px;cursor:pointer;color:#7d8590;border-bottom:2px solid transparent}
  .tab.on{color:#e6edf3;border-bottom-color:#d9772b}
  .page{display:none;flex:1;overflow:auto}
  .page.on{display:block}
  #status{font-weight:700;text-align:center;padding:6px;min-height:18px}
  .row{display:flex;gap:8px;align-items:center;flex-wrap:wrap}
  button{height:32px;padding:0 14px;border-radius:6px;border:1px solid #444c56;background:#2d333b;color:#cdd9e5;font-size:13px;cursor:pointer}
  button:hover:not(:disabled){background:#373e47}
  button:disabled{opacity:.45;cursor:default}
  .accent{background:#d9772b;border-color:#d9772b;color:#fff}
  #grid{display:grid;grid-template-columns:repeat(5,1fr);gap:6px;margin-top:10px}
  .tile{border:1px solid #30363d;border-radius:6px;padding:6px;text-align:center;cursor:pointer}
  .tile.sel{outline:2px solid #d9772b}
  .tile .nm{font-size:11px;color:#7d8590}
  .tile .box{height:46px;border-radius:4px;background:#161b22;margin:4px 0;display:flex;align-items:center;justify-content:center;color:#7d8590;font-size:11px}
  .tile .q{color:#d9a441;letter-spacing:1px}
  .bk-Orange{background:#c15a0033}.bk-Red{background:#c0303033}.bk-White{background:#ffffff14}
  #actions{margin-top:8px;display:none}
  #settings label{display:inline-block;width:150px;color:#7d8590;text-align:right;margin-right:8px}
  #settings .fld{margin:6px 0}
  input[type=text]{height:28px;background:#161b22;border:1px solid #30363d;border-radius:4px;color:#e6edf3;width:240px;text-transform:uppercase}
  #foot{display:flex;gap:8px;justify-content:flex-end;border-top:1px solid #30363d;padding-top:8px}
  #box{position:fixed;inset:0;display:none;align-items:center;justify-content:center;background:#0008}
  #boxCard{background:#20242b;border:1px solid #444c56;border-radius:10px;padding:18px;max-width:460px}
  #boxText{white-space:pre-wrap;margin:0 0 12px}
  #toast{position:fixed;right:16px;bottom:16px;display:flex;flex-direction:column;gap:8px}
  .t{background:#20242b;border:1px solid #444c56;border-radius:6px;padding:8px 12px;box-shadow:0 6px 20px #0008}
  .leg{display:flex;gap:6px;align-items:center;margin:3px 0}.sw{width:16px;height:16px;border-radius:3px}
</style></head><body>
  <div id="wrap">
    <div id="title"></div>
    <div id="tabs"><div class="tab on" data-p="enroll" id="tabEnroll"></div><div class="tab" data-p="settings" id="tabSettings"></div></div>
    <div id="status"></div>
    <div class="page on" id="pageEnroll">
      <div class="row">
        <button id="takeAuto" class="accent" onclick="S('takeAuto')"></button>
        <button id="stop" onclick="S('stop')"></button>
      </div>
      <div class="row" style="margin-top:8px">
        <b id="lblRight"></b> <button onclick="S('groupTake:right')" id="htR"></button> <button onclick="clr('right')" id="hcR"></button>
        <b id="lblThumbs"></b> <button onclick="S('groupTake:thumbs')" id="htT"></button> <button onclick="clr('thumbs')" id="hcT"></button>
        <b id="lblLeft"></b> <button onclick="S('groupTake:left')" id="htL"></button> <button onclick="clr('left')" id="hcL"></button>
      </div>
      <div id="grid"></div>
      <div id="actions" class="row">
        <span id="selName" style="color:#7d8590"></span>
        <button onclick="S('enroll')" id="aEnroll"></button>
        <button onclick="S('damaged')" id="aDamaged"></button>
        <button onclick="S('absent')" id="aAbsent"></button>
        <button onclick="S('normal')" id="aNormal"></button>
      </div>
    </div>
    <div class="page" id="pageSettings">
      <div id="settings" style="margin-top:8px">
        <div class="fld"><label id="lInfoModel"></label><input type="text" id="fModel" readonly></div>
        <div class="fld"><label id="lInfoFirmware"></label><input type="text" id="fFirmware" readonly></div>
        <div class="fld"><label id="lInfoHardware"></label><input type="text" id="fHardware" readonly></div>
        <div class="fld"><label id="lInfoSdk"></label><input type="text" id="fSdk" readonly></div>
        <div class="fld"><label id="lInfoId"></label><input type="text" id="fId" readonly></div>
        <div class="row" style="margin:10px 0">
          <button onclick="S('initSdk')" id="bInitSdk"></button>
          <button onclick="S('initDevice')" id="bInitDevice"></button>
          <button onclick="S('exitDevice')" id="bExitDevice"></button>
          <button onclick="S('calibrate')" id="bCalibrate"></button>
        </div>
        <div class="fld"><label id="lSens"></label>
          <input type="range" min="0" max="3" step="1" id="sens" oninput="S('sensitivity:'+this.value)"></div>
        <div id="legend" style="margin-top:10px"></div>
      </div>
    </div>
    <div id="foot">
      <button id="save" class="accent" onclick="S('save')" disabled></button>
      <button id="exit" onclick="S('cancel')">&#10005;</button>
    </div>
  </div>
  <div id="box"><div id="boxCard"><p id="boxText"></p><div class="row" style="justify-content:flex-end"><button onclick="hideBox()">OK</button></div></div></div>
  <div id="toast"></div>
<script>
  var L=null, cur=0, clearQ={};
  function S(m){ window.ui && window.ui.send(m); }
  function clr(g){ if(confirm(clearQ[g]||'?')) S('groupClear:'+g); }
  function hideBox(){ var b=document.getElementById('box'); b._q=(b._q||[]); if(b._q.length){ document.getElementById('boxText').textContent=b._q.shift(); } else { b.style.display='none'; } }
  function showBox(t){ var b=document.getElementById('box'); if(b.style.display==='flex'){ (b._q=b._q||[]).push(t); return;} document.getElementById('boxText').textContent=t; b.style.display='flex'; }
  function toast(c,t){ var d=document.createElement('div'); d.className='t'; d.textContent=c+(t?(' — '+t):''); document.getElementById('toast').appendChild(d); setTimeout(function(){d.remove();},2000); }
  document.querySelectorAll('.tab').forEach(function(el){ el.onclick=function(){
    document.querySelectorAll('.tab').forEach(function(t){t.classList.remove('on');}); el.classList.add('on');
    document.getElementById('pageEnroll').classList.toggle('on', el.dataset.p==='enroll');
    document.getElementById('pageSettings').classList.toggle('on', el.dataset.p==='settings');
    S('tab');
  };});
  function applyLabels(l){
    if(!l) return; L=l;
    var set=function(id,v){ var e=document.getElementById(id); if(e) e.textContent=v; };
    document.title=l.title; set('title',l.title); set('tabEnroll',l.tabEnroll); set('tabSettings',l.tabSettings);
    set('takeAuto',l.takeAuto); set('stop',l.stop); set('save',l.save);
    set('lblRight',l.groupRight); set('lblLeft',l.groupLeft); set('lblThumbs',l.groupThumbs);
    ['htR','htT','htL'].forEach(function(i){set(i,l.headerTake);}); ['hcR','hcT','hcL'].forEach(function(i){set(i,l.headerClear);});
    set('aEnroll',l.menuEnroll); set('aDamaged',l.menuDamaged); set('aAbsent',l.menuAbsent); set('aNormal',l.menuNormal);
    set('lInfoModel',l.infoModel); set('lInfoFirmware',l.infoFirmware); set('lInfoHardware',l.infoHardware);
    set('lInfoSdk',l.infoSdkVersion); set('lInfoId',l.infoDeviceId); set('lSens',l.sensitivity);
    set('bInitSdk',l.initSdk); set('bInitDevice',l.initDevice); set('bExitDevice',l.exitDevice); set('bCalibrate',l.calibrate);
    clearQ={right:l.clearRight,left:l.clearLeft,thumbs:l.clearThumbs};
    var lg=document.getElementById('legend');
    lg.innerHTML='';
    [['#c03030',l.legendMissing],['#c15a00',l.legendDamaged],['#2a8a2a',l.legendScanning]].forEach(function(p){
      var d=document.createElement('div'); d.className='leg'; d.innerHTML='<span class="sw" style="background:'+p[0]+'"></span>'; var s=document.createElement('span'); s.textContent=p[1]; d.appendChild(s); lg.appendChild(d);
    });
  }
  function stars(n){ n=Math.max(0,Math.min(5,n)); return '★'.repeat(n)+'☆'.repeat(5-n); }
  function render(s){
    document.getElementById('status').textContent=s.status||'';
    document.getElementById('takeAuto').disabled=!s.takeAutoEnabled;
    ['htR','htT','htL'].forEach(function(i){document.getElementById(i).disabled=!s.groupTakeEnabled;});
    document.getElementById('save').disabled=!s.saveEnabled;
    document.getElementById('bInitSdk').disabled=!s.initSdkEnabled;
    document.getElementById('bInitDevice').disabled=!s.initDeviceEnabled;
    document.getElementById('bExitDevice').disabled=!s.exitDeviceEnabled;
    document.getElementById('fModel').value=s.deviceInfo||''; document.getElementById('fFirmware').value=s.firmwareInfo||'';
    document.getElementById('fHardware').value=s.hardwareInfo||''; document.getElementById('fSdk').value=s.sdkInfo||'';
    document.getElementById('fId').value=s.deviceId||''; document.getElementById('sens').value=s.sensitivity;
    cur=s.currentPosition||0;
    var names=(L&&L.fingerNames)||[];
    var g=document.getElementById('grid'); g.innerHTML='';
    // Tiles in finger order 1..10.
    for(var n=1;n<=10;n++){ (function(n){
      var f=s.fingers[n-1]||{};
      var t=document.createElement('div'); t.className='tile bk-'+(f.tileBack||'Default')+(cur===n?' sel':'');
      t.onclick=function(){ S('finger:'+n); };
      var nm=document.createElement('div'); nm.className='nm'; nm.textContent=(names[n]||('#'+n));
      var box=document.createElement('div'); box.className='box'; box.textContent=f.hasImage?'●':'';
      var q=document.createElement('div'); q.className='q'; q.textContent=stars(f.tileRating||0);
      t.appendChild(nm); t.appendChild(box); t.appendChild(q); g.appendChild(t);
    })(n); }
    var act=document.getElementById('actions');
    act.style.display = (cur>=1&&cur<=10)?'flex':'none';
    if(cur>=1&&cur<=10){ document.getElementById('selName').textContent=(names[cur]||('#'+cur)); }
    (s.boxes||[]).forEach(function(b){ showBox(b.text); });
    (s.toasts||[]).forEach(function(t){ toast(t.caption,t.text); });
  }
  window.ui=window.ui||{};
  window.ui.onUpdate=function(j){ var s; try{s=JSON.parse(j);}catch(e){return;} applyLabels(s.labels); render(s); };
</script></body></html>
"##;
