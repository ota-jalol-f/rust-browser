//! `DocumentPhotoSession` driven by a fake camera and a recording view — the port
//! of the reference `DocumentPhotoSessionTests` (FormCaptureDocument's logic with
//! no window and no camera).

use std::cell::RefCell;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::rc::Rc;

use sentinel_photo_core::camera::{Camera, CameraError, CameraEvent};
use sentinel_photo_core::pipeline::Rgb;
use sentinel_photo_core::session::{DocumentPhotoSession, LiveViewState};
use sentinel_photo_core::texts;
use sentinel_photo_core::view::DocumentView;
use sentinel_photo_core::{base64, pipeline};

// ---- fixtures -------------------------------------------------------------

fn fixture_bytes(name: &str) -> Vec<u8> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name);
    std::fs::read(&p).unwrap_or_else(|e| panic!("read {name}: {e}"))
}

/// A tiny solid JPEG (`Fixture.TinyJpeg`), decodable to `w x h`.
fn tiny_jpeg(w: u16, h: u16) -> Vec<u8> {
    let data = vec![100u8; w as usize * h as usize * 3];
    let mut out = Vec::new();
    jpeg_encoder::Encoder::new(&mut out, 90)
        .encode(&data, w, h, jpeg_encoder::ColorType::Rgb)
        .unwrap();
    out
}

fn base64_jpeg(w: u16, h: u16) -> String {
    base64::encode(&tiny_jpeg(w, h))
}

/// The CR2 header from the reference fake: not decodable by the JPEG decoder.
const RAW_FILE: [u8; 16] =
    [0x49, 0x49, 0x2A, 0x00, 0x10, 0x00, 0x00, 0x00, 0x43, 0x52, 0x02, 0x00, 0, 0, 0, 0];

// ---- fake camera ----------------------------------------------------------

#[derive(Default)]
struct CamState {
    calls: Vec<String>,
    events: VecDeque<CameraEvent>,
    session_open: bool,
    live_on: bool,
    initialized: bool,
    dispose_count: u32,
    no_camera: bool,
    throw_on_initialize: Option<CameraError>,
    throw_on_open_session: Option<CameraError>,
    throw_on_take_photo: Option<CameraError>,
    throw_on_close_session: Option<CameraError>,
    throw_on_dispose: Option<CameraError>,
}

/// A handle shared between the test and the session-owned [`TestCamera`].
#[derive(Clone)]
struct CamHandle(Rc<RefCell<CamState>>);

impl CamHandle {
    fn new() -> Self {
        CamHandle(Rc::new(RefCell::new(CamState::default())))
    }
    fn camera(&self) -> Box<dyn Camera> {
        Box::new(TestCamera(self.0.clone()))
    }
    fn calls(&self) -> Vec<String> {
        self.0.borrow().calls.clone()
    }
    fn take_photo_count(&self) -> usize {
        self.0.borrow().calls.iter().filter(|c| *c == "TakePhoto").count()
    }
    fn dispose_count(&self) -> u32 {
        self.0.borrow().dispose_count
    }
    fn raise_download(&self, file: Vec<u8>) {
        self.0.borrow_mut().events.push_back(CameraEvent::PhotoDownloaded(file));
    }
    fn raise_download_failed(&self, message: &str) {
        self.0.borrow_mut().events.push_back(CameraEvent::DownloadFailed(message.to_string()));
    }
    fn raise_shutdown(&self) {
        self.0.borrow_mut().events.push_back(CameraEvent::Shutdown);
    }
}

struct TestCamera(Rc<RefCell<CamState>>);

