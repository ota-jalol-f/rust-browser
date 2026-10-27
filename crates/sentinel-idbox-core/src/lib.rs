//! `sentinel-idbox-core`: the device-free heart of the Elyctis ID BOX reader.
//!
//! A faithful Rust port of the reference `Asbt.IdBox.Plugin` + `Asbt.IdBox.ChipHelper`
//! (pv-winform `FormReader` / `ClassMRZ` / `Scanner`, extracted and pinned). It holds
//! everything that does **not** touch hardware, an OS service, or the vendor SDK, so
//! both the x64 plugin and the x86/i686 chip helper can share it and every host can
//! build and test it:
//!
//! - [`checksum`] — the managed replacement for Ely's x86 `calculateChecksum`.
//! - [`mrz`] — [`mrz::MrzLines`]: split the scanner text and derive `MRZ_info` (the BAC seed).
//! - [`reader_result`] — [`reader_result::ReaderResult`] + [`reader_result::ReaderResultBuilder`]
//!   (`CMRZ`: the chip and MRZ-fallback field logic, quirks included).
//! - [`answere`] — the two Answere JSON texts (`ToPluginData` / `ToCSharpJson`), byte-for-byte.
//! - [`texts`] — every user-visible status/alert string, verbatim.
//! - [`protocol`] — the helper wire protocol ([`protocol::HelperRequest`] / [`protocol::HelperMessage`]).
//! - [`chip`] — the [`chip::ElyMrtd`] seam + [`chip::ChipReadJob`] (the streaming chip read).
//! - [`completion`] — [`completion::complete`]: the C# three-way outcome into plugin-data JSON.
//!
//! The pinning fixture `tests/fixtures/reference.json` is the very file the C# test
//! suite uses, produced on .NET Framework (x86) from the real binaries.

#![forbid(unsafe_code)]
#![warn(missing_docs)]
// The MRZ parser mirrors pv-winform's sequential `.Replace(a).Replace(b)` chains
// verbatim; collapsing them (as clippy suggests) would diverge from the C# semantics
// the fixture pins, so keep them as written.
#![allow(clippy::collapsible_str_replace)]

pub mod answere;
pub mod checksum;
pub mod chip;
pub mod completion;
pub mod errortext;
pub mod mrz;
pub mod protocol;
pub mod reader_result;
pub mod texts;

pub use answere::{to_csharp_json, to_plugin_data};
pub use chip::{ChipReadJob, ElyMrtd, PhotoDecoder};
pub use completion::{complete, IdBoxIcon, IconState, View};
pub use mrz::MrzLines;
pub use protocol::{HelperMessage, HelperRequest, MessageType};
pub use reader_result::{MrzError, ReaderResult, ReaderResultBuilder, ReadingType};
