//! The RealScan device used when the real RS_SDK is not linked (every build without
//! the `device` feature, and any non-Windows host).
//!
//! It reports an SDK that loads but finds no scanner, so the enrollment overlay
//! opens and shows the reference's "no device" status — the user can only close it
//! (cancel). Fingerprint capture has no browser substitute, so this is the honest
//! no-hardware state (like Photo's no-camera overlay).

use sentinel_fingerprints_core::finger::GreyImage;
use sentinel_fingerprints_core::sdk::{codes, RealScanDevice, RsDeviceText, RsPoint, RsSegmentation};

/// A RealScan device with no SDK / scanner behind it.
pub struct StubDevice;

impl RealScanDevice for StubDevice {
    fn missing_enroll_exports(&mut self) -> (Vec<String>, String) {
        // No DLL to inspect; skip the preflight so the overlay opens and shows the
        // no-scanner status rather than an export-missing error.
        (Vec::new(), String::new())
    }
    fn init_sdk(&mut self) -> (i32, i32) {
        // SDK "loads", zero devices.
        (codes::SUCCESS, 0)
    }
    fn get_sdk_info(&mut self) -> (i32, String, String) {
        (codes::SUCCESS, String::new(), String::new())
    }
    fn init_device(&mut self, _device_index: i32) -> i32 {
        -100 // "Қурилма топилмади"
    }
    fn get_device_info(&mut self) -> (i32, RsDeviceText) {
        (-100, RsDeviceText::default())
    }
    fn set_view_window(&mut self, _window: usize) -> i32 {
        codes::SUCCESS
    }
    fn stop_view_window(&mut self) -> i32 {
        codes::SUCCESS
    }
    fn abort_capture(&mut self) -> i32 {
        codes::SUCCESS
    }
    fn exit_device(&mut self) -> i32 {
        codes::SUCCESS
    }
    fn exit_all_devices(&mut self) {}
    fn set_capture_mode(&mut self, _capture_mode: i32, _sensitivity: i32) -> i32 {
        codes::SUCCESS
    }
    fn set_minimum_finger(&mut self, _finger_count: i32) -> i32 {
        codes::SUCCESS
    }
    fn get_err_string(&mut self, error_code: i32) -> String {
        format!("RS_SDK {error_code}")
    }
    fn remove_all_overlay(&mut self) -> i32 {
        codes::SUCCESS
    }
    fn take_image(&mut self, _timeout_ms: i32) -> (i32, Option<GreyImage>) {
        // No scanner: report "device not found" at once (no blocking).
        (-100, None)
    }
    fn segment4(&mut self, _image: &GreyImage, _slap_type: i32, num_of_fingers: &mut i32) -> (i32, RsSegmentation) {
        *num_of_fingers = 0;
        (codes::SUCCESS, RsSegmentation::default())
    }
    fn encode_wsq(&mut self, _image: &GreyImage, _ratio: f32, _wsq_buffer: &mut [u8], _len: &mut i32) -> i32 {
        codes::SUCCESS
    }
    fn add_overlay_quadrangle(&mut self, _corners: [RsPoint; 4], _color: u64, overlay_handle: &mut i32) -> i32 {
        *overlay_handle = 1;
        codes::SUCCESS
    }
    fn show_overlay(&mut self, _overlay_handle: i32, _show: bool) -> i32 {
        codes::SUCCESS
    }
    fn calibrate(&mut self) -> i32 {
        -100
    }
}
