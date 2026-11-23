//! `EnrollmentSession` driven by a fake RealScan device + a recording view — the
//! port of the reference `EnrollmentSessionTests` (FormFingerprintsV2's handlers
//! with no window and no scanner).

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use sentinel_fingerprints_core::finger::{FingerPosition, FingerStatus, GreyImage};
use sentinel_fingerprints_core::sdk::{codes, CaptureOutcome, RealScanDevice, RsDeviceText, RsPoint, RsSegmentation};
use sentinel_fingerprints_core::session::{EnrollmentSession, FingerGroup};
use sentinel_fingerprints_core::view::{AlertKind, EnrollView};

// ---- SlapScript ----------------------------------------------------------

#[derive(Clone, Default)]
struct SlapScript {
    result: i32,
    num_of_fingers: i32,
    finger_types: [i32; 4],
    qualities: [i32; 4],
    has_image: [bool; 4],
}

impl SlapScript {
    fn fingers(fingers: &[(i32, i32)]) -> Self {
        let mut s = SlapScript { num_of_fingers: fingers.len() as i32, ..Default::default() };
        for (i, &(t, q)) in fingers.iter().enumerate() {
            s.finger_types[i] = t;
            s.qualities[i] = q;
            s.has_image[i] = true;
        }
        s
    }
    fn error(result: i32) -> Self {
        SlapScript { result, ..Default::default() }
    }
}

// ---- fake device ---------------------------------------------------------

#[derive(Default)]
struct FakeState {
    calls: Vec<String>,
    take_results: VecDeque<i32>,
    slaps: VecDeque<SlapScript>,
    capture_modes: Vec<(i32, i32)>,
    minimum_fingers: Vec<i32>,
    overlays: Vec<[RsPoint; 4]>,
    overlay_colors: Vec<u64>,
    encode_ratios: Vec<f32>,

    num_of_device: i32,
    init_sdk_result: i32,
    init_device_result: i32,
    set_view_result: i32,
    set_capture_mode_result: i32,
    set_minimum_finger_result: i32,
    remove_overlay_result: i32,
    encode_result: i32,
    show_overlay_result: i32,
    calibrate_result: i32,
    exit_device_result: i32,
    wsq_padding: i32,
    segment_width: i32,
    segment_height: i32,

    sdk_inits: i32,
    sdk_exits: i32,
    last_slap_type: i32,
    last_device_index: i32,
    missing_exports: Vec<String>,
}

type Dev = Rc<RefCell<FakeState>>;
struct FakeDevice(Dev);

fn new_device() -> Dev {
    Rc::new(RefCell::new(FakeState {
        num_of_device: 1,
        segment_width: 10,
        segment_height: 12,
        last_device_index: -99,
        ..Default::default()
    }))
}

impl FakeState {
    fn log(&mut self, call: &str) {
        self.calls.push(call.to_string());
    }
}

