//! `EnrollmentSession`: the logic of pv-winform `FormFingerprintsV2` without the UI
//! toolkit — every event handler is a method, every control property it touches is
//! a field, and the overlay only mirrors this state. Ported from the C# line by
//! line, quirks included (see `docs/fingerprints-enroll.md`).

use crate::error_dico;
use crate::finger::{EnrolledFinger, FingerPosition, FingerStatus, FingerType, GreyImage, TileBack};
use crate::model_json::{build, serialize, IncompleteError};
use crate::sdk::{codes, CaptureOutcome, RealScanDevice, RsPoint, RsSegmentation};
use crate::texts;
use crate::view::{AlertKind, EnrollView};

/// `RS_EncodeWSQ` ratio (decision 2).
pub const WSQ_RATIO: f32 = 0.75;
/// `RS_TakeImageData` timeout.
pub const CAPTURE_TIMEOUT_MS: i32 = 10000;

/// The three layout groups with header buttons.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FingerGroup {
    /// The two thumbs.
    Thumbs,
    /// The right hand (fingers 2..5).
    RightHand,
    /// The left hand (fingers 7..10).
    LeftHand,
}

/// `Load` failed its export preflight before `RS_InitSDK` (not a C# path).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SdkExportMissing {
    /// The SDK module the DllImports bound to.
    pub path: String,
    /// The Enroll-critical exports it lacks.
    pub missing: Vec<String>,
}

impl SdkExportMissing {
    /// The `error_message` C# would surface (§3.6).
    pub fn message(&self) -> String {
        let first = self.missing.first().map(String::as_str).unwrap_or("");
        format!(
            "Asbt.Fingerprints Enroll: {} does not export {}. Enrollment needs RS_EncodeWSQ and RS_Segment4; install an RS_SDK build that has them.",
            self.path, first
        )
    }
}

impl std::fmt::Display for SdkExportMissing {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message())
    }
}

impl std::error::Error for SdkExportMissing {}

/// The reference `FormFingerprintsV2` state machine.
pub struct EnrollmentSession {
    device: Box<dyn RealScanDevice>,
    view: Box<dyn EnrollView>,

    fingers: Vec<EnrolledFinger>,
    missing: Vec<FingerPosition>,
    damaged: Vec<FingerPosition>,
    device_list: Vec<String>,

    slap_type: i32,
    prev_stopped: bool,
    is_auto_mode: bool,

    status_text: String,
    take_auto_enabled: bool,
    group_take_enabled: bool,
    save_enabled: bool,
    save_to_file_visible: bool,
    init_sdk_enabled: bool,
    init_device_enabled: bool,
    exit_device_enabled: bool,
    selected_device_index: i32,
    device_info: String,
    firmware_info: String,
    hardware_info: String,
    device_id: String,
    sdk_info: String,
    sensitivity: i32,
    selected_position: FingerPosition,
    is_rolled: bool,
    current_position: FingerPosition,
}

impl EnrollmentSession {
    /// Build the session with the Designer defaults.
    pub fn new(device: Box<dyn RealScanDevice>, view: Box<dyn EnrollView>) -> Self {
        let fingers = (1..=10).map(|n| EnrolledFinger::new(FingerType::from_number(n).unwrap())).collect();
        EnrollmentSession {
            device,
            view,
            fingers,
            missing: Vec::new(),
            damaged: Vec::new(),
            device_list: Vec::new(),
            slap_type: codes::SLAP_RIGHT_FOUR,
            prev_stopped: true,
            is_auto_mode: false,
            status_text: texts::MSG_PANEL_INITIAL.to_string(),
            take_auto_enabled: true,
            group_take_enabled: true,
            save_enabled: false,
            save_to_file_visible: false,
            init_sdk_enabled: true,
            init_device_enabled: false,
            exit_device_enabled: false,
            selected_device_index: -1,
            device_info: String::new(),
            firmware_info: String::new(),
            hardware_info: String::new(),
            device_id: String::new(),
            sdk_info: String::new(),
            sensitivity: codes::AUTO_SENSITIVITY_HIGHER,
            selected_position: FingerPosition::Unknown,
            is_rolled: false,
            current_position: FingerPosition::Unknown,
        }
    }

    // ---- Mirrored state (read by the overlay / tests) ----

