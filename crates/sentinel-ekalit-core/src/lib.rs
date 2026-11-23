//! `sentinel-ekalit-core`: the device-free heart of the `Asbt.eKalit` plugin (the
//! `ekalit` host object — E-IMZO / E-Kalit USB-token signing), ported from the
//! reference `Asbt.EKalitWebV6.Plugin`.
//!
//! - [`canonicalize`] — `SignStringV2`'s "sum string" (the exact bytes that get
//!   signed), pinned to real Newtonsoft output for string/int/bool/object/array.
//! - [`answere`] — the `Answere` JSON envelope (`AnswereId`/`AnswereMessage`/
//!   `AnswereComment`/`Data`, nulls omitted).
//! - [`models`] — `ModelSignature` / `ModelCertificate` / `ModelStringSign`.
//! - [`cert`] — X.509 certificate parsing (`GetCertificateInfo`).
//! - [`texts`] — the user-visible strings.
//! - [`signer`] — the E-Kalit token seam (`Signer`), driven by the plugin.
//! - [`base64`] — standard base64.
//!
//! The E-Kalit token itself is reached through the vendor's **managed** `libesigner.net`
//! wrapper (no native API in the source, like SignPad's Topaz), so the real signing
//! backend needs the vendor native SDK; the shipped build uses a stub signer.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod answere;
pub mod base64;
pub mod canonicalize;
pub mod cert;
pub mod models;
pub mod signer;
pub mod texts;

pub use answere::answere_json;
pub use canonicalize::sign_string_v2;
pub use cert::parse_certificate;
pub use models::{ModelCertificate, ModelSignature};
pub use signer::{SignOutcome, Signer};
