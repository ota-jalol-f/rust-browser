//! The real Canon EDSDK camera (`CanonCamera` + `Edsdk` in the reference), ported
//! to Rust FFI over `EDSDK.dll` via `libloading`. x64 Windows `device` builds only.
//!
//! EDSDK is not thread safe and delivers its object/state events through the
//! message pump of the thread that called `EdsInitializeSDK` — here the UI thread,
//! the same thread that polls the overlay. The two native callbacks push
//! [`CameraEvent`]s into a queue the session drains in `pump`.
//!
//! RUNTIME-VERIFY ON A REAL ID-BOX/CANON (no hardware or DLL here, so this is
//! compile-checked only — design §8 of docs/photo-document-plugin.md):
//! - event delivery without an explicit `EdsGetEvent` (the reference relied on the
//!   WPF pump; if events don't arrive, call `EdsGetEvent` each `poll`);
//! - the `EdsDirectoryItemInfo` layout and the two-stream download/live-view sizing;
//! - the x64 EDSDK version vs the production body.
//!
//! Nothing may unwind across the FFI boundary, so both callbacks run their body in
//! `catch_unwind`.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::ffi::c_void;
use std::os::raw::c_char;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;

use libloading::{Library, Symbol};
use sentinel_photo_core::camera::{Camera, CameraError, CameraEvent};

// ---- EDSDK constants (Edsdk.cs) -------------------------------------------

const EDS_ERR_OK: u32 = 0x0000_0000;
const PROP_ID_SAVE_TO: u32 = 0x0000_000b;
const PROP_ID_EVF_OUTPUT_DEVICE: u32 = 0x0000_0500;
const SAVE_TO_HOST: u32 = 2;
const EVF_OUTPUT_DEVICE_OFF: u32 = 0;
const EVF_OUTPUT_DEVICE_PC: u32 = 2;
const CAMERA_COMMAND_TAKE_PICTURE: u32 = 0x0000_0000;
const OBJECT_EVENT_ALL: u32 = 0x0000_0200;
const OBJECT_EVENT_DIR_ITEM_REQUEST_TRANSFER: u32 = 0x0000_0208;
const STATE_EVENT_ALL: u32 = 0x0000_0300;
const STATE_EVENT_SHUTDOWN: u32 = 0x0000_0301;

#[repr(C)]
#[derive(Clone, Copy)]
struct EdsCapacity {
    number_of_free_clusters: i32,
    bytes_per_sector: i32,
    reset: i32,
}

#[repr(C)]
struct EdsDirectoryItemInfo {
    size: u32,
    is_folder: i32,
    group_id: u32,
    option: u32,
    file_name: [c_char; 256],
    format: u32,
    date_time: u32,
}

type ObjectEventHandler = unsafe extern "system" fn(u32, *mut c_void, *mut c_void) -> u32;
type StateEventHandler = unsafe extern "system" fn(u32, u32, *mut c_void) -> u32;

// Native signatures (all `stdcall`/`system`, returning the EDSDK error code).
type FnVoidU32 = unsafe extern "system" fn() -> u32;
type FnPtrU32 = unsafe extern "system" fn(*mut c_void) -> u32;
type FnOutPtr = unsafe extern "system" fn(*mut *mut c_void) -> u32;
type FnChildCount = unsafe extern "system" fn(*mut c_void, *mut i32) -> u32;
type FnChildAt = unsafe extern "system" fn(*mut c_void, i32, *mut *mut c_void) -> u32;
type FnSetProp = unsafe extern "system" fn(*mut c_void, u32, i32, i32, *const u32) -> u32;
type FnSetCapacity = unsafe extern "system" fn(*mut c_void, EdsCapacity) -> u32;
type FnSendCommand = unsafe extern "system" fn(*mut c_void, u32, i32) -> u32;
type FnSetObjHandler = unsafe extern "system" fn(*mut c_void, u32, ObjectEventHandler, *mut c_void) -> u32;
type FnSetStateHandler = unsafe extern "system" fn(*mut c_void, u32, StateEventHandler, *mut c_void) -> u32;
type FnCreateMemStream = unsafe extern "system" fn(u64, *mut *mut c_void) -> u32;
type FnCreateEvfImageRef = unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> u32;
type FnDownloadEvf = unsafe extern "system" fn(*mut c_void, *mut c_void) -> u32;
type FnDirItemInfo = unsafe extern "system" fn(*mut c_void, *mut EdsDirectoryItemInfo) -> u32;
type FnDownload = unsafe extern "system" fn(*mut c_void, u64, *mut c_void) -> u32;
type FnGetPointer = unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> u32;
type FnGetLength = unsafe extern "system" fn(*mut c_void, *mut u64) -> u32;