    /// The ten fingers.
    pub fn fingers(&self) -> &[EnrolledFinger] {
        &self.fingers
    }
    /// `MsgPanel.Text`.
    pub fn status_text(&self) -> &str {
        &self.status_text
    }
    /// `sbTakeAuto.Enabled`.
    pub fn take_auto_enabled(&self) -> bool {
        self.take_auto_enabled
    }
    /// The three "Олиш" header buttons.
    pub fn group_take_enabled(&self) -> bool {
        self.group_take_enabled
    }
    /// `sbSave.Enabled`.
    pub fn save_enabled(&self) -> bool {
        self.save_enabled
    }
    /// `sbSaveToFile.Visible`.
    pub fn save_to_file_visible(&self) -> bool {
        self.save_to_file_visible
    }
    /// `sbInitSDK.Enabled`.
    pub fn init_sdk_enabled(&self) -> bool {
        self.init_sdk_enabled
    }
    /// `sbInitDevice.Enabled`.
    pub fn init_device_enabled(&self) -> bool {
        self.init_device_enabled
    }
    /// `sbExitDevice.Enabled`.
    pub fn exit_device_enabled(&self) -> bool {
        self.exit_device_enabled
    }
    /// The scanner list.
    pub fn device_list(&self) -> &[String] {
        &self.device_list
    }
    /// The selected scanner index.
    pub fn selected_device_index(&self) -> i32 {
        self.selected_device_index
    }
    /// Set the selected scanner (`DeviceList` combo).
    pub fn set_selected_device_index(&mut self, index: i32) {
        self.selected_device_index = index;
    }
    /// Model.
    pub fn device_info(&self) -> &str {
        &self.device_info
    }
    /// Firmware.
    pub fn firmware_info(&self) -> &str {
        &self.firmware_info
    }
    /// Hardware.
    pub fn hardware_info(&self) -> &str {
        &self.hardware_info
    }
    /// Device ID.
    pub fn device_id(&self) -> &str {
        &self.device_id
    }
    /// SDK version string.
    pub fn sdk_info(&self) -> &str {
        &self.sdk_info
    }
    /// `trackSensitivity.Value`.
    pub fn sensitivity(&self) -> i32 {
        self.sensitivity
    }
    /// `fSelector.SelectedPosition`.
    pub fn selected_position(&self) -> FingerPosition {
        self.selected_position
    }
    /// The absent (missing) positions.
    pub fn missing_positions(&self) -> &[FingerPosition] {
        &self.missing
    }
    /// The damaged positions.
    pub fn damaged_positions(&self) -> &[FingerPosition] {
        &self.damaged
    }
    /// `fSelector.IsRolled`.
    pub fn is_rolled(&self) -> bool {
        self.is_rolled
    }
    /// The finger the radial menu acts on.
    pub fn current_position(&self) -> FingerPosition {
        self.current_position
    }
    /// The current slap type.
    pub fn slap_type(&self) -> i32 {
        self.slap_type
    }
    /// Whether an auto chain is running.
    pub fn is_auto_mode(&self) -> bool {
        self.is_auto_mode
    }
    /// Whether the live preview is running.
    pub fn preview_running(&self) -> bool {
        !self.prev_stopped
    }

    // ---- Form events ----

    /// `FormFingerprintsV2_Load`: the export preflight, then `sbInitSDK_Click` +
    /// `sbInitDevice_Click`.
    pub fn load(&mut self) -> Result<(), SdkExportMissing> {
        let (missing, path) = self.device.missing_enroll_exports();
        if !missing.is_empty() {
            return Err(SdkExportMissing { path, missing });
        }
        self.init_sdk_core();
        self.init_device_core();
        Ok(())
    }

    /// `sbInitSDK_Click`.
    pub fn init_sdk_click(&mut self) {
        self.init_sdk_core();
    }

    /// `sbInitDevice_Click`.
    pub fn init_device_click(&mut self) {
        self.init_device_core();
    }

    /// `sbExitDevice_Click`.
    pub fn exit_device_click(&mut self) {
        self.exit_device_core();
    }

    /// `FormFingerprintsV2_FormClosing` → ExitDevice + `RS_ExitAllDevices` (D11).
    pub fn closing(&mut self) {
        self.exit_device_core();
        self.device.exit_all_devices();
    }

