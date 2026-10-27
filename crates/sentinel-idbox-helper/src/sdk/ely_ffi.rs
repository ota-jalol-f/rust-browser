//! Native FFI to the x86 Elyctis chain (`ElyMRTD.dll` + `ElyRdrControl.dll`), from the
//! signatures confirmed in `docs/2026-10-27-elymrtd-ffi-spec.md` (no vendor header
//! needed — `ElyMRTDdotNet.dll` is a pure-IL P/Invoke wrapper, so its metadata gave the
//! native entry names, parameter shapes, calling convention and CharSet).
//!
//! ABI (see the spec): every free function is `stdcall`; the text getters are UTF-16
//! (`wchar_t*` buffer + `u32*` length, pre-sized — the two-call pattern), `connect` /
//! `establishBAC` take ANSI strings, and `bool` returns are Win32 `BOOL` (treat `!= 0`).
//! The DLLs are x86, so this module only compiles for `target_arch = "x86"`.
//!
//! A native fault (e.g. an access violation inside the SDK with no card) is **not**
//! caught here — it crashes the helper, the plugin sees the pipe close and reports a
//! fault, and the completion falls back to the MRZ, exactly the reference's end state.
//!
//! Runtime-verify against a real ID BOX: the two-call buffer sizing, `init()`'s
//! logging/licensing, the DG2 image format, and the BAC/BAP branch.

#![allow(unsafe_code)]

use std::ffi::CString;
use std::os::raw::{c_char, c_uchar};
use std::path::PathBuf;

use libloading::{Library, Symbol};
use sentinel_idbox_core::chip::ElyMrtd;
use sentinel_idbox_core::helper::ChipSdk;

use crate::decoder::PassthroughDecoder;

type VoidFn = unsafe extern "stdcall" fn();
type AnsiToI32 = unsafe extern "stdcall" fn(*const c_char) -> i32;
type I32Fn = unsafe extern "stdcall" fn() -> i32;
type TextGetter = unsafe extern "stdcall" fn(*mut u16, *mut u32);
type CountryGetter = unsafe extern "stdcall" fn(*const c_uchar, *mut u16, *mut u32);
type BinGetter = unsafe extern "stdcall" fn(*mut c_uchar, *mut u32);

const TEXT_CAP: usize = 512;
const PHOTO_CAP: usize = 256 * 1024;

/// The x86 Elyctis SDK behind the [`ChipSdk`] seam.
pub struct RealChipSdk {
    ely_path: PathBuf,
    rdr: Library,
    decoder: PassthroughDecoder,
}

impl RealChipSdk {
    /// Load `ElyRdrControl.dll` (for `RDR_CloseComm`) and locate `ElyMRTD.dll` next to
    /// the helper. The chip handle itself is created per read in [`ChipSdk::create_ely`].
    pub fn load() -> Result<Self, String> {
        let dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from("."));
        let ely_path = dir.join("ElyMRTD.dll");
        let rdr_path = dir.join("ElyRdrControl.dll");
        // SAFETY: loading a vendor DLL from the helper's own folder.
        let rdr = unsafe { Library::new(&rdr_path) }.map_err(|e| format!("ElyRdrControl.dll: {e}"))?;
        if !ely_path.exists() {
            return Err(format!("ElyMRTD.dll not found: {}", ely_path.display()));
        }
        Ok(RealChipSdk { ely_path, rdr, decoder: PassthroughDecoder })
    }
}

impl ChipSdk for RealChipSdk {
    fn create_ely(&mut self) -> Result<Box<dyn ElyMrtd>, String> {
        // The SDK opens %LOCALAPPDATA%\elyMRTD\Log.txt and throws if the folder is
        // missing; create it before init(), as the reference does.
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            let _ = std::fs::create_dir_all(PathBuf::from(local).join("elyMRTD"));
        }
        // SAFETY: the vendor DLL at the path we validated in `load`.
        let lib = unsafe { Library::new(&self.ely_path) }.map_err(|e| format!("ElyMRTD.dll: {e}"))?;
        let ely = FfiEly { lib };
        ely.call_void(b"init\0");
        Ok(Box::new(ely))
    }

    fn init_reader_manager(&mut self) -> Result<(), String> {
        // The native connect() establishes its own SCard context; nothing to do here.
        Ok(())
    }

    fn close_comm(&mut self) -> Result<(), String> {
        // SAFETY: RDR_CloseComm takes no arguments.
        unsafe {
            let f: Symbol<VoidFn> = self.rdr.get(b"RDR_CloseComm\0").map_err(|e| e.to_string())?;
            f();
        }
        Ok(())
    }

    fn photo_decoder(&self) -> &dyn sentinel_idbox_core::chip::PhotoDecoder {
        &self.decoder
    }
}

/// One loaded `ElyMRTD.dll`, calling its native entries. Functions are looked up by name
/// per call (the read is a handful of calls, so the cost is irrelevant).
struct FfiEly {
    lib: Library,
}

impl FfiEly {
    fn call_void(&self, name: &[u8]) {
        // SAFETY: `name` is a documented no-arg stdcall export.
        unsafe {
            if let Ok(f) = self.lib.get::<VoidFn>(name) {
                f();
            }
        }
    }

    fn call_i32(&self, name: &[u8]) -> i32 {
        // SAFETY: `name` is a documented `() -> i32` stdcall export.
        unsafe {
            match self.lib.get::<I32Fn>(name) {
                Ok(f) => f(),
                Err(_) => -1,
            }
        }
    }