/// The resolved EDSDK entry points (valid while [`RealCanonCamera::_lib`] is loaded).
#[derive(Clone, Copy)]
struct Api {
    initialize_sdk: FnVoidU32,
    terminate_sdk: FnVoidU32,
    release: FnPtrU32,
    get_camera_list: FnOutPtr,
    get_child_count: FnChildCount,
    get_child_at_index: FnChildAt,
    open_session: FnPtrU32,
    close_session: FnPtrU32,
    set_property_data: FnSetProp,
    set_capacity: FnSetCapacity,
    send_command: FnSendCommand,
    set_object_event_handler: FnSetObjHandler,
    set_camera_state_event_handler: FnSetStateHandler,
    create_memory_stream: FnCreateMemStream,
    create_evf_image_ref: FnCreateEvfImageRef,
    download_evf_image: FnDownloadEvf,
    get_directory_item_info: FnDirItemInfo,
    download: FnDownload,
    download_complete: FnPtrU32,
    get_pointer: FnGetPointer,
    get_length: FnGetLength,
}

/// State shared with the native callbacks (passed to EDSDK as the event context):
/// the event queue and the entry points the object callback needs to download.
struct Shared {
    events: RefCell<VecDeque<CameraEvent>>,
    api: Api,
}

/// A session with the first connected Canon camera.
pub struct RealCanonCamera {
    _lib: Library,
    api: Api,
    shared: Box<Shared>,
    object_handler: ObjectEventHandler,
    state_handler: StateEventHandler,
    camera: *mut c_void,
    sdk_initialized: bool,
    session_open: bool,
    live_on: bool,
}

impl RealCanonCamera {
    /// Load `EDSDK.dll` from next to the executable and resolve its entry points.
    /// `Err` (→ the stub camera) when the DLL is absent or a symbol is missing.
    pub fn load() -> Result<Self, String> {
        let dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from("."));
        let path = dir.join("EDSDK.dll");
        if !path.exists() {
            return Err(format!("EDSDK.dll not found: {}", path.display()));
        }
        // SAFETY: loading the vendor DLL from the executable's own folder.
        let lib = unsafe { Library::new(&path) }.map_err(|e| e.to_string())?;

        // SAFETY: each symbol is resolved to its documented signature; the fn
        // pointers stay valid while `lib` is held by the returned struct.
        let api = unsafe {
            macro_rules! sym {
                ($name:literal, $ty:ty) => {{
                    let s: Symbol<$ty> = lib.get($name).map_err(|e| e.to_string())?;
                    *s
                }};
            }
            Api {
                initialize_sdk: sym!(b"EdsInitializeSDK\0", FnVoidU32),
                terminate_sdk: sym!(b"EdsTerminateSDK\0", FnVoidU32),
                release: sym!(b"EdsRelease\0", FnPtrU32),
                get_camera_list: sym!(b"EdsGetCameraList\0", FnOutPtr),
                get_child_count: sym!(b"EdsGetChildCount\0", FnChildCount),
                get_child_at_index: sym!(b"EdsGetChildAtIndex\0", FnChildAt),
                open_session: sym!(b"EdsOpenSession\0", FnPtrU32),
                close_session: sym!(b"EdsCloseSession\0", FnPtrU32),
                set_property_data: sym!(b"EdsSetPropertyData\0", FnSetProp),
                set_capacity: sym!(b"EdsSetCapacity\0", FnSetCapacity),
                send_command: sym!(b"EdsSendCommand\0", FnSendCommand),
                set_object_event_handler: sym!(b"EdsSetObjectEventHandler\0", FnSetObjHandler),
                set_camera_state_event_handler: sym!(b"EdsSetCameraStateEventHandler\0", FnSetStateHandler),
                create_memory_stream: sym!(b"EdsCreateMemoryStream\0", FnCreateMemStream),
                create_evf_image_ref: sym!(b"EdsCreateEvfImageRef\0", FnCreateEvfImageRef),
                download_evf_image: sym!(b"EdsDownloadEvfImage\0", FnDownloadEvf),
                get_directory_item_info: sym!(b"EdsGetDirectoryItemInfo\0", FnDirItemInfo),
                download: sym!(b"EdsDownload\0", FnDownload),
                download_complete: sym!(b"EdsDownloadComplete\0", FnPtrU32),
                get_pointer: sym!(b"EdsGetPointer\0", FnGetPointer),
                get_length: sym!(b"EdsGetLength\0", FnGetLength),
            }
        };