impl RealScanDevice for FakeDevice {
    fn missing_enroll_exports(&mut self) -> (Vec<String>, String) {
        (self.0.borrow().missing_exports.clone(), "C:\\Fake\\RS_SDK_64.dll".to_string())
    }
    fn init_sdk(&mut self) -> (i32, i32) {
        let mut s = self.0.borrow_mut();
        s.log("InitSdk");
        let r = s.init_sdk_result;
        let num = if r == 0 { s.num_of_device } else { 0 };
        if r == 0 {
            s.sdk_inits += 1;
        }
        (r, num)
    }
    fn get_sdk_info(&mut self) -> (i32, String, String) {
        self.0.borrow_mut().log("GetSdkInfo");
        (0, "1.2.3".to_string(), "2020-01-01".to_string())
    }
    fn init_device(&mut self, device_index: i32) -> i32 {
        let mut s = self.0.borrow_mut();
        s.log(&format!("InitDevice({device_index})"));
        s.last_device_index = device_index;
        s.init_device_result
    }
    fn get_device_info(&mut self) -> (i32, RsDeviceText) {
        self.0.borrow_mut().log("GetDeviceInfo");
        (
            0,
            RsDeviceText {
                product_name: "RealScan-G10".to_string(),
                device_id: "ID42".to_string(),
                firmware_version: "FW1".to_string(),
                hardware_version: "HW2".to_string(),
            },
        )
    }
    fn set_view_window(&mut self, _window: usize) -> i32 {
        let mut s = self.0.borrow_mut();
        s.log("SetViewWindow");
        s.set_view_result
    }
    fn stop_view_window(&mut self) -> i32 {
        self.0.borrow_mut().log("StopViewWindow");
        0
    }
    fn abort_capture(&mut self) -> i32 {
        self.0.borrow_mut().log("AbortCapture");
        0
    }
    fn exit_device(&mut self) -> i32 {
        let mut s = self.0.borrow_mut();
        s.log("ExitDevice");
        s.exit_device_result
    }
    fn exit_all_devices(&mut self) {
        let mut s = self.0.borrow_mut();
        s.log("ExitAllDevices");
        s.sdk_exits += 1;
    }
    fn set_capture_mode(&mut self, capture_mode: i32, sensitivity: i32) -> i32 {
        let mut s = self.0.borrow_mut();
        s.log(&format!("SetCaptureMode({capture_mode})"));
        s.capture_modes.push((capture_mode, sensitivity));
        s.set_capture_mode_result
    }
    fn set_minimum_finger(&mut self, finger_count: i32) -> i32 {
        let mut s = self.0.borrow_mut();
        s.log(&format!("SetMinimumFinger({finger_count})"));
        s.minimum_fingers.push(finger_count);
        s.set_minimum_finger_result
    }
    fn get_err_string(&mut self, error_code: i32) -> String {
        format!("ERR{error_code}")
    }
    fn remove_all_overlay(&mut self) -> i32 {
        let mut s = self.0.borrow_mut();
        s.log("RemoveAllOverlay");
        s.remove_overlay_result
    }
    fn take_image(&mut self, timeout_ms: i32) -> (i32, Option<GreyImage>) {
        let mut s = self.0.borrow_mut();
        s.log(&format!("TakeImage({timeout_ms})"));
        let result = s.take_results.pop_front().unwrap_or(0);
        let image = if result == 0 {
            Some(GreyImage { pixels: vec![0u8; 1600 * 1500], width: 1600, height: 1500 })
        } else {
            None
        };
        (result, image)
    }
    fn segment4(&mut self, _image: &GreyImage, slap_type: i32, num_of_fingers: &mut i32) -> (i32, RsSegmentation) {
        let mut s = self.0.borrow_mut();
        s.log(&format!("Segment4({slap_type})"));
        s.last_slap_type = slap_type;
        let script = s.slaps.pop_front().unwrap_or_default();
        let (sw, sh) = (s.segment_width, s.segment_height);
        let mut seg = RsSegmentation::default();
        *num_of_fingers = script.num_of_fingers;
        for i in 0..4 {
            seg.infos[i].finger_type = script.finger_types[i];
            seg.infos[i].image_quality = script.qualities[i];
            let x = 100 * (i as i32 + 1);
            seg.infos[i].finger_position = [
                RsPoint { x, y: 150 },
                RsPoint { x: x + 80, y: 150 },
                RsPoint { x, y: 450 },
                RsPoint { x: x + 80, y: 450 },
            ];
            if script.has_image[i] {
                let mut pixels = vec![0u8; (sw * sh) as usize];
                pixels[0] = (script.finger_types[i] + 1) as u8;
                seg.images[i] = Some(GreyImage { pixels, width: sw, height: sh });
            }
        }
        (script.result, seg)
    }
    fn encode_wsq(&mut self, image: &GreyImage, ratio: f32, wsq_buffer: &mut [u8], _len: &mut i32) -> i32 {
        let mut s = self.0.borrow_mut();
        s.log("EncodeWsq");
        s.encode_ratios.push(ratio);
        if s.encode_result != 0 {
            return s.encode_result;
        }
        let mut bytes: Vec<u8> = vec![0xFF, 0xA0, image.pixels[0].wrapping_sub(1), image.width as u8];
        let pad = s.wsq_padding.max(0) as usize;
        bytes.resize(bytes.len() + pad, 0x55);
        bytes.push(0xFF);
        bytes.push(0xA1);
        if bytes.len() > wsq_buffer.len() {
            return -14;
        }
        wsq_buffer[..bytes.len()].copy_from_slice(&bytes);
        0
    }
    fn add_overlay_quadrangle(&mut self, corners: [RsPoint; 4], color: u64, overlay_handle: &mut i32) -> i32 {
        let mut s = self.0.borrow_mut();
        s.log("AddOverlayQuadrangle");
        s.overlays.push(corners);
        s.overlay_colors.push(color);
        *overlay_handle = s.overlays.len() as i32;
        0
    }
    fn show_overlay(&mut self, overlay_handle: i32, _show: bool) -> i32 {
        let mut s = self.0.borrow_mut();
        s.log(&format!("ShowOverlay({overlay_handle})"));
        s.show_overlay_result
    }
    fn calibrate(&mut self) -> i32 {
        let mut s = self.0.borrow_mut();
        s.log("Calibrate");
        s.calibrate_result
    }
}

// ---- recording view ------------------------------------------------------

#[derive(Default)]
struct ViewState {
    messages: Vec<String>,
    alerts: Vec<String>,
    questions: Vec<String>,
    plain_boxes: Vec<String>,
    pending: i32,
    question_answer: bool,
}

type Vw = Rc<RefCell<ViewState>>;
struct FakeView(Vw);

fn new_view() -> Vw {
    Rc::new(RefCell::new(ViewState { question_answer: true, ..Default::default() }))
}

