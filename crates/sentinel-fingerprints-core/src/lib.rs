//! `sentinel-fingerprints-core`: the device-free heart of the `Asbt.Fingerprints`
//! plugin's 10-finger enrollment (`fingerprints.Enroll`), ported from the reference
//! `Asbt.Fingerprints.Plugin` (pv-winform `FormFingerprintsV2`, Suprema RealScan).
//!
//! - [`finger`] — the finger enums + `EnrolledFinger` state (status, quality, WSQ).
//! - [`model_json`] — `ModelFingerprint` → JSON, byte-for-byte as Newtonsoft wrote
//!   it (what `Enroll` returns / "Файлга (json)" writes).
//! - [`wsqtrim`] — `WSQProcesses.RemoveNullElements` + the 18000-byte warning.
//! - [`options`] — the `Enroll(optionsJson)` options parse.
//! - [`dispatch`] — classify a request into Enroll / Scan.
//! - [`texts`] — the user-visible strings (ported byte-for-byte).
//! - [`base64`] — standard base64 (the WSQ bytes → `FingerN`).
//!
//! The RealScan device (RS_SDK FFI + WSQ encode + segmentation), the enrollment
//! window overlay and the plugin wiring are built on top of this in
//! `sentinel-fingerprints`.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod base64;
pub mod dispatch;
pub mod finger;
pub mod model_json;
pub mod options;
pub mod texts;
pub mod wsqtrim;

pub use dispatch::{classify, FingerprintsCall};
pub use finger::{EnrolledFinger, FingerPosition, FingerStatus, FingerType, GreyImage, TileBack};
pub use model_json::{build, serialize, Dto, IncompleteError};
pub use options::{EnrollOptions, OptionsError};
pub use wsqtrim::remove_null_elements;