impl Camera for TestCamera {
    fn initialize(&mut self) -> Result<(), CameraError> {
        let mut s = self.0.borrow_mut();
        s.calls.push("Initialize".into());
        if let Some(e) = s.throw_on_initialize.clone() {
            return Err(e);
        }
        s.initialized = true;
        Ok(())
    }
    fn open_session(&mut self) -> Result<bool, CameraError> {
        let mut s = self.0.borrow_mut();
        s.calls.push("OpenSession".into());
        if let Some(e) = s.throw_on_open_session.clone() {
            return Err(e);
        }
        if !s.initialized {
            return Err(CameraError::Other("EDSDK is not initialized".into()));
        }
        if s.no_camera {
            return Ok(false);
        }
        s.session_open = true;
        Ok(true)
    }
    fn close_session(&mut self) -> Result<(), CameraError> {
        let mut s = self.0.borrow_mut();
        s.calls.push("CloseSession".into());
        if let Some(e) = s.throw_on_close_session.clone() {
            return Err(e);
        }
        s.session_open = false;
        s.live_on = false;
        Ok(())
    }
    fn prepare_host_capture(&mut self) -> Result<(), CameraError> {
        let mut s = self.0.borrow_mut();
        s.calls.push("PrepareHostCapture".into());
        if !s.session_open {
            return Err(CameraError::Other("Фотокамера уланмаган".into()));
        }
        Ok(())
    }
    fn start_live_view(&mut self) -> Result<(), CameraError> {
        let mut s = self.0.borrow_mut();
        s.calls.push("StartLiveView".into());
        if !s.session_open {
            return Err(CameraError::Other("Фотокамера уланмаган".into()));
        }
        s.live_on = true;
        Ok(())
    }
    fn stop_live_view(&mut self) -> Result<(), CameraError> {
        let mut s = self.0.borrow_mut();
        s.calls.push("StopLiveView".into());
        s.live_on = false;
        Ok(())
    }
    fn get_live_view_frame(&mut self) -> Result<Option<Vec<u8>>, CameraError> {
        Ok(None)
    }
    fn take_photo(&mut self) -> Result<(), CameraError> {
        let mut s = self.0.borrow_mut();
        s.calls.push("TakePhoto".into());
        if let Some(e) = s.throw_on_take_photo.clone() {
            return Err(e);
        }
        if !s.session_open {
            return Err(CameraError::Other("Фотокамера уланмаган".into()));
        }
        Ok(())
    }
    fn release_camera(&mut self) -> Result<(), CameraError> {
        let mut s = self.0.borrow_mut();
        s.calls.push("ReleaseCamera".into());
        s.session_open = false;
        s.live_on = false;
        Ok(())
    }
    fn dispose(&mut self) -> Result<(), CameraError> {
        let mut s = self.0.borrow_mut();
        s.calls.push("Dispose".into());
        s.dispose_count += 1;
        if let Some(e) = s.throw_on_dispose.clone() {
            return Err(e);
        }
        s.session_open = false;
        s.live_on = false;
        Ok(())
    }
    fn is_session_open(&self) -> bool {
        self.0.borrow().session_open
    }
    fn is_live_view_on(&self) -> bool {
        self.0.borrow().live_on
    }
    fn poll_event(&mut self) -> Option<CameraEvent> {
        self.0.borrow_mut().events.pop_front()
    }
}

// ---- recording view -------------------------------------------------------

struct ViewRec {
    messages: Vec<String>,
    toasts: Vec<(String, String)>,
    capture_visible: bool,
    capture_enabled: bool,
    save_enabled: bool,
    shown_image_size: Option<(usize, usize)>,
    show_image_count: u32,
    close_ok_count: u32,
}

impl Default for ViewRec {
    fn default() -> Self {
        // WinForms designer defaults: sbCapture.Visible=false, sbSave.Enabled=false,
        // sbCapture.Enabled=true (hidden but enabled).
        ViewRec {
            messages: Vec::new(),
            toasts: Vec::new(),
            capture_visible: false,
            capture_enabled: true,
            save_enabled: false,
            shown_image_size: None,
            show_image_count: 0,
            close_ok_count: 0,
        }
    }
}

#[derive(Clone)]
struct ViewHandle(Rc<RefCell<ViewRec>>);

impl ViewHandle {
    fn new() -> Self {
        ViewHandle(Rc::new(RefCell::new(ViewRec::default())))
    }
    fn view(&self) -> Box<dyn DocumentView> {
        Box::new(RecordingView(self.0.clone()))
    }
    fn messages(&self) -> Vec<String> {
        self.0.borrow().messages.clone()
    }
    fn toasts(&self) -> Vec<(String, String)> {
        self.0.borrow().toasts.clone()
    }
}

struct RecordingView(Rc<RefCell<ViewRec>>);

impl DocumentView for RecordingView {
    fn show_error_message(&mut self, text: &str) {
        self.0.borrow_mut().messages.push(text.to_string());
    }
    fn show_toast(&mut self, caption: &str, text: &str) {
        self.0.borrow_mut().toasts.push((caption.to_string(), text.to_string()));
    }
    fn set_capture_visible(&mut self, visible: bool) {
        self.0.borrow_mut().capture_visible = visible;
    }
    fn set_capture_enabled(&mut self, enabled: bool) {
        self.0.borrow_mut().capture_enabled = enabled;
    }
    fn set_save_enabled(&mut self, enabled: bool) {
        self.0.borrow_mut().save_enabled = enabled;
    }
    fn show_captured_image(&mut self, image: Option<&Rgb>) {
        let mut r = self.0.borrow_mut();
        r.show_image_count += 1;
        r.shown_image_size = image.map(|i| (i.width, i.height));
    }
    fn close_ok(&mut self) {
        self.0.borrow_mut().close_ok_count += 1;
    }
}