fn kind(k: AlertKind) -> &'static str {
    match k {
        AlertKind::Info => "Info",
        AlertKind::Warning => "Warning",
        AlertKind::Error => "Error",
        AlertKind::Question => "Question",
    }
}

impl EnrollView for FakeView {
    fn show_message(&mut self, message: &str, k: AlertKind) {
        self.0.borrow_mut().messages.push(format!("{}:{}", kind(k), message));
    }
    fn show_alert(&mut self, caption: &str, text: &str, k: AlertKind) {
        self.0.borrow_mut().alerts.push(format!("{}:{}|{}", kind(k), caption, text));
    }
    fn show_question(&mut self, message: &str) -> bool {
        let mut s = self.0.borrow_mut();
        s.questions.push(message.to_string());
        s.question_answer
    }
    fn show_plain_message_box(&mut self, text: &str) {
        self.0.borrow_mut().plain_boxes.push(text.to_string());
    }
    fn start_capture(&mut self) {
        self.0.borrow_mut().pending += 1;
    }
    fn preview_handle(&self) -> usize {
        0x1234
    }
    fn preview_client_size(&self) -> (i32, i32) {
        (452, 351)
    }
}

// ---- harness -------------------------------------------------------------

fn session(dev: &Dev, view: &Vw) -> EnrollmentSession {
    EnrollmentSession::new(Box::new(FakeDevice(dev.clone())), Box::new(FakeView(view.clone())))
}

fn loaded() -> (EnrollmentSession, Dev, Vw) {
    let dev = new_device();
    let view = new_view();
    let mut s = session(&dev, &view);
    s.load().unwrap();
    (s, dev, view)
}

/// Dequeue a pending capture and run the worker only (C#'s `PendingCaptures.Dequeue()()`).
fn run_worker(s: &mut EnrollmentSession, view: &Vw) -> CaptureOutcome {
    view.borrow_mut().pending -= 1;
    s.do_auto_capture()
}

/// Run the worker and feed the outcome to `CaptureProcess` (C#'s `CompleteCapture`).
fn complete_capture(s: &mut EnrollmentSession, view: &Vw) {
    let o = run_worker(s, view);
    s.capture_process(o);
}

fn right() -> SlapScript {
    SlapScript::fingers(&[(2, 1), (3, 1), (4, 1), (5, 1)])
}
fn left() -> SlapScript {
    SlapScript::fingers(&[(7, 1), (8, 1), (9, 1), (10, 1)])
}
fn thumbs() -> SlapScript {
    SlapScript::fingers(&[(1, 1), (6, 1)])
}

fn enroll_all(s: &mut EnrollmentSession, dev: &Dev, view: &Vw) {
    dev.borrow_mut().slaps.extend([right(), left(), thumbs()]);
    s.take_auto_click();
    complete_capture(s, view);
    complete_capture(s, view);
    complete_capture(s, view);
}

fn calls(dev: &Dev) -> Vec<String> {
    dev.borrow().calls.clone()
}

// ---- tests ---------------------------------------------------------------

#[test]
fn initial_state_is_the_designer_defaults() {
    let s = session(&new_device(), &new_view());
    assert_eq!(s.status_text(), "Бармоқ изини олишни бошланишини кутинг");
    assert_eq!(s.fingers().len(), 10);
    assert!(s.fingers().iter().all(|f| f.status == FingerStatus::Excist && !f.is_enrolled));
    assert!(s.take_auto_enabled());
    assert!(s.group_take_enabled());
    assert!(!s.save_enabled());
    assert!(s.init_sdk_enabled());
    assert!(!s.init_device_enabled());
    assert!(!s.exit_device_enabled());
    assert_eq!(s.sensitivity(), 2);
}

#[test]
fn load_initialises_sdk_and_device_and_starts_preview() {
    let (s, dev, _) = loaded();
    assert_eq!(calls(&dev).join(","), "InitSdk,GetSdkInfo,InitDevice(0),GetDeviceInfo,SetViewWindow");
    assert_eq!(s.status_text(), "Қуримла ишлашга тайёр бўлди");
    assert_eq!(s.device_list(), ["Сканер қурилмаси 0"]);
    assert_eq!(s.sdk_info(), "1.2.3 2020-01-01");
    assert_eq!(s.device_info(), "RealScan-G10");
    assert!(!s.init_device_enabled());
    assert!(s.exit_device_enabled());
    assert!(s.preview_running());
}

#[test]
fn load_without_scanner_shows_the_dico_message_and_stays_open() {
    let dev = new_device();
    dev.borrow_mut().init_sdk_result = -100;
    dev.borrow_mut().init_device_result = -101;
    let view = new_view();
    let mut s = session(&dev, &view);
    s.load().unwrap();
    assert_eq!(dev.borrow().last_device_index, -1);
    assert_eq!(s.status_text(), "Қурилма индекси хато");
    assert!(!s.preview_running());
}