    /// `sbTakeAuto_Click`.
    pub fn take_auto_click(&mut self) {
        self.is_auto_mode = true;
        self.get_right_hand();
    }

    /// `sbStop_Click`.
    pub fn stop_click(&mut self) {
        self.device.abort_capture();
        self.set_buttons_status(true);
        self.selected_position = FingerPosition::Unknown;
    }

    /// `sbCalibrate_Click`. `true` → close the window with Cancel.
    pub fn calibrate_click(&mut self) -> bool {
        let result = self.device.calibrate();
        if result != codes::SUCCESS {
            let msg = self.device.get_err_string(result);
            self.view.show_message(&msg, AlertKind::Info);
            return false;
        }
        self.view.show_message(texts::CALIBRATE_DONE, AlertKind::Info);
        true
    }

    /// `sbSave_Click`: the ModelFingerprint JSON, or `None` (error shown, open).
    pub fn save_click(&mut self) -> Option<String> {
        match build(&self.fingers) {
            Ok(model) => Some(serialize(&model)),
            Err(IncompleteError) => {
                self.view.show_message(texts::FINGERPRINTS_INCOMPLETE, AlertKind::Error);
                None
            }
        }
    }

    /// `sbSaveToFile_Click` content (same as Save, with the error box; D6).
    pub fn save_to_file_json(&mut self) -> Option<String> {
        self.save_click()
    }

    /// `tabPaneFingers_SelectedPageChanged`.
    pub fn tab_changed(&mut self) {
        self.save_to_file_visible = self.save_enabled;
    }

    /// `trackSensitivity_ValueChanged`.
    pub fn sensitivity_changed(&mut self, value: i32) {
        self.sensitivity = value;
    }

    /// `fSelector_FingerClick`.
    pub fn finger_click(&mut self, position: FingerPosition) {
        self.current_position = position;
    }

    /// `brDamaged_ItemClick`.
    pub fn menu_damaged(&mut self) {
        if !self.damaged.contains(&self.current_position) {
            let pos = self.current_position;
            self.missing.retain(|p| *p != pos);
            self.damaged.push(pos);
            if let Some(finger) = self.finger_mut(pos.value()) {
                finger.status = FingerStatus::Damaged;
                if finger.wsq.is_none() {
                    finger.is_enrolled = false;
                }
                finger.tile_back = TileBack::Orange;
            }
            self.check_all_fingers();
        }
    }

    /// `brAbsend_ItemClick`.
    pub fn menu_absent(&mut self) {
        if !self.missing.contains(&self.current_position) {
            let pos = self.current_position;
            self.damaged.retain(|p| *p != pos);
            self.missing.push(pos);
            if let Some(finger) = self.finger_mut(pos.value()) {
                finger.status = FingerStatus::Absent;
                if finger.wsq.is_none() {
                    finger.is_enrolled = false;
                }
                finger.tile_back = TileBack::Red;
            }
            self.check_all_fingers();
        }
    }

    /// `brNormal_ItemClick`.
    pub fn menu_normal(&mut self) {
        let pos = self.current_position;
        self.missing.retain(|p| *p != pos);
        self.damaged.retain(|p| *p != pos);
        if let Some(finger) = self.finger_mut(pos.value()) {
            finger.status = FingerStatus::Excist;
            if finger.wsq.is_none() {
                finger.is_enrolled = false;
            }
            finger.tile_back = TileBack::White;
        }
        self.check_all_fingers();
    }

    /// `brEnrollFinger_ItemClick`: single flat finger for [`current_position`](Self::current_position).
    pub fn menu_enroll(&mut self) {
        self.is_auto_mode = false;
        self.selected_position = self.current_position;

        self.set_capture_mode(codes::CAPTURE_FLAT_SINGLE_FINGER);

        if self.set_minimum_num_of_fingers(1) {
            self.status_text =
                format!("{}{}", texts::finger_position_name(self.current_position.value()), texts::PUT_SINGLE_SUFFIX);
        }

        self.set_buttons_status(false);
        self.selected_position = self.current_position;
        self.view.start_capture();
    }