// ---- harness --------------------------------------------------------------

struct Fix {
    session: DocumentPhotoSession,
    view: ViewHandle,
    camera: CamHandle,
}

fn create(current_photo: Option<String>, percent: i32, setup: impl FnOnce(&mut CamState)) -> Fix {
    let view = ViewHandle::new();
    let camera = CamHandle::new();
    setup(&mut camera.0.borrow_mut());
    let session = DocumentPhotoSession::new(
        view.view(),
        camera.camera(),
        percent,
        current_photo,
        Box::new(|_| {}),
    );
    Fix { session, view, camera }
}

/// Default happy setup (no flags).
fn plain(_: &mut CamState) {}

// ---- tests ----------------------------------------------------------------

#[test]
fn start_happy_opens_session_and_live_view_shows_capture() {
    let mut f = create(None, 20, plain);
    f.session.start();
    assert_eq!(f.camera.calls().join(","), "Initialize,OpenSession,PrepareHostCapture,StartLiveView");
    assert!(f.view.0.borrow().capture_visible);
    assert!(!f.view.0.borrow().save_enabled);
    assert_eq!(f.view.messages().len(), 0);
    assert_eq!(f.view.toasts().len(), 0);
    assert_eq!(f.session.live_view_state(), LiveViewState::On);
}

#[test]
fn start_no_camera_box_and_toast_capture_hidden() {
    let mut f = create(None, 20, |s| s.no_camera = true);
    f.session.start();
    assert_eq!(f.view.messages(), vec![texts::CAMERA_NOT_FOUND.to_string()]);
    assert_eq!(f.view.toasts(), vec![(texts::CAMERA_NOT_FOUND.to_string(), String::new())]);
    assert!(!f.view.0.borrow().capture_visible);
    assert_eq!(f.session.live_view_state(), LiveViewState::NoSession);
}

#[test]
fn start_dll_missing_canon_files_box_and_toast_no_session() {
    let mut f = create(None, 20, |s| s.throw_on_initialize = Some(CameraError::DllNotFound("EDSDK.dll".into())));
    f.session.start();
    assert_eq!(f.view.messages(), vec![texts::CANON_FILES_NOT_FOUND.to_string()]);
    assert_eq!(f.view.toasts()[0].0, texts::CANON_FILES_NOT_FOUND);
    assert!(!f.camera.calls().contains(&"OpenSession".to_string()));
    assert!(!f.view.0.borrow().capture_visible);
}

#[test]
fn start_other_sdk_failure_files_start_box_no_toast() {
    let mut f = create(None, 20, |s| s.throw_on_initialize = Some(CameraError::Other("x86 EDSDK".into())));
    f.session.start();
    assert_eq!(f.view.messages(), vec![texts::CAMERA_FILES_START_ERROR.to_string()]);
    assert_eq!(f.view.toasts().len(), 0);
}

#[test]
fn start_open_session_throws_camera_start_box_capture_stays_hidden() {
    let mut f = create(None, 20, |s| s.throw_on_open_session = Some(CameraError::Other("EdsOpenSession".into())));
    f.session.start();
    assert_eq!(f.view.messages(), vec![texts::CAMERA_START_ERROR.to_string()]);
    assert_eq!(f.view.toasts().len(), 0);
    assert!(!f.view.0.borrow().capture_visible);
}

#[test]
fn reload_with_session_open_closes_and_fails_then_reopens() {
    let mut f = create(None, 20, plain);
    f.session.start();

    f.session.init_camera();
    assert_eq!(f.view.messages(), vec![texts::CAMERA_START_ERROR.to_string()]);
    assert_eq!(f.session.live_view_state(), LiveViewState::NoSession);
    assert!(f.view.0.borrow().capture_visible); // never hidden on this path

    f.session.init_camera();
    assert_eq!(f.view.messages().len(), 1);
    assert_eq!(f.session.live_view_state(), LiveViewState::On);
}

#[test]
fn reload_after_dll_missing_is_camera_start_error() {
    let mut f = create(None, 20, |s| s.throw_on_initialize = Some(CameraError::DllNotFound("EDSDK.dll".into())));
    f.session.start();
    f.session.init_camera();
    assert_eq!(f.view.messages().last().unwrap(), texts::CAMERA_START_ERROR);
}