#[test]
fn take_auto_starts_the_right_hand_with_its_count_and_mode() {
    let (mut s, dev, view) = loaded();
    s.take_auto_click();
    assert_eq!(*dev.borrow().capture_modes.last().unwrap(), (codes::CAPTURE_FLAT_RIGHT_FOUR_FINGERS, 2));
    assert_eq!(*dev.borrow().minimum_fingers.last().unwrap(), 4);
    assert_eq!(s.status_text(), "Ўнг қўлнинг 4та бармоғини сканерга қўйинг");
    assert_eq!(s.selected_position(), FingerPosition::PlainRightFourFingers);
    assert!(!s.take_auto_enabled());
    assert!(!s.group_take_enabled());
    assert_eq!(view.borrow().pending, 1);
}

#[test]
fn capture_worker_removes_overlays_then_takes_the_image_with_10s_timeout() {
    let (mut s, dev, view) = loaded();
    s.take_auto_click();
    run_worker(&mut s, &view);
    assert_eq!(calls(&dev)[7..], ["RemoveAllOverlay".to_string(), "TakeImage(10000)".to_string()]);
}

#[test]
fn auto_chain_right_left_thumbs_enrolls_all_ten_and_enables_save() {
    let (mut s, dev, view) = loaded();
    dev.borrow_mut().slaps.extend([right(), left(), thumbs()]);

    s.take_auto_click();
    complete_capture(&mut s, &view);
    assert_eq!(s.status_text(), "Чап қўлнинг 4та бармоғини сканерга қўйинг");
    assert_eq!(s.selected_position(), FingerPosition::PlainLeftFourFingers);
    assert_eq!(dev.borrow().last_slap_type, codes::SLAP_RIGHT_FOUR);

    complete_capture(&mut s, &view);
    assert_eq!(s.status_text(), "Бош бармоқларнинг 2тасини сканерга қўйинг");
    assert_eq!(dev.borrow().capture_modes.last().unwrap().0, codes::CAPTURE_FLAT_TWO_FINGERS_EX);
    assert_eq!(*dev.borrow().minimum_fingers.last().unwrap(), 2);

    complete_capture(&mut s, &view);
    assert_eq!(s.status_text(), "Бармоқлар №: [1]  [6] ");
    assert!(s.fingers().iter().all(|f| f.is_enrolled && f.wsq.is_some()));
    assert!(s.fingers().iter().all(|f| f.quality == Some(4) && f.tile_rating == 4 && f.tile_image.is_some()));
    assert!(s.save_enabled());
    assert!(!s.is_auto_mode());
    assert!(s.is_rolled());
    assert_eq!(s.selected_position(), FingerPosition::Unknown);
    assert!(s.take_auto_enabled());
    assert_eq!(view.borrow().pending, 0);
    assert!(dev.borrow().encode_ratios.iter().all(|&r| r == 0.75));
}

#[test]
fn save_after_full_enrollment_returns_the_model_fingerprint_json() {
    let (mut s, dev, view) = loaded();
    enroll_all(&mut s, &dev, &view);
    let json = s.save_click().unwrap();
    assert!(json.starts_with("{\"Finger1\":\"/6ABCv+h\",\"FingerQuality1\":\"4\",\"fingerStatus1\":1,\"Finger2\":\"/6ACCv+h\""));
    assert!(json.ends_with("\"Finger10\":\"/6AKCv+h\",\"FingerQuality10\":\"4\",\"fingerStatus10\":1}"));
    assert_eq!(view.borrow().messages.len(), 0);
}

#[test]
fn unknown_finger_type_goes_to_first_excist_not_enrolled_of_slap() {
    let (mut s, dev, view) = loaded();
    s.finger_click(FingerPosition::RightMiddle);
    s.menu_absent();
    dev.borrow_mut().slaps.push_back(SlapScript::fingers(&[(0, 1), (0, 2), (0, 3)]));
    s.take_auto_click();
    assert_eq!(*dev.borrow().minimum_fingers.last().unwrap(), 3);
    assert_eq!(s.status_text(), "Ўнг қўлнинг 3та бармоғини сканерга қўйинг");
    complete_capture(&mut s, &view);

    assert_eq!(s.fingers()[1].quality, Some(4)); // finger 2 ← q1
    assert!(!s.fingers()[2].is_enrolled); // 3 absent, skipped
    assert_eq!(s.fingers()[3].quality, Some(3)); // 4 ← q2
    assert_eq!(s.fingers()[4].quality, Some(2)); // 5 ← q3
    assert_eq!(s.status_text(), "Чап қўлнинг 4та бармоғини сканерга қўйинг");
}

#[test]
fn finger_type_outside_0_to_10_is_discarded_but_still_consumes_its_buffer() {
    let (mut s, dev, view) = loaded();
    dev.borrow_mut().slaps.push_back(SlapScript::fingers(&[(13, 1), (-1, 1), (4, 2)]));
    s.take_auto_click();
    complete_capture(&mut s, &view);

    assert_eq!(s.fingers().iter().filter(|f| f.is_enrolled).count(), 1);
    assert!(s.fingers()[3].is_enrolled);
    assert_eq!(s.fingers()[3].quality, Some(3));
    assert_eq!(dev.borrow().overlays.len(), 1);
    assert_eq!(dev.borrow().overlays[0][0].x, 300 * 452 / 1600);
}