    /// Group header button (0 = clear after a confirm, 1 = take that group only).
    pub fn group_button_click(&mut self, group: FingerGroup, button_index: i32) {
        if button_index == 0 {
            let question = match group {
                FingerGroup::RightHand => texts::CLEAR_RIGHT_QUESTION,
                FingerGroup::LeftHand => texts::CLEAR_LEFT_QUESTION,
                FingerGroup::Thumbs => texts::CLEAR_THUMBS_QUESTION,
            };
            if self.view.show_question(question) {
                for &number in group_fingers(group) {
                    if let Some(finger) = self.finger_mut(number) {
                        finger.wsq = None;
                        finger.quality = None;
                        finger.is_enrolled = false;
                        finger.width = None;
                        finger.height = None;
                        finger.tile_image = None;
                        finger.tile_rating = 0;
                    }
                }
            }
        } else {
            self.is_auto_mode = false;
            match group {
                FingerGroup::RightHand => self.get_right_hand(),
                FingerGroup::LeftHand => self.get_left_hand(),
                FingerGroup::Thumbs => self.get_thrumbs(),
            }
        }
        self.check_all_fingers();
    }

    // ---- Capture ----

    /// `DoautoCapture` (worker): `RS_RemoveAllOverlay`, then `RS_TakeImageData`.
    pub fn do_auto_capture(&mut self) -> CaptureOutcome {
        let result = self.device.remove_all_overlay();
        if result != codes::SUCCESS {
            return CaptureOutcome { result, image: None };
        }
        let (result, image) = self.device.take_image(CAPTURE_TIMEOUT_MS);
        CaptureOutcome { result, image: if result == codes::SUCCESS { image } else { None } }
    }

    /// `captureProcess` (UI thread), fed with the worker's outcome.
    pub fn capture_process(&mut self, outcome: CaptureOutcome) {
        if outcome.result != codes::SUCCESS {
            self.status_text = self.describe(outcome.result);
            self.set_buttons_status(true);
            return;
        }

        self.status_text = texts::CAPTURE_SUCCEEDED.to_string();
        let captured = match outcome.image {
            Some(img) => img,
            None => return,
        };

        let mut num_of_fingers = 0;
        let (result, segmentation) = self.device.segment4(&captured, self.slap_type, &mut num_of_fingers);
        if result != codes::SUCCESS {
            self.status_text = self.describe(result);
        }
        let segmentation: RsSegmentation = segmentation;

        let preview = self.preview_size();
        let mut im = 0usize;
        let mut overlay_handle = -1;
        const NEEDLESS_FINGER: i32 = 15;
        for item_idx in 0..4 {
            // C#: the i-th info is `item`; `im` advances only on a non-null buffer,
            // and the buffer/position come from `im` (not `item_idx`).
            if im >= 4 || segmentation.images[im].is_none() {
                continue;
            }
            let item_type = segmentation.infos[item_idx].finger_type;
            let item_quality = segmentation.infos[item_idx].image_quality;

            let mut current_finger = if (0..=10).contains(&item_type) { item_type } else { NEEDLESS_FINGER };
            if current_finger == 0 {
                current_finger = self.assign_unknown_finger();
            }

            if current_finger > 0 && current_finger <= 10 {
                let asbt_quality = quality_of(item_quality);
                let segment = segmentation.images[im].clone().unwrap();
                let wsq = self.get_wsq(&segment);
                let shows_tile = shows_tile(self.slap_type, current_finger);
                if let Some(finger) = self.finger_mut(current_finger) {
                    finger.wsq = wsq;
                    finger.quality = Some(asbt_quality);
                    finger.bitmap = if finger.wsq.is_none() { None } else { Some(segment.clone()) };
                    finger.width = Some(segment.width);
                    finger.height = Some(segment.height);
                    if shows_tile {
                        finger.tile_image = finger.bitmap.clone();
                        finger.tile_rating = asbt_quality;
                    }
                    finger.is_enrolled = true;
                }

                let corners =
                    overlay_corners(&segmentation.infos[im].finger_position, preview, captured.width, captured.height);
                self.device.add_overlay_quadrangle(corners, overlay_color(item_quality), &mut overlay_handle);
                let result = self.device.show_overlay(overlay_handle, true);
                if result != codes::SUCCESS {
                    self.status_text = format!("{}{}", texts::OVERLAY_FAILED_PREFIX, result);
                    return;
                }
            }

            im += 1;
        }

        let mut numbers = String::from(texts::FINGERS_NUMBER_PREFIX);
        // D8: clamp to the 4 infos.
        let limit = num_of_fingers.clamp(0, 4) as usize;
        for info in segmentation.infos.iter().take(limit) {
            numbers.push_str(" [");
            numbers.push_str(&info.finger_type.to_string());
            numbers.push_str("] ");
        }
        self.status_text = numbers;
        self.selected_position = FingerPosition::Unknown;

        self.set_buttons_status(true);
        self.is_rolled = true;

        if self.is_auto_mode && self.slap_type == codes::SLAP_RIGHT_FOUR {
            self.get_left_hand();
            return;
        }
        if self.is_auto_mode && self.slap_type == codes::SLAP_LEFT_FOUR {
            self.get_thrumbs();
            return;
        }

        self.is_auto_mode = false;
        self.check_all_fingers();
    }