#[test]
fn enter_with_current_photo_and_no_shot_returns_same_string() {
    let current = base64_jpeg(16, 12);
    let mut f = create(Some(current.clone()), 20, |s| s.no_camera = true);
    f.session.start();
    assert_eq!(f.view.0.borrow().shown_image_size, Some((16, 12)));
    assert!(!f.view.0.borrow().save_enabled);
    f.session.save(); // Enter bypasses the disabled sbSave
    assert_eq!(f.view.0.borrow().close_ok_count, 1);
    assert_eq!(f.session.result_base64(), Some(current.as_str())); // unchanged, not re-encoded
}

#[test]
fn enter_with_nothing_no_photo_box_and_toast_stays_open() {
    let mut f = create(None, 20, plain);
    f.session.start();
    f.session.save();
    assert_eq!(f.view.0.borrow().close_ok_count, 0);
    assert_eq!(f.view.messages(), vec![texts::NO_PHOTO.to_string()]);
    assert_eq!(f.view.toasts(), vec![(texts::NO_PHOTO.to_string(), String::new())]);
    assert_eq!(f.session.result_base64(), None);
}

#[test]
fn current_photo_that_does_not_decode_is_no_image() {
    let garbage = base64::encode(&[1, 2, 3, 4, 5]);
    let mut f = create(Some(garbage), 20, plain);
    f.session.start();
    f.session.save();
    assert_eq!(f.view.0.borrow().show_image_count, 0);
    assert_eq!(f.view.messages(), vec![texts::NO_PHOTO.to_string()]);
}

#[test]
fn capture_then_download_scales_into_pane_enables_save_and_save_encodes() {
    let current = base64_jpeg(16, 12);
    let mut f = create(Some(current.clone()), 20, plain);
    f.session.start();

    f.session.capture();
    assert!(!f.view.0.borrow().capture_enabled);
    assert!(f.session.is_capturing());

    f.camera.raise_download(fixture_bytes("camera-5184x3456.jpg"));
    assert_ne!(f.view.0.borrow().shown_image_size, Some((400, 477))); // not before pump
    f.session.pump();

    assert_eq!(f.view.0.borrow().shown_image_size, Some((400, 477)));
    assert!(f.view.0.borrow().save_enabled);
    assert!(f.view.0.borrow().capture_enabled);
    assert!(!f.session.is_capturing());

    f.session.save();
    let result = f.session.result_base64().unwrap().to_string();
    assert_ne!(result, current);
    let decoded = pipeline::decode_jpeg(&base64::decode_dotnet(&result).unwrap()).unwrap();
    assert_eq!((decoded.width, decoded.height), (400, 477));
}

#[test]
fn capture_is_ignored_while_capturing() {
    let mut f = create(None, 20, plain);
    f.session.start();
    f.session.capture();
    f.session.capture();
    assert_eq!(f.camera.take_photo_count(), 1);
}

#[test]
fn capture_that_throws_box_only_releases_camera_and_is_not_stuck() {
    let mut f = create(None, 20, |s| s.throw_on_take_photo = Some(CameraError::Other("0x00008D01".into())));
    f.session.start();

    f.session.capture();
    assert_eq!(f.view.messages(), vec![texts::CAPTURE_ERROR.to_string()]);
    assert_eq!(f.view.toasts().len(), 0);
    assert!(f.camera.calls().contains(&"ReleaseCamera".to_string()));
    assert!(f.view.0.borrow().capture_enabled);
    assert!(!f.view.0.borrow().save_enabled);

    // Decision 5 (deviation): isCapturing reset, so a later click works again.
    assert!(!f.session.is_capturing());
    f.session.capture();
    assert_eq!(f.camera.take_photo_count(), 2);
}

#[test]
fn capture_without_a_camera_is_the_capture_error_box() {
    let mut f = create(None, 20, |s| s.no_camera = true);
    f.session.start();
    f.view.0.borrow_mut().messages.clear();
    f.session.capture(); // the live-view click works although sbCapture is hidden
    assert_eq!(f.view.messages(), vec![texts::CAPTURE_ERROR.to_string()]);
}

#[test]
fn capture_after_a_failure_keeps_save_enabled_when_pane_has_an_image() {
    let mut f = create(Some(base64_jpeg(16, 12)), 20, plain);
    f.session.start();
    f.camera.0.borrow_mut().throw_on_take_photo = Some(CameraError::Other("busy".into()));
    f.session.capture();
    assert!(f.view.0.borrow().save_enabled);
}