#[test]
fn overlay_corners_use_order_0132_scaled_to_the_preview_with_integer_math() {
    let (mut s, dev, view) = loaded();
    dev.borrow_mut().slaps.push_back(SlapScript::fingers(&[(2, 1)]));
    s.take_auto_click();
    complete_capture(&mut s, &view);

    let c = dev.borrow().overlays[0];
    assert_eq!((c[0].x, c[0].y), (100 * 452 / 1600, 150 * 351 / 1500));
    assert_eq!((c[1].x, c[1].y), (180 * 452 / 1600, 150 * 351 / 1500));
    assert_eq!((c[2].x, c[2].y), (180 * 452 / 1600, 450 * 351 / 1500));
    assert_eq!((c[3].x, c[3].y), (100 * 452 / 1600, 450 * 351 / 1500));
    assert_eq!(dev.borrow().overlay_colors[0], 0x0000_ff00);
}

#[test]
fn quality_and_overlay_colour_follow_the_csharp_formulas() {
    assert_eq!(EnrollmentSession::quality_of(0), 0);
    assert_eq!(EnrollmentSession::quality_of(1), 4);
    assert_eq!(EnrollmentSession::quality_of(4), 1);
    assert_eq!(EnrollmentSession::quality_of(5), 0);
    assert_eq!(EnrollmentSession::quality_of(6), -1);
    assert_eq!(EnrollmentSession::overlay_color(1), 0x0000_ff00);
    assert_eq!(EnrollmentSession::overlay_color(2), 0x00ff_0000);
    assert_eq!(EnrollmentSession::overlay_color(3), 0x00ff_0000);
    assert_eq!(EnrollmentSession::overlay_color(4), 0xff00_0000);
}

#[test]
fn single_finger_uses_clicked_finger_prints_enum_name_and_leaves_tile_alone() {
    let (mut s, dev, view) = loaded();
    enroll_all(&mut s, &dev, &view);
    let old_tile = s.fingers()[3].tile_image.clone();
    let old_wsq = s.fingers()[3].wsq.clone();

    s.finger_click(FingerPosition::RightRing);
    dev.borrow_mut().slaps.push_back(SlapScript::fingers(&[(0, 4)]));
    s.menu_enroll();

    assert_eq!(dev.borrow().capture_modes.last().unwrap().0, codes::CAPTURE_FLAT_SINGLE_FINGER);
    assert_eq!(*dev.borrow().minimum_fingers.last().unwrap(), 1);
    assert_eq!(s.status_text(), "RightRingFinger бармоқни сканерга қўйинг");
    assert_eq!(s.selected_position(), FingerPosition::RightRing);

    complete_capture(&mut s, &view);

    let ring = &s.fingers()[3];
    assert_eq!(ring.quality, Some(1)); // new capture q4 → 1
    assert_ne!(ring.wsq, old_wsq);
    assert_eq!(ring.tile_rating, 4); // tile not repainted in single mode
    assert_eq!(ring.tile_image, old_tile);
    assert_eq!(dev.borrow().last_slap_type, codes::SLAP_ONE_FINGER);
    assert_eq!(view.borrow().pending, 0);
}

#[test]
fn single_finger_sdk_type_wins_over_the_clicked_finger() {
    let (mut s, dev, view) = loaded();
    s.finger_click(FingerPosition::LeftIndex);
    dev.borrow_mut().slaps.push_back(SlapScript::fingers(&[(2, 1)]));
    s.menu_enroll();
    complete_capture(&mut s, &view);

    assert_eq!(s.status_text(), "Бармоқлар №: [2] ");
    assert!(s.fingers()[1].is_enrolled);
    assert!(!s.fingers()[6].is_enrolled);
}

#[test]
fn single_finger_names_for_all_ten_match_the_framework_enum() {
    let expected = [
        "RightThumb", "RightIndex", "RightMiddle", "RightRingFinger", "RightLittleFinger",
        "LeftThumb", "LeftIndexFinger", "LeftMiddle", "LeftRingFinger", "LeftLittleFinger",
    ];
    for i in 1..=10 {
        let (mut s, _, _) = loaded();
        s.finger_click(FingerPosition::from_value(i).unwrap());
        s.menu_enroll();
        assert_eq!(s.status_text(), format!("{} бармоқни сканерга қўйинг", expected[(i - 1) as usize]));
    }
}