    /// `captureProcess`: finger type 0 → first Excist, not-yet-enrolled finger of the slap.
    fn assign_unknown_finger(&self) -> i32 {
        let order: &[i32] = if self.slap_type == codes::SLAP_RIGHT_FOUR {
            &[2, 3, 4, 5]
        } else if self.slap_type == codes::SLAP_LEFT_FOUR {
            &[7, 8, 9, 10]
        } else if self.slap_type == codes::SLAP_TWO_THUMB {
            &[1, 6]
        } else {
            // Single finger: the clicked finger if Excist (the enrolled check is
            // commented out in C#).
            let current = self.current_position.value();
            if (1..=10).contains(&current) {
                let clicked = &self.fingers[(current - 1) as usize];
                return if clicked.status == FingerStatus::Excist { current } else { 0 };
            }
            return 0;
        };
        for &number in order {
            let finger = &self.fingers[(number - 1) as usize];
            if finger.status == FingerStatus::Excist && !finger.is_enrolled {
                return number;
            }
        }
        0
    }

    /// `imageQuality == 0 ? 0 : 5 - imageQuality`.
    pub fn quality_of(sdk_quality: i32) -> i32 {
        quality_of(sdk_quality)
    }

    /// The overlay colour for an SDK quality (the raw C# value).
    pub fn overlay_color(sdk_quality: i32) -> u64 {
        overlay_color(sdk_quality)
    }

    /// Whether a tile is repainted for `finger` captured in `slap_type`.
    pub fn shows_tile(slap_type: i32, finger: i32) -> bool {
        shows_tile(slap_type, finger)
    }

    /// `RS_GetErrString` / dico lookup for a status line (the `Describe` helper).
    pub fn describe(&mut self, code: i32) -> String {
        if let Some(message) = error_dico::lookup(code) {
            return message.to_string();
        }
        let message = self.device.get_err_string(code);
        if message.trim().is_empty() {
            format!("RS_SDK хатоси: {code}")
        } else {
            message
        }
    }

    /// `GetWsq`: `RS_EncodeWSQ(0.75)` + `RemoveNullElements` (with the 18000 box).
    fn get_wsq(&mut self, segment: &GreyImage) -> Option<Vec<u8>> {
        let wsq_buffer_len = (segment.width * segment.height).max(0);
        let mut buffer = vec![0u8; wsq_buffer_len as usize];
        let mut len = wsq_buffer_len;
        let result = self.device.encode_wsq(segment, WSQ_RATIO, &mut buffer, &mut len);
        if result != codes::SUCCESS {
            self.status_text = self.describe(result);
            return None;
        }
        let (wsq, too_large) = crate::wsqtrim::remove_null_elements(&buffer);
        if too_large {
            self.view.show_plain_message_box(texts::WSQ_TOO_LARGE);
        }
        Some(wsq)
    }

    fn preview_size(&self) -> RsPoint {
        let (right, bottom) = self.view.preview_client_size();
        RsPoint { x: right, y: bottom }
    }

    // ---- Slap steps ----

