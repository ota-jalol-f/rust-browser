//! The [`ChipSdk`] the helper serves: the real x86 Elyctis FFI (`device` on x86), or a
//! stub that reports the chip unavailable (every other build), which makes the plugin
//! fall back to the MRZ — so even a stub helper is a working degraded mode.

use sentinel_idbox_core::chip::PhotoDecoder;
use sentinel_idbox_core::helper::ChipSdk;

use crate::decoder::PassthroughDecoder;

#[cfg(all(feature = "device", target_arch = "x86"))]
mod ely_ffi;

/// Build the SDK for this platform.
pub fn make_sdk() -> Box<dyn ChipSdk> {
    #[cfg(all(feature = "device", target_arch = "x86"))]
    {
        match ely_ffi::RealChipSdk::load() {
            Ok(sdk) => Box::new(sdk),
            Err(e) => Box::new(StubChipSdk::unavailable(e)),
        }
    }
    #[cfg(not(all(feature = "device", target_arch = "x86")))]
    {
        Box::new(StubChipSdk::unavailable(
            "ID BOX chip helper built without the `device` feature (or not x86)".to_string(),
        ))
    }
}

/// A SDK whose `create_ely` always fails: `hello.ok = false`, so the plugin reports the
/// chip unavailable and every read falls back to the MRZ.
pub struct StubChipSdk {
    reason: String,
    decoder: PassthroughDecoder,
}

impl StubChipSdk {
    fn unavailable(reason: String) -> Self {
        StubChipSdk { reason, decoder: PassthroughDecoder }
    }
}

impl ChipSdk for StubChipSdk {
    fn create_ely(&mut self) -> Result<Box<dyn sentinel_idbox_core::chip::ElyMrtd>, String> {
        Err(self.reason.clone())
    }
    fn init_reader_manager(&mut self) -> Result<(), String> {
        Ok(())
    }
    fn close_comm(&mut self) -> Result<(), String> {
        Ok(())
    }
    fn photo_decoder(&self) -> &dyn PhotoDecoder {
        &self.decoder
    }
}