#[test]
fn capture_error_shows_dico_reenables_buttons_and_stops_the_chain() {
    let (mut s, dev, view) = loaded();
    dev.borrow_mut().take_results.push_back(codes::ERR_CAPTURE_TIMEOUT);
    s.take_auto_click();
    complete_capture(&mut s, &view);

    assert_eq!(s.status_text(), "Берилган вақт ичида суратга олинмади!");
    assert!(s.take_auto_enabled());
    assert!(s.group_take_enabled());
    assert_eq!(view.borrow().pending, 0);
    assert!(!calls(&dev).contains(&"Segment4(2)".to_string()));
}

#[test]
fn remove_all_overlay_error_skips_the_capture() {
    let (mut s, dev, view) = loaded();
    dev.borrow_mut().remove_overlay_result = codes::ERR_INVALID_HANDLE;
    s.take_auto_click();
    complete_capture(&mut s, &view);

    assert_eq!(s.status_text(), "Invalid Handle!");
    assert!(!calls(&dev).iter().any(|c| c.starts_with("TakeImage")));
}

#[test]
fn segmentation_error_still_advances_the_auto_chain() {
    let (mut s, dev, view) = loaded();
    dev.borrow_mut().slaps.push_back(SlapScript::error(codes::ERR_CANNOT_SEGMENT));
    s.take_auto_click();
    complete_capture(&mut s, &view);

    assert!(!s.fingers().iter().any(|f| f.is_enrolled));
    assert_eq!(s.status_text(), "Чап қўлнинг 4та бармоғини сканерга қўйинг");
    assert_eq!(view.borrow().pending, 1);
}

#[test]
fn set_capture_mode_error_keeps_slap_type_and_still_starts_capture() {
    let (mut s, dev, view) = loaded();
    dev.borrow_mut().set_capture_mode_result = codes::ERR_INVALID_PARAM;
    s.finger_click(FingerPosition::LeftThumb);
    s.menu_enroll();

    assert_eq!(s.slap_type(), codes::SLAP_RIGHT_FOUR);
    assert_eq!(s.status_text(), "LeftThumb бармоқни сканерга қўйинг");
    assert_eq!(view.borrow().pending, 1);
}

#[test]
fn set_minimum_finger_error_shows_the_scanner_setup_toast_and_keeps_status() {
    let (mut s, dev, view) = loaded();
    dev.borrow_mut().set_minimum_finger_result = codes::ERR_INVALID_PARAM;
    s.take_auto_click();

    assert_eq!(view.borrow().alerts, ["Error:Сканер созлаш|ERR-13"]);
    assert_eq!(s.status_text(), "Қуримла ишлашга тайёр бўлди");
    assert_eq!(view.borrow().pending, 1);
}

#[test]
fn overlay_show_error_returns_early_drops_later_segments_and_leaves_buttons_disabled() {
    let (mut s, dev, view) = loaded();
    dev.borrow_mut().show_overlay_result = codes::ERR_CANNOT_SHOW_OVERLAY;
    dev.borrow_mut().slaps.push_back(right());
    s.take_auto_click();
    complete_capture(&mut s, &view);

    assert_eq!(s.status_text(), "Бармоқ излари ажратилмаёпти -303");
    assert_eq!(s.fingers().iter().filter(|f| f.is_enrolled).count(), 1);
    assert!(!s.take_auto_enabled());
    assert_eq!(view.borrow().pending, 0);

    s.stop_click();
    assert!(s.take_auto_enabled());
    assert!(s.group_take_enabled());
    assert_eq!(calls(&dev).last().unwrap(), "AbortCapture");
}

#[test]
fn encode_error_leaves_finger_enrolled_without_wsq_so_save_enables_but_rejects() {
    let (mut s, dev, view) = loaded();
    dev.borrow_mut().encode_result = codes::ERR_MEM_FULL;
    enroll_all(&mut s, &dev, &view);

    assert!(s.fingers().iter().all(|f| f.is_enrolled && f.wsq.is_none() && f.tile_image.is_none()));
    assert!(s.save_enabled());

    assert!(s.save_click().is_none());
    assert_eq!(view.borrow().messages, ["Error:Бармоқ излари тўлиқ берилмаган"]);
}

#[test]
fn wsq_over_18000_shows_the_warning_box_and_keeps_the_finger() {
    let (mut s, dev, view) = loaded();
    {
        let mut d = dev.borrow_mut();
        d.wsq_padding = 18000;
        d.segment_width = 200;
        d.segment_height = 200;
        d.slaps.push_back(SlapScript::fingers(&[(2, 1)]));
    }
    s.take_auto_click();
    complete_capture(&mut s, &view);

    assert_eq!(view.borrow().plain_boxes, ["Сурат ўлчами тавсия этилган ўлчамдан катта \n Ушбу бармоқни қайта сканер қилинг"]);
    assert_eq!(s.fingers()[1].wsq.as_ref().unwrap().len(), 18006);
}