#[test]
fn raw_download_toast_with_detail_capture_stays_disabled_live_click_still_works() {
    let mut f = create(Some(base64_jpeg(16, 12)), 20, plain);
    f.session.start();
    f.session.capture();
    f.camera.raise_download(RAW_FILE.to_vec());
    f.session.pump();

    assert_eq!(f.view.messages().len(), 0);
    let toasts = f.view.toasts();
    assert_eq!(toasts[0].0, texts::DOWNLOAD_ERROR);
    assert!(toasts[0].1.ends_with("\r\n"));
    assert!(!f.view.0.borrow().capture_enabled); // only the success path re-enables it
    assert!(!f.session.is_capturing());
    assert_eq!(f.view.0.borrow().shown_image_size, Some((16, 12)));

    f.session.capture();
    assert_eq!(f.camera.take_photo_count(), 2);
}

#[test]
fn raw_plus_jpeg_two_downloads_toast_then_the_photo() {
    let mut f = create(None, 20, plain);
    f.session.start();
    f.session.capture();
    f.camera.raise_download(RAW_FILE.to_vec());
    f.camera.raise_download(fixture_bytes("camera-3456x2304.jpg"));
    f.session.pump();

    assert_eq!(f.view.toasts().len(), 1);
    assert_eq!(f.view.0.borrow().shown_image_size, Some((345, 460)));
    assert!(f.view.0.borrow().save_enabled);
}

#[test]
fn download_failed_is_the_download_toast() {
    let mut f = create(None, 20, plain);
    f.session.start();
    f.session.capture();
    f.camera.raise_download_failed("EdsDownload: EDSDK error 0x00000081");
    f.session.pump();
    assert_eq!(
        f.view.toasts(),
        vec![(texts::DOWNLOAD_ERROR.to_string(), "EdsDownload: EDSDK error 0x00000081\r\n".to_string())]
    );
    assert!(!f.session.is_capturing());
}

#[test]
fn no_download_ever_leaves_capture_busy_as_in_csharp() {
    let mut f = create(None, 20, plain);
    f.session.start();
    f.session.capture();
    f.session.capture();
    assert!(f.session.is_capturing());
    assert_eq!(f.camera.take_photo_count(), 1);
}

#[test]
fn shutdown_closes_the_session_or_shows_the_state_box() {
    let mut f = create(None, 20, plain);
    f.session.start();
    f.camera.raise_shutdown();
    f.session.pump();
    assert_eq!(f.session.live_view_state(), LiveViewState::NoSession);
    assert_eq!(f.view.messages().len(), 0);

    f.camera.0.borrow_mut().throw_on_close_session = Some(CameraError::Other("EdsCloseSession".into()));
    f.camera.raise_shutdown();
    f.session.pump();
    assert_eq!(f.view.messages(), vec![texts::CAMERA_STATE_ERROR.to_string()]);
}

#[test]
fn live_view_failed_is_the_live_view_box_no_toast() {
    let mut f = create(None, 20, plain);
    f.session.live_view_failed(&["frame"]);
    assert_eq!(f.view.messages(), vec![texts::LIVE_VIEW_ERROR.to_string()]);
    assert_eq!(f.view.toasts().len(), 0);
}

#[test]
fn close_disposes_the_camera_once_and_ignores_late_shots() {
    let mut f = create(None, 20, plain);
    f.session.start();
    f.session.capture();
    f.session.close();
    f.session.close();
    f.camera.raise_download(fixture_bytes("camera-3456x2304.jpg"));
    f.session.pump();
    assert_eq!(f.camera.dispose_count(), 1);
    assert_eq!(f.view.0.borrow().show_image_count, 0);
    assert_eq!(f.view.toasts().len(), 0);
}

#[test]
fn close_when_dispose_fails_is_the_close_box() {
    let mut f = create(None, 20, |s| s.throw_on_dispose = Some(CameraError::Other("EdsTerminateSDK".into())));
    f.session.start();
    f.session.close();
    assert_eq!(f.view.messages(), vec![texts::CAMERA_CLOSE_ERROR.to_string()]);
}

#[test]
fn replace_pane_removes_the_original_then_enter_says_no_photo() {
    let mut f = create(Some(base64_jpeg(16, 12)), 20, plain);
    f.session.start();
    f.session.replace_pane(None);
    f.session.save();
    assert_eq!(f.view.messages(), vec![texts::NO_PHOTO.to_string()]);
}