    fn get_right_hand(&mut self) {
        let count = self.excist_count(&[2, 3, 4, 5]);
        self.set_capture_mode(codes::CAPTURE_FLAT_RIGHT_FOUR_FINGERS);
        if self.set_minimum_num_of_fingers(count) {
            self.status_text = format!("{}{}{}", texts::PUT_RIGHT_PREFIX, count, texts::PUT_FOUR_SUFFIX);
        }
        self.set_buttons_status(false);
        self.selected_position = FingerPosition::PlainRightFourFingers;
        self.view.start_capture();
    }

    fn get_left_hand(&mut self) {
        let count = self.excist_count(&[7, 8, 9, 10]);
        self.set_capture_mode(codes::CAPTURE_FLAT_LEFT_FOUR_FINGERS);
        if self.set_minimum_num_of_fingers(count) {
            self.status_text = format!("{}{}{}", texts::PUT_LEFT_PREFIX, count, texts::PUT_FOUR_SUFFIX);
        }
        self.set_buttons_status(false);
        self.selected_position = FingerPosition::PlainLeftFourFingers;
        self.view.start_capture();
    }

    fn get_thrumbs(&mut self) {
        let count = self.excist_count(&[1, 6]);
        self.set_capture_mode(codes::CAPTURE_FLAT_TWO_FINGERS_EX);
        if self.set_minimum_num_of_fingers(count) {
            self.status_text = format!("{}{}{}", texts::PUT_THUMBS_PREFIX, count, texts::PUT_THUMBS_SUFFIX);
        }
        self.set_buttons_status(false);
        self.selected_position = FingerPosition::PlainThumbs;
        self.view.start_capture();
    }

    fn excist_count(&self, numbers: &[i32]) -> i32 {
        numbers
            .iter()
            .filter(|&&n| self.fingers[(n - 1) as usize].status == FingerStatus::Excist)
            .count() as i32
    }

    /// `SetCaptureMode`: on failure only the status changes (capture still starts).
    fn set_capture_mode(&mut self, capture_mode: i32) -> bool {
        let result = self.device.set_capture_mode(capture_mode, self.sensitivity);
        if result != codes::SUCCESS {
            self.status_text = self.describe(result);
            return false;
        }
        self.slap_type = match capture_mode {
            codes::CAPTURE_FLAT_SINGLE_FINGER | codes::CAPTURE_FLAT_SINGLE_FINGER_EX => codes::SLAP_ONE_FINGER,
            codes::CAPTURE_FLAT_TWO_FINGERS | codes::CAPTURE_FLAT_TWO_FINGERS_EX => codes::SLAP_TWO_THUMB,
            codes::CAPTURE_FLAT_LEFT_FOUR_FINGERS => codes::SLAP_LEFT_FOUR,
            codes::CAPTURE_FLAT_RIGHT_FOUR_FINGERS => codes::SLAP_RIGHT_FOUR,
            _ => self.slap_type,
        };
        self.take_auto_enabled = true;
        true
    }

    /// `SetMinimumNumOfFingers`: on failure the "Сканер созлаш" toast.
    fn set_minimum_num_of_fingers(&mut self, fingers_number: i32) -> bool {
        let result = self.device.set_minimum_finger(fingers_number);
        if result != codes::SUCCESS {
            let text = self.device.get_err_string(result);
            self.view.show_alert(texts::SCANNER_SETUP_CAPTION, &text, AlertKind::Error);
            return false;
        }
        true
    }

    fn set_buttons_status(&mut self, is_enabled: bool) {
        self.group_take_enabled = is_enabled;
        self.take_auto_enabled = is_enabled;
    }

    /// `CheckAllFingers`: count-based (an enrolled-then-Absent finger still counts).
    fn check_all_fingers(&mut self) {
        let enrolled = self.fingers.iter().filter(|f| f.is_enrolled).count();
        let excist = self.fingers.iter().filter(|f| f.status == FingerStatus::Excist).count();
        self.save_enabled = enrolled >= excist;
    }

    // ---- Device buttons (settings tab) ----

    fn init_sdk_core(&mut self) {
        let (result, num_of_device) = self.device.init_sdk();
        if result != codes::SUCCESS {
            self.status_text = self.describe(result);
            return;
        }
        let (code, version, build_date) = self.device.get_sdk_info();
        if code == codes::SUCCESS {
            self.sdk_info = format!("{version} {build_date}");
        }
        self.device_list.clear();
        self.status_text = texts::SDK_LOADED.to_string();
        for i in 0..num_of_device {
            self.device_list.push(format!("{}{}", texts::DEVICE_ITEM_PREFIX, i));
        }
        if num_of_device > 0 {
            self.selected_device_index = 0;
        }
        self.init_device_enabled = true;
    }