#[test]
fn enrolled_then_absent_still_counts_and_is_saved_with_its_wsq() {
    let (mut s, dev, view) = loaded();
    enroll_all(&mut s, &dev, &view);

    s.finger_click(FingerPosition::RightMiddle);
    s.menu_absent();

    let middle = &s.fingers()[2];
    assert_eq!(middle.status, FingerStatus::Absent);
    assert!(middle.is_enrolled);
    assert_eq!(middle.tile_back, sentinel_fingerprints_core::finger::TileBack::Red);
    assert!(s.save_enabled());
    assert!(s.save_click().unwrap().contains("\"Finger3\":\"/6ADCv+h\",\"FingerQuality3\":\"4\",\"fingerStatus3\":3"));
}

#[test]
fn check_all_fingers_is_count_based() {
    let (mut s, dev, view) = loaded();
    dev.borrow_mut().slaps.push_back(right());
    s.group_button_click(FingerGroup::RightHand, 1);
    complete_capture(&mut s, &view);
    for p in [FingerPosition::RightIndex, FingerPosition::RightMiddle, FingerPosition::RightRing, FingerPosition::RightLittle, FingerPosition::LeftIndex] {
        s.finger_click(p);
        s.menu_absent();
    }
    assert!(!s.save_enabled()); // 4 enrolled < 5 Excist
    s.finger_click(FingerPosition::LeftMiddle);
    s.menu_absent();
    assert!(s.save_enabled());

    assert!(s.save_click().is_none());
    assert_eq!(view.borrow().messages, ["Error:Бармоқ излари тўлиқ берилмаган"]);
}

#[test]
fn mark_damaged_absent_normal_moves_between_lists_sets_tile_colour_and_keeps_wsq() {
    use sentinel_fingerprints_core::finger::TileBack;
    let (mut s, dev, view) = loaded();
    enroll_all(&mut s, &dev, &view);
    let wsq = s.fingers()[0].wsq.clone();
    s.finger_click(FingerPosition::RightThumb);

    s.menu_damaged();
    assert_eq!(s.fingers()[0].status, FingerStatus::Damaged);
    assert_eq!(s.fingers()[0].tile_back, TileBack::Orange);
    assert_eq!(s.damaged_positions(), [FingerPosition::RightThumb]);

    s.menu_absent();
    assert_eq!(s.fingers()[0].status, FingerStatus::Absent);
    assert_eq!(s.damaged_positions().len(), 0);
    assert_eq!(s.missing_positions(), [FingerPosition::RightThumb]);

    s.menu_normal();
    assert_eq!(s.fingers()[0].status, FingerStatus::Excist);
    assert_eq!(s.fingers()[0].tile_back, TileBack::White);
    assert_eq!(s.missing_positions().len(), 0);
    assert_eq!(s.fingers()[0].wsq, wsq);
}

#[test]
fn mark_damaged_twice_is_a_noop_and_unenrolled_finger_stays_unenrolled() {
    let (mut s, _, _) = loaded();
    s.finger_click(FingerPosition::LeftLittle);
    s.menu_damaged();
    s.menu_damaged();
    assert_eq!(s.damaged_positions().len(), 1);
    assert!(!s.fingers()[9].is_enrolled);
}

#[test]
fn absent_fingers_lower_the_slap_count() {
    let (mut s, dev, _) = loaded();
    s.finger_click(FingerPosition::RightIndex);
    s.menu_absent();
    s.finger_click(FingerPosition::RightLittle);
    s.menu_damaged();
    s.take_auto_click();
    assert_eq!(*dev.borrow().minimum_fingers.last().unwrap(), 2);
    assert_eq!(s.status_text(), "Ўнг қўлнинг 2та бармоғини сканерга қўйинг");
}

#[test]
fn group_clear_asks_the_group_question_and_clears_only_that_group() {
    let (mut s, dev, view) = loaded();
    enroll_all(&mut s, &dev, &view);
    s.group_button_click(FingerGroup::LeftHand, 0);

    assert_eq!(view.borrow().questions, ["Олинган чап қўл бармоқ излари тозалансинми?"]);
    for n in [7usize, 8, 9, 10] {
        let f = &s.fingers()[n - 1];
        assert!(f.wsq.is_none() && f.quality.is_none() && !f.is_enrolled && f.tile_image.is_none() && f.tile_rating == 0);
        assert!(f.width.is_none());
    }
    assert!(s.fingers()[1].is_enrolled);
    assert!(!s.save_enabled());
}

#[test]
fn group_clear_answered_no_clears_nothing() {
    let (mut s, dev, view) = loaded();
    enroll_all(&mut s, &dev, &view);
    view.borrow_mut().question_answer = false;
    s.group_button_click(FingerGroup::Thumbs, 0);

    assert_eq!(view.borrow().questions, ["Олинган бош бармоқлар тозалансинми?"]);
    assert!(s.fingers()[0].is_enrolled);
    assert!(s.save_enabled());
}

#[test]
fn group_clear_right_asks_the_right_question() {
    let (mut s, _, view) = loaded();
    s.group_button_click(FingerGroup::RightHand, 0);
    assert_eq!(view.borrow().questions, ["Олинган ўнг қўл бармоқ излари тозалансинми?"]);
}

