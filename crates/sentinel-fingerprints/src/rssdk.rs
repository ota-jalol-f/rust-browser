//! The real Suprema RealScan device (`RealScanDevice.cs` + `.Enroll.cs` + the
//! `RS_SDK.cs` P/Invoke surface), ported to a Rust FFI over `RS_SDK.dll` /
//! `RS_SDK_64.dll` via `libloading`. x64 Windows `device` builds only.
//!
//! RUNTIME-VERIFY ON A REAL SUPREMA REALSCAN (no scanner or DLL here, so this is
//! compile-checked only — see `docs/fingerprints-enroll.md` §7). Open items that
//! need the device:
//! - `RS_TakeImageData` blocks up to 10 s; the enrollment overlay currently runs
//!   `do_auto_capture` inline on the UI thread, so the device build must move the
//!   worker off-thread (the session already separates do_auto_capture /
//!   capture_process). The handle (`i32`) and the resolved fn pointers are `Send`;
//!   the `libloading::Library` is kept alive by the owning plugin.
//! - the native preview window (`RS_SetViewWindow` needs the overlay's child HWND);
//!   the shell does not yet hand the plugin a native window handle.
//! - the `RSSlapInfoArray` / `RSOverlayQuadrangle` layouts are the C# marshalled
//!   ones (RSSlapInfo 56, array 224, quad 112); the native header is unavailable.
//!
//! Nothing may unwind across FFI, but every call here only returns an `i32` / writes
//! out-params, so no Rust panic path crosses the boundary.

#![allow(unsafe_code)]

use std::ffi::c_void;
use std::path::PathBuf;
use std::sync::Mutex;

use libloading::{Library, Symbol};
use sentinel_fingerprints_core::finger::GreyImage;
use sentinel_fingerprints_core::sdk::{codes, RealScanDevice, RsDeviceText, RsPoint, RsSegmentation};