        Ok(RealCanonCamera {
            _lib: lib,
            api,
            shared: Box::new(Shared { events: RefCell::new(VecDeque::new()), api }),
            object_handler: on_object_event,
            state_handler: on_state_event,
            camera: std::ptr::null_mut(),
            sdk_initialized: false,
            session_open: false,
            live_on: false,
        })
    }

    fn ctx(&self) -> *mut c_void {
        (self.shared.as_ref() as *const Shared) as *mut c_void
    }

    fn release_camera_ref(&mut self) {
        if !self.camera.is_null() {
            // SAFETY: a camera ref we own.
            unsafe { (self.api.release)(self.camera) };
            self.camera = std::ptr::null_mut();
        }
    }

    fn set_evf_output_device(&mut self, device: u32) -> Result<(), CameraError> {
        if !self.session_open {
            return Err(CameraError::Other("Фотокамера уланмаган".into()));
        }
        // SAFETY: a 4-byte uint property on the open camera.
        let err = unsafe {
            (self.api.set_property_data)(self.camera, PROP_ID_EVF_OUTPUT_DEVICE, 0, 4, &device)
        };
        check(err, "EdsSetPropertyData(Evf_OutputDevice)")
    }
}

impl Camera for RealCanonCamera {
    fn initialize(&mut self) -> Result<(), CameraError> {
        if self.sdk_initialized {
            return Ok(());
        }
        // SAFETY: no-arg EDSDK entry.
        check(unsafe { (self.api.initialize_sdk)() }, "EdsInitializeSDK")?;
        self.sdk_initialized = true;
        Ok(())
    }

    fn open_session(&mut self) -> Result<bool, CameraError> {
        if !self.sdk_initialized {
            return Err(CameraError::Other("EDSDK is not initialized".into()));
        }
        if self.session_open {
            return Ok(true);
        }
        self.release_camera_ref();

        // SAFETY: the standard camera-list walk; refs are released below.
        unsafe {
            let mut list: *mut c_void = std::ptr::null_mut();
            check((self.api.get_camera_list)(&mut list), "EdsGetCameraList")?;
            let mut count = 0i32;
            let count_err = (self.api.get_child_count)(list, &mut count);
            if let Err(e) = check(count_err, "EdsGetChildCount") {
                (self.api.release)(list);
                return Err(e);
            }
            if count == 0 {
                (self.api.release)(list);
                return Ok(false);
            }
            let at_err = (self.api.get_child_at_index)(list, 0, &mut self.camera);
            (self.api.release)(list);
            check(at_err, "EdsGetChildAtIndex")?;

            check((self.api.open_session)(self.camera), "EdsOpenSession")?;
            self.session_open = true;

            check(
                (self.api.set_object_event_handler)(
                    self.camera,
                    OBJECT_EVENT_ALL,
                    self.object_handler,
                    self.ctx(),
                ),
                "EdsSetObjectEventHandler",
            )?;
            check(
                (self.api.set_camera_state_event_handler)(
                    self.camera,
                    STATE_EVENT_ALL,
                    self.state_handler,
                    self.ctx(),
                ),
                "EdsSetCameraStateEventHandler",
            )?;
        }
        Ok(true)
    }