    fn init_device_core(&mut self) {
        let result = self.device.init_device(self.selected_device_index);
        if result != codes::SUCCESS {
            self.status_text = self.describe(result);
            return;
        }
        if !self.prev_stopped {
            self.device.stop_view_window();
        }
        self.prev_stopped = true;

        let (result, info) = self.device.get_device_info();
        if result != codes::SUCCESS {
            self.status_text = self.describe(result);
            return;
        }
        self.device_info = info.product_name;
        self.device_id = info.device_id;
        self.firmware_info = info.firmware_version;
        self.hardware_info = info.hardware_version;

        self.init_device_enabled = false;
        self.exit_device_enabled = true;
        self.status_text = texts::DEVICE_READY.to_string();
        self.set_preview();
    }

    fn set_preview(&mut self) {
        let handle = self.view.preview_handle();
        let result = self.device.set_view_window(handle);
        if result != codes::SUCCESS {
            self.status_text = self.describe(result);
            return;
        }
        self.prev_stopped = false;
    }

    fn exit_device_core(&mut self) {
        self.device.abort_capture();
        if !self.prev_stopped {
            let stop = self.device.stop_view_window();
            if stop != codes::SUCCESS {
                self.status_text = self.describe(stop);
                return;
            }
            self.prev_stopped = true;
        }
        let result = self.device.exit_device();
        if result != codes::SUCCESS {
            self.status_text = self.describe(result);
            return;
        }
        self.device_info.clear();
        self.firmware_info.clear();
        self.device_id.clear();
        self.hardware_info.clear();
        self.init_sdk_enabled = true;
        self.init_device_enabled = false;
        self.exit_device_enabled = false;
        self.take_auto_enabled = false;
        self.status_text = texts::DEVICE_DISCONNECTED.to_string();
    }

    fn finger_mut(&mut self, number: i32) -> Option<&mut EnrolledFinger> {
        if (1..=10).contains(&number) {
            Some(&mut self.fingers[(number - 1) as usize])
        } else {
            None
        }
    }
}

/// `imageQuality == 0 ? 0 : 5 - imageQuality`.
pub fn quality_of(sdk_quality: i32) -> i32 {
    if sdk_quality == 0 {
        0
    } else {
        5 - sdk_quality
    }
}

/// The overlay colour for an SDK quality (the raw C# `ulong` value).
pub fn overlay_color(sdk_quality: i32) -> u64 {
    if sdk_quality < 2 {
        0x0000_ff00
    } else if sdk_quality < 4 {
        0x00ff_0000
    } else {
        0xff00_0000
    }
}

/// A tile is repainted only for a finger belonging to the captured slap.
pub fn shows_tile(slap_type: i32, finger: i32) -> bool {
    if slap_type == codes::SLAP_TWO_THUMB {
        finger == 6 || finger == 1
    } else if slap_type == codes::SLAP_LEFT_FOUR {
        (7..=10).contains(&finger)
    } else if slap_type == codes::SLAP_RIGHT_FOUR {
        (2..=5).contains(&finger)
    } else {
        false
    }
}

/// Corners 0,1,3,2 scaled to the preview client rect (integer math).
pub fn overlay_corners(finger_position: &[RsPoint; 4], preview: RsPoint, captured_w: i32, captured_h: i32) -> [RsPoint; 4] {
    let source = [0usize, 1, 3, 2];
    let mut pos = [RsPoint::default(); 4];
    for k in 0..4 {
        let p = finger_position[source[k]];
        pos[k].x = if captured_w == 0 { 0 } else { p.x * preview.x / captured_w };
        pos[k].y = if captured_h == 0 { 0 } else { p.y * preview.y / captured_h };
    }
    pos
}

/// The 1..10 finger numbers of a group.
pub fn group_fingers(group: FingerGroup) -> &'static [i32] {
    match group {
        FingerGroup::RightHand => &[2, 3, 4, 5],
        FingerGroup::LeftHand => &[7, 8, 9, 10],
        FingerGroup::Thumbs => &[1, 6],
    }
}
