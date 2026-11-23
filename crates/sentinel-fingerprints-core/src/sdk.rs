//! The RealScan SDK seam: the result codes / capture-mode / slap-type constants
//! `FormFingerprintsV2` uses, the marshalled structs, and the [`RealScanDevice`]
//! trait (`IRealScanDevice`) the session drives. The real implementation (RS_SDK
//! FFI) lives in the plugin crate; tests use a fake.

/// RealScan result codes and mode/slap constants (`RS_SDK.cs`).
pub mod codes {
    /// `RS_SUCCESS`.
    pub const SUCCESS: i32 = 0;

    /// `RS_ERR_SDK_ALREADY_INITIALIZED`.
    pub const ERR_SDK_ALREADY_INITIALIZED: i32 = -11;
    /// `RS_ERR_INVALID_PARAM`.
    pub const ERR_INVALID_PARAM: i32 = -13;
    /// `RS_ERR_MEM_FULL`.
    pub const ERR_MEM_FULL: i32 = -14;
    /// `RS_ERR_INVALID_HANDLE`.
    pub const ERR_INVALID_HANDLE: i32 = -102;
    /// `RS_ERR_CAPTURE_TIMEOUT`.
    pub const ERR_CAPTURE_TIMEOUT: i32 = -202;
    /// `RS_ERR_CAPTURE_ABORTED` (RS_AbortCapture result).
    pub const ERR_CAPTURE_ABORTED: i32 = -203;
    /// `RS_ERR_CANNOT_SEGMENT`.
    pub const ERR_CANNOT_SEGMENT: i32 = -215;
    /// `RS_ERR_CANNOT_SHOW_OVERLAY`.
    pub const ERR_CANNOT_SHOW_OVERLAY: i32 = -303;

    /// `RS_CAPTURE_FLAT_SINGLE_FINGER`.
    pub const CAPTURE_FLAT_SINGLE_FINGER: i32 = 0x02;
    /// `RS_CAPTURE_FLAT_TWO_FINGERS`.
    pub const CAPTURE_FLAT_TWO_FINGERS: i32 = 0x03;
    /// `RS_CAPTURE_FLAT_LEFT_FOUR_FINGERS`.
    pub const CAPTURE_FLAT_LEFT_FOUR_FINGERS: i32 = 0x04;
    /// `RS_CAPTURE_FLAT_RIGHT_FOUR_FINGERS`.
    pub const CAPTURE_FLAT_RIGHT_FOUR_FINGERS: i32 = 0x05;
    /// `RS_CAPTURE_FLAT_SINGLE_FINGER_EX`.
    pub const CAPTURE_FLAT_SINGLE_FINGER_EX: i32 = 0x12;
    /// `RS_CAPTURE_FLAT_TWO_FINGERS_EX`.
    pub const CAPTURE_FLAT_TWO_FINGERS_EX: i32 = 0x13;

    /// `RS_SLAP_LEFT_FOUR`.
    pub const SLAP_LEFT_FOUR: i32 = 1;
    /// `RS_SLAP_RIGHT_FOUR`.
    pub const SLAP_RIGHT_FOUR: i32 = 2;
    /// `RS_SLAP_TWO_THUMB`.
    pub const SLAP_TWO_THUMB: i32 = 4;
    /// `RS_SLAP_ONE_FINGER`.
    pub const SLAP_ONE_FINGER: i32 = 6;

    /// `RS_AUTO_SENSITIVITY_HIGHER` (the Designer default).
    pub const AUTO_SENSITIVITY_HIGHER: i32 = 0x02;
}

/// `RSPoint` (two ints).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RsPoint {
    /// X.
    pub x: i32,
    /// Y.
    pub y: i32,
}

/// `RSDeviceInfo`, decoded ASCII without NUL padding.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RsDeviceText {
    /// Model.
    pub product_name: String,
    /// Device ID.
    pub device_id: String,
    /// Firmware version.
    pub firmware_version: String,
    /// Hardware version.
    pub hardware_version: String,
}