#[test]
fn group_take_captures_only_that_group_without_chaining() {
    let (mut s, dev, view) = loaded();
    dev.borrow_mut().slaps.push_back(thumbs());
    s.group_button_click(FingerGroup::Thumbs, 1);

    assert_eq!(s.status_text(), "Бош бармоқларнинг 2тасини сканерга қўйинг");
    assert_eq!(s.selected_position(), FingerPosition::PlainThumbs);
    complete_capture(&mut s, &view);

    assert!(s.fingers()[0].is_enrolled && s.fingers()[5].is_enrolled);
    assert!(s.fingers()[0].tile_image.is_some());
    assert_eq!(view.borrow().pending, 0);
}

#[test]
fn calibrate_success_asks_to_close_as_cancel() {
    let (mut s, _, view) = loaded();
    assert!(s.calibrate_click());
    assert_eq!(view.borrow().messages, ["Info:Калибровка мувафақиятли якунланди"]);
}

#[test]
fn calibrate_failure_shows_the_sdk_error_string_as_info() {
    let (mut s, dev, view) = loaded();
    dev.borrow_mut().calibrate_result = -114;
    assert!(!s.calibrate_click());
    assert_eq!(view.borrow().messages, ["Info:ERR-114"]);
}

#[test]
fn exit_device_stops_preview_and_resets_the_buttons() {
    let (mut s, dev, _) = loaded();
    s.exit_device_click();
    assert_eq!(s.status_text(), "Қурилма узилди");
    assert!(s.init_sdk_enabled() && !s.init_device_enabled() && !s.exit_device_enabled() && !s.take_auto_enabled());
    assert_eq!(s.device_info(), "");
    assert_eq!(calls(&dev)[5..], ["AbortCapture".to_string(), "StopViewWindow".to_string(), "ExitDevice".to_string()]);
}

#[test]
fn init_sdk_again_after_exit_reports_already_initialised() {
    let (mut s, dev, _) = loaded();
    s.exit_device_click();
    dev.borrow_mut().init_sdk_result = codes::ERR_SDK_ALREADY_INITIALIZED;
    s.init_sdk_click();
    assert_eq!(s.status_text(), "SDK initialize қилиб бўлинган");
}

#[test]
fn closing_exits_the_device_and_releases_the_sdk_balanced() {
    let dev = new_device();
    for _ in 0..2 {
        let mut s = session(&dev, &new_view());
        s.load().unwrap();
        s.closing();
    }
    assert_eq!(dev.borrow().sdk_inits, 2);
    assert_eq!(dev.borrow().sdk_exits, 2);
    assert_eq!(calls(&dev).iter().filter(|c| *c == "ExitDevice").count(), 2);
}

#[test]
fn tab_change_shows_save_to_file_only_when_save_is_enabled() {
    let (mut s, dev, view) = loaded();
    s.tab_changed();
    assert!(!s.save_to_file_visible());

    enroll_all(&mut s, &dev, &view);
    assert!(!s.save_to_file_visible()); // only updated on a tab change
    s.tab_changed();
    assert!(s.save_to_file_visible());
}

#[test]
fn sensitivity_is_passed_to_set_capture_mode() {
    let (mut s, dev, _) = loaded();
    s.sensitivity_changed(0);
    s.take_auto_click();
    assert_eq!(dev.borrow().capture_modes.last().unwrap().1, 0);
}

#[test]
fn describe_unknown_code_falls_back_to_the_sdk_string() {
    let (mut s, _, _) = loaded();
    assert_eq!(s.describe(-202), "Берилган вақт ичида суратга олинмади!");
    assert_eq!(s.describe(-999), "ERR-999");
}

#[test]
fn shows_tile_only_for_fingers_of_the_captured_slap() {
    assert!(EnrollmentSession::shows_tile(codes::SLAP_RIGHT_FOUR, 2));
    assert!(!EnrollmentSession::shows_tile(codes::SLAP_RIGHT_FOUR, 7));
    assert!(EnrollmentSession::shows_tile(codes::SLAP_LEFT_FOUR, 10));
    assert!(EnrollmentSession::shows_tile(codes::SLAP_TWO_THUMB, 6));
    assert!(!EnrollmentSession::shows_tile(codes::SLAP_TWO_THUMB, 2));
    assert!(!EnrollmentSession::shows_tile(codes::SLAP_ONE_FINGER, 4));
}

#[test]
fn export_preflight_missing_fails_load() {
    let dev = new_device();
    dev.borrow_mut().missing_exports = vec!["RS_EncodeWSQ".to_string()];
    let mut s = session(&dev, &new_view());
    let err = s.load().unwrap_err();
    assert!(err.message().contains("does not export RS_EncodeWSQ"));
    // Preflight runs before any SDK call.
    assert!(!calls(&dev).contains(&"InitSdk".to_string()));
}