// ---- marshalled structs (#[repr(C)], matching RS_SDK.cs) -----------------

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct PointRaw {
    x: i32,
    y: i32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct RectRaw {
    x: i32,
    y: i32,
    right: i32,
    bottom: i32,
    left: i32,
    top: i32,
}

#[repr(C)]
struct SdkInfoRaw {
    product: [u8; 16],
    version: [u8; 16],
    build_date: [u8; 16],
    reserved: [i32; 16],
}

#[repr(C)]
struct DeviceInfoRaw {
    device_type: i32,
    product_name: [u8; 16],
    device_id: [u8; 16],
    firmware_version: [u8; 16],
    hardware_version: [u8; 16],
    reserved: [i32; 64],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct SlapInfoRaw {
    finger_type: i32,
    finger_position: [PointRaw; 4],
    image_quality: i32,
    rotation: i32,
    reserved: [i32; 3],
}

#[repr(C)]
struct SlapInfoArrayRaw {
    a: [SlapInfoRaw; 4],
}

#[repr(C)]
struct OverlayQuadRaw {
    pos: [PointRaw; 4],
    color: u64,
    width: i32,
    reserved: [i32; 16],
}

// ---- entry points (all stdcall/Winapi on x64 = extern "system", CharSet Ansi) ----

type FnInitSdk = unsafe extern "system" fn(*const u8, i32, *mut i32) -> i32;
type FnGetSdkInfo = unsafe extern "system" fn(*mut SdkInfoRaw) -> i32;
type FnInitDevice = unsafe extern "system" fn(i32, *mut i32) -> i32;
type FnGetDeviceInfo = unsafe extern "system" fn(i32, *mut DeviceInfoRaw) -> i32;
type FnSetViewWindow = unsafe extern "system" fn(i32, *mut c_void, RectRaw, i32) -> i32;
type FnHandle = unsafe extern "system" fn(i32) -> i32;
type FnVoidRet = unsafe extern "system" fn() -> i32;
type FnSetCaptureMode = unsafe extern "system" fn(i32, i32, i32, i32) -> i32;
type FnSetMinimumFinger = unsafe extern "system" fn(i32, i32) -> i32;
type FnGetErrStringChar = unsafe extern "system" fn(i32, *mut u8) -> i32;
type FnTakeImageData = unsafe extern "system" fn(i32, i32, *mut *mut c_void, *mut i32, *mut i32) -> i32;
#[allow(clippy::type_complexity)]
type FnSegment4 = unsafe extern "system" fn(
    *const c_void, i32, i32, i32, *mut i32, *mut *mut c_void,
    *mut *mut c_void, *mut i32, *mut i32,
    *mut *mut c_void, *mut i32, *mut i32,
    *mut *mut c_void, *mut i32, *mut i32,
    *mut *mut c_void, *mut i32, *mut i32,
) -> i32;
type FnEncodeWsq = unsafe extern "system" fn(*const c_void, i32, i32, f32, *mut u8, *mut i32) -> i32;
type FnAddOverlayQuad = unsafe extern "system" fn(i32, *mut OverlayQuadRaw, *mut i32) -> i32;
type FnShowOverlay = unsafe extern "system" fn(i32, i32) -> i32;
type FnFreeImageData = unsafe extern "system" fn(*mut c_void);

#[derive(Clone, Copy)]
struct Api {
    init_sdk: FnInitSdk,
    get_sdk_info: FnGetSdkInfo,
    init_device: FnInitDevice,
    get_device_info: FnGetDeviceInfo,
    set_view_window: FnSetViewWindow,
    stop_view_window: FnHandle,
    abort_capture: FnHandle,
    exit_device: FnHandle,
    exit_all_devices: FnVoidRet,
    set_capture_mode: FnSetCaptureMode,
    set_minimum_finger: FnSetMinimumFinger,
    get_err_string_char: FnGetErrStringChar,
    remove_all_overlay: FnHandle,
    take_image_data: FnTakeImageData,
    segment4: FnSegment4,
    encode_wsq: FnEncodeWsq,
    add_overlay_quadrangle: FnAddOverlayQuad,
    show_overlay: FnShowOverlay,
    free_image_data: FnFreeImageData,
    calibrate: FnHandle,
}

// `GetClientRect` is a standard user32 export; link it directly.
#[link(name = "user32")]
extern "system" {
    fn GetClientRect(hwnd: *mut c_void, rect: *mut RectRaw) -> i32;
}

/// The Enroll-critical exports the SDK must have (`EnrollExports.Required`).
const REQUIRED_EXPORTS: [&[u8]; 2] = [b"RS_EncodeWSQ\0", b"RS_Segment4\0"];

/// Process-wide SDK init state (C#'s static `SdkGate` / `sdkLoaded` / `sdkDeviceCount`).
static SDK: Mutex<SdkShared> = Mutex::new(SdkShared { loaded: false, device_count: 0 });

struct SdkShared {
    loaded: bool,
    device_count: i32,
}

/// A session with the first connected RealScan device.
pub struct RealScan {
    _lib: Library,
    api: Api,
    path: String,
    handle: i32,
    preview_on: bool,
}

impl RealScan {
    /// Load `RS_SDK_64.dll` (preferred) or `RS_SDK.dll` from next to the exe and
    /// resolve the Enroll entry points. `Err` (→ the stub) when neither loads.
    pub fn load() -> Result<Self, String> {
        let dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from("."));
        let (lib, path) = Self::load_from(&dir)?;

        // SAFETY: each symbol is resolved to its documented RS_SDK signature; the fn
        // pointers stay valid while `lib` is held by the returned struct.
        let api = unsafe {
            macro_rules! sym {
                ($name:literal, $ty:ty) => {{
                    let s: Symbol<$ty> = lib.get($name).map_err(|e| e.to_string())?;
                    *s
                }};
            }
            Api {
                init_sdk: sym!(b"RS_InitSDK\0", FnInitSdk),
                get_sdk_info: sym!(b"RS_GetSDKInfo\0", FnGetSdkInfo),
                init_device: sym!(b"RS_InitDevice\0", FnInitDevice),
                get_device_info: sym!(b"RS_GetDeviceInfo\0", FnGetDeviceInfo),
                set_view_window: sym!(b"RS_SetViewWindow\0", FnSetViewWindow),
                stop_view_window: sym!(b"RS_StopViewWindow\0", FnHandle),
                abort_capture: sym!(b"RS_AbortCapture\0", FnHandle),
                exit_device: sym!(b"RS_ExitDevice\0", FnHandle),
                exit_all_devices: sym!(b"RS_ExitAllDevices\0", FnVoidRet),
                set_capture_mode: sym!(b"RS_SetCaptureMode\0", FnSetCaptureMode),
                set_minimum_finger: sym!(b"RS_SetMinimumFinger\0", FnSetMinimumFinger),
                get_err_string_char: sym!(b"RS_GetErrStringChar\0", FnGetErrStringChar),
                remove_all_overlay: sym!(b"RS_RemoveAllOverlay\0", FnHandle),
                take_image_data: sym!(b"RS_TakeImageData\0", FnTakeImageData),
                segment4: sym!(b"RS_Segment4\0", FnSegment4),
                encode_wsq: sym!(b"RS_EncodeWSQ\0", FnEncodeWsq),
                add_overlay_quadrangle: sym!(b"RS_AddOverlayQuadrangle\0", FnAddOverlayQuad),
                show_overlay: sym!(b"RS_ShowOverlay\0", FnShowOverlay),
                free_image_data: sym!(b"RS_FreeImageData\0", FnFreeImageData),
                calibrate: sym!(b"RS_Calibrate\0", FnHandle),
            }
        };
        Ok(RealScan { _lib: lib, api, path, handle: 0, preview_on: false })
    }

    /// Load RS_SDK_64.dll first, then RS_SDK.dll (the C# resolver order).
    fn load_from(dir: &std::path::Path) -> Result<(Library, String), String> {
        for name in ["RS_SDK_64.dll", "RS_SDK.dll"] {
            let path = dir.join(name);
            if path.exists() {
                // SAFETY: loading the vendor SDK from the executable's own folder.
                if let Ok(lib) = unsafe { Library::new(&path) } {
                    return Ok((lib, path.display().to_string()));
                }
            }
        }
        Err("RS_SDK_64.dll / RS_SDK.dll not found next to the executable".into())
    }
}

fn ascii(field: &[u8]) -> String {
    let end = field.iter().position(|&b| b == 0).unwrap_or(field.len());
    String::from_utf8_lossy(&field[..end]).into_owned()
}

impl RealScanDevice for RealScan {
    fn missing_enroll_exports(&mut self) -> (Vec<String>, String) {
        let mut missing = Vec::new();
        for sym in REQUIRED_EXPORTS {
            // SAFETY: a presence check; the symbol is not called.
            let present = unsafe { self._lib.get::<*const c_void>(sym).is_ok() };
            if !present {
                missing.push(String::from_utf8_lossy(&sym[..sym.len() - 1]).into_owned());
            }
        }
        (missing, self.path.clone())
    }

    fn init_sdk(&mut self) -> (i32, i32) {
        let mut shared = SDK.lock().unwrap_or_else(|p| p.into_inner());
        if shared.loaded && shared.device_count > 0 {
            return (codes::SUCCESS, shared.device_count);
        }
        // SAFETY: the SDK init dance from InitSdkShared.
        unsafe {
            if shared.loaded {
                (self.api.exit_all_devices)();
                shared.loaded = false;
            }
            let mut num = 0i32;
            let mut result = (self.api.init_sdk)(std::ptr::null(), 0, &mut num);
            if result == codes::ERR_SDK_ALREADY_INITIALIZED {
                (self.api.exit_all_devices)();
                num = 0;
                result = (self.api.init_sdk)(std::ptr::null(), 0, &mut num);
            }
            if result == codes::SUCCESS {
                shared.loaded = true;
                shared.device_count = num;
            }
            (result, if result == codes::SUCCESS { num } else { 0 })
        }
    }

    fn get_sdk_info(&mut self) -> (i32, String, String) {
        // SAFETY: out-param struct the SDK fills.
        unsafe {
            let mut info: SdkInfoRaw = std::mem::zeroed();
            let result = (self.api.get_sdk_info)(&mut info);
            (result, ascii(&info.version), ascii(&info.build_date))
        }
    }

    fn init_device(&mut self, device_index: i32) -> i32 {
        unsafe { (self.api.init_device)(device_index, &mut self.handle) }
    }

    fn get_device_info(&mut self) -> (i32, RsDeviceText) {
        unsafe {
            let mut raw: DeviceInfoRaw = std::mem::zeroed();
            let result = (self.api.get_device_info)(self.handle, &mut raw);
            (
                result,
                RsDeviceText {
                    product_name: ascii(&raw.product_name),
                    device_id: ascii(&raw.device_id),
                    firmware_version: ascii(&raw.firmware_version),
                    hardware_version: ascii(&raw.hardware_version),
                },
            )
        }
    }

    fn set_view_window(&mut self, window: usize) -> i32 {
        unsafe {
            let mut rect = RectRaw { x: 0, y: 0, right: 0, bottom: 0, left: 0, top: 0 };
            GetClientRect(window as *mut c_void, &mut rect);
            let result = (self.api.set_view_window)(self.handle, window as *mut c_void, rect, 0);
            if result == codes::SUCCESS {
                self.preview_on = true;
            }
            result
        }
    }

    fn stop_view_window(&mut self) -> i32 {
        let result = unsafe { (self.api.stop_view_window)(self.handle) };
        if result == codes::SUCCESS {
            self.preview_on = false;
        }
        result
    }

    fn abort_capture(&mut self) -> i32 {
        unsafe { (self.api.abort_capture)(self.handle) }
    }

    fn exit_device(&mut self) -> i32 {
        let result = unsafe { (self.api.exit_device)(self.handle) };
        if result == codes::SUCCESS {
            self.handle = 0;
        }
        result
    }

    fn exit_all_devices(&mut self) {
        // No RS_ExitAllDevices on close, as in pv-winform (InitSdkShared keeps the
        // SDK loaded for the next Enroll/Scan). D11's balance is handled by init_sdk.
    }

    fn set_capture_mode(&mut self, capture_mode: i32, sensitivity: i32) -> i32 {
        unsafe { (self.api.set_capture_mode)(self.handle, capture_mode, sensitivity, 1) }
    }

    fn set_minimum_finger(&mut self, finger_count: i32) -> i32 {
        unsafe { (self.api.set_minimum_finger)(self.handle, finger_count) }
    }

    fn get_err_string(&mut self, error_code: i32) -> String {
        unsafe {
            let mut buf = [0u8; 1024];
            (self.api.get_err_string_char)(error_code, buf.as_mut_ptr());
            ascii(&buf)
        }
    }

    fn remove_all_overlay(&mut self) -> i32 {
        unsafe { (self.api.remove_all_overlay)(self.handle) }
    }

    fn take_image(&mut self, timeout_ms: i32) -> (i32, Option<GreyImage>) {
        // NOTE: this blocks up to `timeout_ms`; the overlay must run it off the UI
        // thread on the device build (see the module header).
        unsafe {
            let mut data: *mut c_void = std::ptr::null_mut();
            let (mut w, mut h) = (0i32, 0i32);
            let result = (self.api.take_image_data)(self.handle, timeout_ms, &mut data, &mut w, &mut h);
            let image = if result == codes::SUCCESS && !data.is_null() {
                Some(copy_grey(&self.api, data, w, h))
            } else {
                None
            };
            if !data.is_null() {
                (self.api.free_image_data)(data);
            }
            (result, image)
        }
    }

    fn segment4(&mut self, image: &GreyImage, slap_type: i32, num_of_fingers: &mut i32) -> (i32, RsSegmentation) {
        let mut seg = RsSegmentation::default();
        unsafe {
            // A zeroed RSSlapInfoArray; the SDK fills it or replaces the pointer.
            let mut ours: SlapInfoArrayRaw = std::mem::zeroed();
            let ours_ptr = (&mut ours as *mut SlapInfoArrayRaw) as *mut c_void;
            let mut slap_info: *mut c_void = ours_ptr;
            let mut buffers: [*mut c_void; 4] = [std::ptr::null_mut(); 4];
            let mut widths = [0i32; 4];
            let mut heights = [0i32; 4];

            let result = (self.api.segment4)(
                image.pixels.as_ptr() as *const c_void,
                image.width,
                image.height,
                slap_type,
                num_of_fingers,
                &mut slap_info,
                &mut buffers[0], &mut widths[0], &mut heights[0],
                &mut buffers[1], &mut widths[1], &mut heights[1],
                &mut buffers[2], &mut widths[2], &mut heights[2],
                &mut buffers[3], &mut widths[3], &mut heights[3],
            );

            if !slap_info.is_null() {
                let arr = std::ptr::read_unaligned(slap_info as *const SlapInfoArrayRaw);
                for i in 0..4 {
                    let raw = arr.a[i];
                    seg.infos[i].finger_type = raw.finger_type;
                    seg.infos[i].image_quality = raw.image_quality;
                    seg.infos[i].rotation = raw.rotation;
                    for k in 0..4 {
                        seg.infos[i].finger_position[k] =
                            RsPoint { x: raw.finger_position[k].x, y: raw.finger_position[k].y };
                    }
                }
            }
            for i in 0..4 {
                if !buffers[i].is_null() {
                    seg.images[i] = Some(copy_grey(&self.api, buffers[i], widths[i], heights[i]));
                    (self.api.free_image_data)(buffers[i]);
                }
            }
            if !slap_info.is_null() && slap_info != ours_ptr {
                (self.api.free_image_data)(slap_info);
            }
            (result, seg)
        }
    }

    fn encode_wsq(&mut self, image: &GreyImage, ratio: f32, wsq_buffer: &mut [u8], len: &mut i32) -> i32 {
        unsafe {
            (self.api.encode_wsq)(
                image.pixels.as_ptr() as *const c_void,
                image.width,
                image.height,
                ratio,
                wsq_buffer.as_mut_ptr(),
                len,
            )
        }
    }

    fn add_overlay_quadrangle(&mut self, corners: [RsPoint; 4], color: u64, overlay_handle: &mut i32) -> i32 {
        unsafe {
            let mut quad = OverlayQuadRaw {
                pos: [
                    PointRaw { x: corners[0].x, y: corners[0].y },
                    PointRaw { x: corners[1].x, y: corners[1].y },
                    PointRaw { x: corners[2].x, y: corners[2].y },
                    PointRaw { x: corners[3].x, y: corners[3].y },
                ],
                color,
                width: 0,
                reserved: [0; 16],
            };
            (self.api.add_overlay_quadrangle)(self.handle, &mut quad, overlay_handle)
        }
    }

    fn show_overlay(&mut self, overlay_handle: i32, show: bool) -> i32 {
        unsafe { (self.api.show_overlay)(overlay_handle, show as i32) }
    }

    fn calibrate(&mut self) -> i32 {
        unsafe { (self.api.calibrate)(self.handle) }
    }
}

impl Drop for RealScan {
    fn drop(&mut self) {
        // Mirror RealScanDevice.Dispose: abort, stop preview, exit the device (no
        // RS_ExitAllDevices — the SDK stays loaded for the next session).
        if self.handle != 0 {
            unsafe {
                (self.api.abort_capture)(self.handle);
                if self.preview_on {
                    (self.api.stop_view_window)(self.handle);
                }
                (self.api.exit_device)(self.handle);
            }
            self.handle = 0;
        }
    }
}

/// Copy an 8-bit greyscale SDK buffer into a managed [`GreyImage`] (one byte/pixel).
///
/// # Safety
/// `data` must be a live `w*h`-byte buffer from the SDK.
unsafe fn copy_grey(_api: &Api, data: *mut c_void, w: i32, h: i32) -> GreyImage {
    let len = (w.max(0) as usize) * (h.max(0) as usize);
    let mut pixels = vec![0u8; len];
    if len > 0 {
        std::ptr::copy_nonoverlapping(data as *const u8, pixels.as_mut_ptr(), len);
    }
    GreyImage { pixels, width: w, height: h }
}