    fn close_session(&mut self) -> Result<(), CameraError> {
        if !self.session_open {
            return Ok(());
        }
        self.session_open = false;
        self.live_on = false;
        // SAFETY: closing the open session.
        check(unsafe { (self.api.close_session)(self.camera) }, "EdsCloseSession")
    }

    fn prepare_host_capture(&mut self) -> Result<(), CameraError> {
        if !self.session_open {
            return Err(CameraError::Other("Фотокамера уланмаган".into()));
        }
        let save_to = SAVE_TO_HOST;
        // SAFETY: SaveTo=Host, then a free-space capacity, on the open camera.
        unsafe {
            check(
                (self.api.set_property_data)(self.camera, PROP_ID_SAVE_TO, 0, 4, &save_to),
                "EdsSetPropertyData(SaveTo)",
            )?;
            let capacity = EdsCapacity { number_of_free_clusters: 0x7FFF_FFFF, bytes_per_sector: 0x1000, reset: 1 };
            check((self.api.set_capacity)(self.camera, capacity), "EdsSetCapacity")?;
        }
        Ok(())
    }

    fn start_live_view(&mut self) -> Result<(), CameraError> {
        self.set_evf_output_device(EVF_OUTPUT_DEVICE_PC)?;
        self.live_on = true;
        Ok(())
    }

    fn stop_live_view(&mut self) -> Result<(), CameraError> {
        self.set_evf_output_device(EVF_OUTPUT_DEVICE_OFF)?;
        self.live_on = false;
        Ok(())
    }

    fn get_live_view_frame(&mut self) -> Result<Option<Vec<u8>>, CameraError> {
        if !self.live_on {
            return Ok(None);
        }
        // SAFETY: the EVF download path; every ref is released. Any non-OK code
        // means "no frame ready" → None, exactly as the reference returned null.
        unsafe {
            let mut stream: *mut c_void = std::ptr::null_mut();
            let mut evf: *mut c_void = std::ptr::null_mut();
            let mut frame = None;
            if (self.api.create_memory_stream)(0, &mut stream) == EDS_ERR_OK
                && (self.api.create_evf_image_ref)(stream, &mut evf) == EDS_ERR_OK
                && (self.api.download_evf_image)(self.camera, evf) == EDS_ERR_OK
            {
                frame = read_stream(&self.api, stream);
            }
            if !evf.is_null() {
                (self.api.release)(evf);
            }
            if !stream.is_null() {
                (self.api.release)(stream);
            }
            Ok(frame)
        }
    }

    fn take_photo(&mut self) -> Result<(), CameraError> {
        if !self.session_open {
            return Err(CameraError::Other("Фотокамера уланмаган".into()));
        }
        // SAFETY: the TakePicture command; the file arrives via the object callback.
        check(
            unsafe { (self.api.send_command)(self.camera, CAMERA_COMMAND_TAKE_PICTURE, 0) },
            "EdsSendCommand(TakePicture)",
        )
    }

    fn release_camera(&mut self) -> Result<(), CameraError> {
        if self.live_on {
            let _ = self.stop_live_view();
        }
        if self.session_open {
            self.session_open = false;
            self.live_on = false;
            // SAFETY: closing the session before releasing the ref.
            unsafe { (self.api.close_session)(self.camera) };
        }
        self.release_camera_ref();
        Ok(())
    }

    fn dispose(&mut self) -> Result<(), CameraError> {
        if self.live_on {
            let _ = self.stop_live_view();
        }
        // SAFETY: full teardown; each handle is released once.
        unsafe {
            if self.session_open {
                (self.api.close_session)(self.camera);
                self.session_open = false;
            }
            self.release_camera_ref();
            if self.sdk_initialized {
                (self.api.terminate_sdk)();
                self.sdk_initialized = false;
            }
        }
        Ok(())
    }

    fn is_session_open(&self) -> bool {
        self.session_open
    }

    fn is_live_view_on(&self) -> bool {
        self.live_on
    }