/// One `RSSlapInfo` entry.
#[derive(Debug, Clone, Default)]
pub struct RsSlapInfo {
    /// Finger type (0..10, or out of range).
    pub finger_type: i32,
    /// Four corners in capture-image pixels.
    pub finger_position: [RsPoint; 4],
    /// SDK image quality.
    pub image_quality: i32,
    /// Rotation.
    pub rotation: i32,
}

/// `RS_Segment4` output: four infos + four optional finger buffers.
#[derive(Debug, Clone, Default)]
pub struct RsSegmentation {
    /// The four slap infos.
    pub infos: [RsSlapInfo; 4],
    /// The four segmented finger images (`None` where the SDK returned no buffer).
    pub images: [Option<crate::finger::GreyImage>; 4],
}

/// The result of the capture worker (`DoautoCapture`): the SDK code and, on
/// success, the slap image.
#[derive(Debug, Clone)]
pub struct CaptureOutcome {
    /// The SDK result code.
    pub result: i32,
    /// The captured slap image on success.
    pub image: Option<crate::finger::GreyImage>,
}

/// The RS_SDK calls `FormFingerprintsV2` makes, one method per SDK function,
/// returning the SDK result code so the session reproduces C#'s error handling.
pub trait RealScanDevice {
    /// Preflight: the Enroll-critical exports the bound SDK lacks + its path
    /// (empty = OK). The real implementation can fail to load the module.
    fn missing_enroll_exports(&mut self) -> (Vec<String>, String);
    /// `RS_InitSDK` → number of devices.
    fn init_sdk(&mut self) -> (i32, i32);
    /// `RS_GetSDKInfo` → (code, version, build date).
    fn get_sdk_info(&mut self) -> (i32, String, String);
    /// `RS_InitDevice(index)`.
    fn init_device(&mut self, device_index: i32) -> i32;
    /// `RS_GetDeviceInfo`.
    fn get_device_info(&mut self) -> (i32, RsDeviceText);
    /// `GetClientRect` + `RS_SetViewWindow`.
    fn set_view_window(&mut self, window: usize) -> i32;
    /// `RS_StopViewWindow`.
    fn stop_view_window(&mut self) -> i32;
    /// `RS_AbortCapture`.
    fn abort_capture(&mut self) -> i32;
    /// `RS_ExitDevice`.
    fn exit_device(&mut self) -> i32;
    /// `RS_ExitAllDevices`.
    fn exit_all_devices(&mut self);
    /// `RS_SetCaptureMode(mode, sensitivity, true)`.
    fn set_capture_mode(&mut self, capture_mode: i32, sensitivity: i32) -> i32;
    /// `RS_SetMinimumFinger`.
    fn set_minimum_finger(&mut self, finger_count: i32) -> i32;
    /// `RS_GetErrString` (NUL padding removed).
    fn get_err_string(&mut self, error_code: i32) -> String;
    /// `RS_RemoveAllOverlay`.
    fn remove_all_overlay(&mut self) -> i32;
    /// `RS_TakeImageData(timeout)`; blocks on the worker until capture / timeout /
    /// abort.
    fn take_image(&mut self, timeout_ms: i32) -> (i32, Option<crate::finger::GreyImage>);
    /// `RS_Segment4`; `num_of_fingers` is in/out.
    fn segment4(
        &mut self,
        image: &crate::finger::GreyImage,
        slap_type: i32,
        num_of_fingers: &mut i32,
    ) -> (i32, RsSegmentation);
    /// `RS_EncodeWSQ(raw, w, h, ratio, buffer, &mut length)`.
    fn encode_wsq(
        &mut self,
        image: &crate::finger::GreyImage,
        ratio: f32,
        wsq_buffer: &mut [u8],
        wsq_buffer_length: &mut i32,
    ) -> i32;
    /// `RS_AddOverlayQuadrangle` → the overlay handle.
    fn add_overlay_quadrangle(&mut self, corners: [RsPoint; 4], color: u64, overlay_handle: &mut i32) -> i32;
    /// `RS_ShowOverlay`.
    fn show_overlay(&mut self, overlay_handle: i32, show: bool) -> i32;
    /// `RS_Calibrate`.
    fn calibrate(&mut self) -> i32;
}