    fn call_ansi_i32(&self, name: &[u8], arg: &str) -> i32 {
        let c = match CString::new(arg) {
            Ok(c) => c,
            Err(_) => return -1,
        };
        // SAFETY: `name` is a documented `(char* ANSI) -> i32` stdcall export; `c` lives
        // across the call.
        unsafe {
            match self.lib.get::<AnsiToI32>(name) {
                Ok(f) => f(c.as_ptr()),
                Err(_) => -1,
            }
        }
    }

    fn text(&self, name: &[u8]) -> String {
        let mut buf = [0u16; TEXT_CAP];
        let mut len = buf.len() as u32;
        // SAFETY: `name` is a documented `(wchar_t* buf, u32* len)` stdcall getter; the
        // buffer and length outlive the call and `len` is read back as the count.
        unsafe {
            if let Ok(f) = self.lib.get::<TextGetter>(name) {
                f(buf.as_mut_ptr(), &mut len);
            }
        }
        let n = (len as usize).min(buf.len());
        String::from_utf16_lossy(&buf[..n])
    }
}

impl ElyMrtd for FfiEly {
    fn connect(&mut self, reader_name: &str) -> Result<i32, String> {
        Ok(self.call_ansi_i32(b"connect\0", reader_name))
    }

    fn get_select_antenna(&mut self) -> Result<Vec<u8>, String> {
        let mut buf = [0u8; 16];
        let mut len = buf.len() as u32;
        // SAFETY: documented `(u8* data, u32* len)` stdcall getter.
        unsafe {
            if let Ok(f) = self.lib.get::<BinGetter>(b"getSelectAntenna\0") {
                f(buf.as_mut_ptr(), &mut len);
            }
        }
        let n = (len as usize).min(buf.len());
        Ok(buf[..n].to_vec())
    }

    fn establish_bac(&mut self, mrz_info: &str) -> Result<bool, String> {
        Ok(self.call_ansi_i32(b"establishBAC\0", mrz_info) != 0)
    }

    fn read_dg1(&mut self) -> Result<i32, String> {
        Ok(self.call_i32(b"readDG1\0"))
    }
    fn read_dg2(&mut self) -> Result<i32, String> {
        Ok(self.call_i32(b"readDG2\0"))
    }
    fn read_dg6(&mut self) -> Result<i32, String> {
        Ok(self.call_i32(b"readDG6\0"))
    }

    fn get_name(&mut self) -> String {
        self.text(b"getName\0")
    }
    fn get_surname(&mut self) -> String {
        self.text(b"getSurname\0")
    }
    fn get_birth_date(&mut self) -> String {
        self.text(b"getBirthDate\0")
    }
    fn get_nationality(&mut self) -> String {
        self.text(b"getNationality\0")
    }
    fn get_issuing_state(&mut self) -> String {
        self.text(b"getIssuingState\0")
    }
    fn get_country_name(&mut self, code: &str) -> String {
        let c = match CString::new(code) {
            Ok(c) => c,
            Err(_) => return String::new(),
        };
        let mut buf = [0u16; TEXT_CAP];
        let mut len = buf.len() as u32;
        // SAFETY: documented `getCountryNameL(u8* code, wchar_t* data, u32* len)`.
        unsafe {
            if let Ok(f) = self.lib.get::<CountryGetter>(b"getCountryNameL\0") {
                f(c.as_ptr() as *const c_uchar, buf.as_mut_ptr(), &mut len);
            }
        }
        let n = (len as usize).min(buf.len());
        String::from_utf16_lossy(&buf[..n])
    }
    fn get_sex(&mut self) -> String {
        self.text(b"getSex\0")
    }
    fn get_validity_date(&mut self) -> String {
        self.text(b"getValidityDate\0")
    }
    fn get_doc_num(&mut self) -> String {
        self.text(b"getDocNum\0")
    }
    fn get_document_type(&mut self) -> String {
        self.text(b"getDocumentType\0")
    }
    fn get_optional_data(&mut self) -> String {
        self.text(b"getOptionalData\0")
    }

    fn get_family_name(&mut self) -> String {
        self.text(b"getFamilyName\0")
    }
    fn get_given_names(&mut self) -> String {
        self.text(b"getGivenNames\0")
    }
    fn get_expiry_date(&mut self) -> String {
        self.text(b"getExpiryDate\0")
    }
    fn get_issuing_date(&mut self) -> String {
        self.text(b"getIssuingDate\0")
    }
    fn get_issuing_authority(&mut self) -> String {
        self.text(b"getIssuingAuthority\0")
    }
    fn get_issuing_country(&mut self) -> String {
        self.text(b"getIssuingCountry\0")
    }
    fn get_licence_number(&mut self) -> String {
        self.text(b"getLicenceNumber\0")
    }

    fn get_categories(&mut self) -> Result<Option<Vec<String>>, String> {
        // IDl only, and the reference's CMRZ throws for a driving licence regardless, so
        // the categories' contents are never used. A non-null list keeps the IDl branch
        // on the same (failing) path as C#. Parsing the native category buffer is left
        // for IDl support.
        Ok(Some(Vec::new()))
    }

    fn get_photo(&mut self) -> Result<Option<Vec<u8>>, String> {
        let mut buf = vec![0u8; PHOTO_CAP];
        let mut len = buf.len() as u32;
        // SAFETY: documented `getPhoto(u8* buf, u32* len)`; the buffer outlives the call.
        unsafe {
            if let Ok(f) = self.lib.get::<BinGetter>(b"getPhoto\0") {
                f(buf.as_mut_ptr(), &mut len);
            }
        }
        let n = (len as usize).min(buf.len());
        if n == 0 {
            Ok(None)
        } else {
            buf.truncate(n);
            Ok(Some(buf))
        }
    }

    fn disconnect(&mut self) -> Result<(), String> {
        self.call_void(b"disconnect\0");
        Ok(())
    }
}