    fn poll_event(&mut self) -> Option<CameraEvent> {
        self.shared.events.borrow_mut().pop_front()
    }
}

impl Drop for RealCanonCamera {
    fn drop(&mut self) {
        // The overlay normally calls Close (→ dispose); guard against a leak if not.
        let _ = self.dispose();
    }
}

/// Map an EDSDK error code to a [`CameraError`] (`Ok` on `EDS_ERR_OK`).
fn check(error: u32, call: &str) -> Result<(), CameraError> {
    if error == EDS_ERR_OK {
        Ok(())
    } else {
        Err(CameraError::Other(format!("{call}: EDSDK error 0x{error:08X}")))
    }
}

/// Read a native EDSDK memory stream's bytes, or `None` when empty/unavailable.
///
/// # Safety
/// `stream` must be a live EDSDK stream ref from `api`.
unsafe fn read_stream(api: &Api, stream: *mut c_void) -> Option<Vec<u8>> {
    let mut length = 0u64;
    if (api.get_length)(stream, &mut length) != EDS_ERR_OK || length == 0 {
        return None;
    }
    let mut pointer: *mut c_void = std::ptr::null_mut();
    if (api.get_pointer)(stream, &mut pointer) != EDS_ERR_OK || pointer.is_null() {
        return None;
    }
    let len = usize::try_from(length).ok()?;
    let mut data = vec![0u8; len];
    std::ptr::copy_nonoverlapping(pointer as *const u8, data.as_mut_ptr(), len);
    Some(data)
}

/// Download a captured directory item to JPEG bytes.
///
/// # Safety
/// `dir_item` must be the directory-item ref delivered by the object event.
unsafe fn download(api: &Api, dir_item: *mut c_void) -> Result<Vec<u8>, String> {
    let mut info: EdsDirectoryItemInfo = std::mem::zeroed();
    if (api.get_directory_item_info)(dir_item, &mut info) != EDS_ERR_OK {
        return Err("EdsGetDirectoryItemInfo failed".into());
    }
    let mut stream: *mut c_void = std::ptr::null_mut();
    let result = (|| {
        if (api.create_memory_stream)(u64::from(info.size), &mut stream) != EDS_ERR_OK {
            return Err("EdsCreateMemoryStream failed".to_string());
        }
        if (api.download)(dir_item, u64::from(info.size), stream) != EDS_ERR_OK {
            return Err("EdsDownload failed".to_string());
        }
        if (api.download_complete)(dir_item) != EDS_ERR_OK {
            return Err("EdsDownloadComplete failed".to_string());
        }
        read_stream(api, stream).ok_or_else(|| "Сурат бўш".to_string())
    })();
    if !stream.is_null() {
        (api.release)(stream);
    }
    result
}

/// The object-event callback: on a transfer request, download the shot and push a
/// [`CameraEvent`]. Never unwinds into native code.
unsafe extern "system" fn on_object_event(event: u32, object_ref: *mut c_void, context: *mut c_void) -> u32 {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if context.is_null() {
            return;
        }
        let shared = &*(context as *const Shared);
        if event != OBJECT_EVENT_DIR_ITEM_REQUEST_TRANSFER {
            if !object_ref.is_null() {
                (shared.api.release)(object_ref);
            }
            return;
        }
        let event = match download(&shared.api, object_ref) {
            Ok(bytes) => CameraEvent::PhotoDownloaded(bytes),
            Err(message) => CameraEvent::DownloadFailed(message),
        };
        shared.events.borrow_mut().push_back(event);
        if !object_ref.is_null() {
            (shared.api.release)(object_ref);
        }
    }));
    EDS_ERR_OK
}

/// The state-event callback: queue a [`CameraEvent::Shutdown`] on power-off. Never
/// unwinds into native code.
unsafe extern "system" fn on_state_event(event: u32, _event_data: u32, context: *mut c_void) -> u32 {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if event == STATE_EVENT_SHUTDOWN && !context.is_null() {
            let shared = &*(context as *const Shared);
            shared.events.borrow_mut().push_back(CameraEvent::Shutdown);
        }
    }));
    EDS_ERR_OK
}
