//! The no-modal `ReadDocument` orchestration: the plugin-side (x64) flow of pv-winform
//! `FormReader`, run **synchronously** for a single `idbox.ReadDocument()` plugin call.
//!
//! The reference opened a modal window and drove the read from serial `DataReceived`
//! events on the UI thread. By deployment requirement there is no window: the read runs
//! start-to-finish inside the call, streaming status through [`View`] (which the plugin
//! forwards to the page / log), and returns the Answere envelope. The device seams
//! ([`Scanner`], [`CardDetector`], [`ChipReader`]) are faked in tests and real behind the
//! `device` feature.

use std::time::Instant;

use sentinel_idbox_core::completion::{complete, IconState, IdBoxIcon, Outcome, View};
use sentinel_idbox_core::mrz::MrzLines;
use sentinel_idbox_core::protocol::HelperMessage;
use sentinel_idbox_core::reader_result::ReaderResultBuilder;
use sentinel_idbox_core::texts as t;

use crate::serial::Scanner;

/// How long to wait for the ID BOX to deliver the MRZ after `Inquire`.
pub const MRZ_READ_TIMEOUT_MS: i32 = 10_000;
const STATUS_CHANGE_TIMEOUT_MS: i32 = 200;

/// The PC/SC card state (`SCardGetStatusChange`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardState {
    /// `SCARD_E_TIMEOUT`.
    Timeout,
    /// The card is not present.
    Absent,
    /// The card is present.
    Present,
}

/// The PC/SC part of `selectAntenna` (`SCardManager.ListReaders` / `GetStatusChange`).
pub trait CardDetector {
    /// `EstablishContext`. `Err` with the SDK-shaped message on failure.
    fn open(&mut self) -> Result<(), String>;
    /// The connected readers.
    fn list_readers(&mut self) -> Result<Vec<String>, String>;
    /// `GetStatusChange(timeout)`.
    fn get_state(&mut self, reader: &str, timeout_ms: i32) -> Result<CardState, String>;
}

/// What the SDK start reported.
#[derive(Debug, Clone, Default)]
pub struct ChipReaderStart {
    /// The chip can be read (the helper runs and the SDK was created).
    pub available: bool,
    /// Why not (logged, never shown).
    pub unavailable_reason: Option<String>,
    /// The SCard manager / antenna selector failed.
    pub reader_init_error: Option<String>,
}

/// Callbacks of a chip read (status line / toast).
pub trait ChipEvents {
    /// A status line (`isError` → red).
    fn status(&mut self, text: &str, is_error: bool);
    /// A toast.
    fn alert(&mut self, caption: &str, text: &str);
}

/// The chip half of `FormReader` behind one seam: the real x86 SDK out of process
/// (`HelperChipReader`, `device` feature), a fake in tests.
pub trait ChipReader {
    /// The SDK start. Blocks; never panics.
    fn start(&mut self) -> ChipReaderStart;
    /// `RDR_CloseComm()`; fire and forget.
    fn close_comm(&mut self);
    /// The chip read. Blocks; never panics; returns the `result` message.
    fn read(&mut self, reader: &str, mrz_info: &str, events: &mut dyn ChipEvents) -> HelperMessage;
    /// Stop a running read.
    fn abort(&mut self);
}

/// Every device the read touches, behind one factory (real or fake).
pub trait Devices {
    /// A fresh MRZ scanner.
    fn create_scanner(&self) -> Box<dyn Scanner>;
    /// The Elyctis COM port, or `Err` when the lookup itself fails.
    fn find_port(&self) -> Result<String, String>;
    /// A fresh PC/SC card detector.
    fn create_card_detector(&self) -> Box<dyn CardDetector>;
    /// A fresh chip reader (starts the x86 helper in production).
    fn create_chip_reader(&self) -> Box<dyn ChipReader>;
}

/// The outcome of a `ReadDocument` call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadOutcome {
    /// Success: the Answere plugin-data JSON (`PluginResult.data`).
    Data(String),
    /// No data — the front-end may retry (`success=false`, empty `error_message`).
    Cancel,
}

/// Run the whole read for one `idbox.ReadDocument()` call. `view` receives the status
/// stream; `log` receives diagnostics; `builder` is the `CMRZ` builder (its year).
pub fn read_document(
    devices: &dyn Devices,
    view: &mut dyn View,
    builder: &ReaderResultBuilder,
    log: &mut dyn FnMut(&str),
) -> ReadOutcome {
    let start = Instant::now();
    let mut scanner = devices.create_scanner();
    let mut chip = devices.create_chip_reader();
    let mut detector = devices.create_card_detector();

    // --- Start (SDK, reader manager, port) — FormReader ctor + Load. ---
    let begin = chip.start();
    if let Some(e) = &begin.reader_init_error {
        log(&format!("Asbt.IdBox: reader init error: {e}"));
        view.show_toast(t::READER_NOT_CONNECTED, e);
    }
    let _ = detector.open(); // a failure surfaces at selectAntenna, as in C#
    if !begin.available {
        log(&format!(
            "Asbt.IdBox: chip reading unavailable ({}); every read falls back to the MRZ",
            begin.unavailable_reason.clone().unwrap_or_default()
        ));
    }

    let port = match devices.find_port() {
        Ok(p) => p,
        Err(e) => {
            log(&format!("Asbt.IdBox: port lookup failed: {e}"));
            view.show_toast(t::DEVICE_NOT_FOUND, t::PORT_LOOKUP_ERROR);
            return ReadOutcome::Cancel;
        }
    };

    // --- ReadClick ---
    clear_icons(view);
    if !scanner.connect(&port) {
        log(&format!("Asbt.IdBox: ReadClick -> {} {}", t::PORT_ERROR_DETAIL, port));
        view.show_error_message(t::DEVICE_NOT_FOUND, t::ERROR_CAPTION);
        return ReadOutcome::Cancel;
    }
    scanner.set_enable_reading(false);
    view.set_status(t::READING_MRZ);
    scanner.inquire();
    chip.close_comm();

    // --- read the MRZ (synchronous; C# waited for the serial DataReceived event) ---
    let mrz_text = scanner.read_mrz(MRZ_READ_TIMEOUT_MS);

    // --- readMRZ ---
    clear_icons(view);
    let mrz = match MrzLines::new(&mrz_text) {
        Ok(m) => m,
        Err(_) => {
            set_all_cross(view);
            view.show_toast(t::DOCUMENT_NOT_PLACED, t::DOCUMENT_NOT_PLACED_DETAIL);
            return ReadOutcome::Cancel;
        }
    };
    view.set_mrz_lines(mrz.line1(), mrz.line2());
    if mrz.checksum() {
        view.set_status(t::MRZ_READ_OK);
        view.set_icon(IdBoxIcon::MrzDetected, IconState::Tick);
        view.set_icon(IdBoxIcon::MrzRead, IconState::Tick);
    } else if mrz.mrz_info().map(|i| i.len()) == Some(24) {
        view.set_status_error(t::MRZ_CHECKSUM_ERROR);
        view.set_icon(IdBoxIcon::MrzDetected, IconState::Tick);
        view.set_icon(IdBoxIcon::MrzRead, IconState::Caution);
    } else {
        // C#: a short line throws -> the "document not placed" warning.
        set_all_cross(view);
        view.show_toast(t::DOCUMENT_NOT_PLACED, t::DOCUMENT_NOT_PLACED_DETAIL);
        return ReadOutcome::Cancel;
    }

    // --- readDocument / selectAntenna ---
    view.set_status(t::DETECTING_CHIP);
    let scard_reader = match select_antenna(detector.as_mut(), view, log) {
        Some(r) => r,
        None => return ReadOutcome::Cancel, // antenna failed: C# leaves the window open
    };

    // --- the chip read + completion ---
    view.set_status(t::READING_DOCUMENT);
    let mrz_info = mrz.mrz_info().unwrap_or("").to_string();
    let result = {
        let mut events = ViewEvents { view, log };
        chip.read(&scard_reader, &mrz_info, &mut events)
    };

    let elapsed = start.elapsed().as_secs_f64();
    match complete(&result, &mrz, elapsed, builder, view) {
        Ok(Outcome::Ok(json)) => {
            scanner.disconnect();
            ReadOutcome::Data(json)
        }
        Ok(Outcome::StayOpen) => ReadOutcome::Cancel,
        Err(e) => {
            // C# catches the completion throw, shows red, leaves the window open.
            log(&format!("Asbt.IdBox: completion failed: {e:?}"));
            ReadOutcome::Cancel
        }
    }
}

/// `FormReader.selectAntenna`: the first contactless reader with a present card, or
/// `None` after a red status (C# leaves the window open).
fn select_antenna(detector: &mut dyn CardDetector, view: &mut dyn View, log: &mut dyn FnMut(&str)) -> Option<String> {
    let readers = match detector.list_readers() {
        Ok(r) => r,
        Err(e) => return antenna_error(view, log, &e),
    };

    let scard = readers.into_iter().find(|r| is_contactless_reader(r));
    let scard = match scard {
        Some(r) => r,
        None => return antenna_error(view, log, t::CONNECT_ELYCTIS_READER),
    };

    match detector.get_state(&scard, STATUS_CHANGE_TIMEOUT_MS) {
        Ok(CardState::Present) => {
            view.set_icon(IdBoxIcon::AntennaSelected, IconState::Tick);
            Some(scard)
        }
        Ok(CardState::Timeout) => antenna_error(view, log, t::CARD_TIMEOUT),
        Ok(CardState::Absent) => antenna_error(view, log, t::CARD_NOT_FOUND),
        Err(e) => antenna_error(view, log, &e),
    }
}

fn antenna_error(view: &mut dyn View, log: &mut dyn FnMut(&str), message: &str) -> Option<String> {
    log(&format!("Asbt.IdBox: selectAntenna -> {message}"));
    view.set_status_error(message);
    view.set_icon(IdBoxIcon::AntennaSelected, IconState::Cross);
    None
}

/// `FormReader.cs:69` regex `eader [A-Z0-9]{16} [0-9]{1}`, matched anywhere in the name.
fn is_contactless_reader(name: &str) -> bool {
    let b = name.as_bytes();
    let needle = b"eader ";
    for start in 0..b.len().saturating_sub(needle.len()) {
        if &b[start..start + needle.len()] != needle {
            continue;
        }
        let p = start + needle.len();
        if p + 18 <= b.len()
            && b[p..p + 16].iter().all(|&c| c.is_ascii_uppercase() || c.is_ascii_digit())
            && b[p + 16] == b' '
            && b[p + 17].is_ascii_digit()
        {
            return true;
        }
    }
    false
}

fn clear_icons(view: &mut dyn View) {
    for icon in [IdBoxIcon::MrzDetected, IdBoxIcon::MrzRead, IdBoxIcon::ChipRead, IdBoxIcon::AntennaSelected] {
        view.set_icon(icon, IconState::Loading);
    }
    view.set_status("");
}

fn set_all_cross(view: &mut dyn View) {
    for icon in [IdBoxIcon::MrzDetected, IdBoxIcon::MrzRead, IdBoxIcon::AntennaSelected, IdBoxIcon::ChipRead] {
        view.set_icon(icon, IconState::Cross);
    }
}

/// Bridges a chip read's `status`/`alert` callbacks onto the [`View`] + log.
struct ViewEvents<'a> {
    view: &'a mut dyn View,
    log: &'a mut dyn FnMut(&str),
}

impl ChipEvents for ViewEvents<'_> {
    fn status(&mut self, text: &str, is_error: bool) {
        if is_error {
            self.view.set_status_error(text);
        } else {
            self.view.set_status(text);
        }
    }
    fn alert(&mut self, caption: &str, text: &str) {
        (self.log)(&format!("Asbt.IdBox: {caption}: {text}"));
        self.view.show_toast(caption, text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::serial::Scanner;

    const TD3: &str = "P<UZBTOSHMATOV<<ALISHER<<<<<<<<<<<<<<<<<<<<<\r\nAA12345678UZB9001011M30010193123456789012324\r\n";
    const CONTACTLESS: &str = "ELYCTIS CL reader 0123456789ABCDEF 0";

    struct FakeScanner {
        connect_ok: bool,
        mrz: String,
    }
    impl Scanner for FakeScanner {
        fn connect(&mut self, _p: &str) -> bool {
            self.connect_ok
        }
        fn disconnect(&mut self) -> bool {
            true
        }
        fn is_connected(&self) -> bool {
            self.connect_ok
        }
        fn set_enable_reading(&mut self, _v: bool) {}
        fn inquire(&mut self) {}
        fn read_mrz(&mut self, _t: i32) -> String {
            self.mrz.clone()
        }
    }

    struct FakeDetector {
        readers: Vec<String>,
        state: CardState,
    }
    impl CardDetector for FakeDetector {
        fn open(&mut self) -> Result<(), String> {
            Ok(())
        }
        fn list_readers(&mut self) -> Result<Vec<String>, String> {
            Ok(self.readers.clone())
        }
        fn get_state(&mut self, _r: &str, _t: i32) -> Result<CardState, String> {
            Ok(self.state)
        }
    }

    struct FakeChip {
        available: bool,
        result: HelperMessage,
    }
    impl ChipReader for FakeChip {
        fn start(&mut self) -> ChipReaderStart {
            ChipReaderStart { available: self.available, ..Default::default() }
        }
        fn close_comm(&mut self) {}
        fn read(&mut self, _r: &str, _i: &str, events: &mut dyn ChipEvents) -> HelperMessage {
            events.status(t::CONNECTING, false);
            events.status(t::DISCONNECTED, false);
            self.result.clone()
        }
        fn abort(&mut self) {}
    }

    struct FakeDevices {
        connect_ok: bool,
        mrz: String,
        port: Result<String, String>,
        readers: Vec<String>,
        state: CardState,
        chip_available: bool,
        chip_result: HelperMessage,
    }
    impl Default for FakeDevices {
        fn default() -> Self {
            FakeDevices {
                connect_ok: true,
                mrz: TD3.to_string(),
                port: Ok("COM7".to_string()),
                readers: vec![CONTACTLESS.to_string()],
                state: CardState::Present,
                chip_available: true,
                chip_result: happy_icao(),
            }
        }
    }
    impl Devices for FakeDevices {
        fn create_scanner(&self) -> Box<dyn Scanner> {
            Box::new(FakeScanner { connect_ok: self.connect_ok, mrz: self.mrz.clone() })
        }
        fn find_port(&self) -> Result<String, String> {
            self.port.clone()
        }
        fn create_card_detector(&self) -> Box<dyn CardDetector> {
            Box::new(FakeDetector { readers: self.readers.clone(), state: self.state })
        }
        fn create_chip_reader(&self) -> Box<dyn ChipReader> {
            Box::new(FakeChip { available: self.chip_available, result: self.chip_result.clone() })
        }
    }

    fn happy_icao() -> HelperMessage {
        let mut m = HelperMessage::result(1);
        m.code = Some(0);
        m.applet_type = Some(0);
        m.dg1 = Some(
            ["ALISHER", "TOSHMATOV", "12/08/74", "Uzbekistan", "M", "15/04/32", "AA1234567", "P", "Uzbekistan", "31208740000000"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
        );
        m.photo = Some("/9j/AAA=".to_string());
        m
    }

    /// A view that only needs to satisfy the trait for the flow.
    #[derive(Default)]
    struct NullView;
    impl View for NullView {
        fn set_status(&mut self, _t: &str) {}
        fn set_status_error(&mut self, _t: &str) {}
        fn set_icon(&mut self, _i: IdBoxIcon, _s: IconState) {}
        fn set_mrz_lines(&mut self, _l1: &str, _l2: &str) {}
        fn show_toast(&mut self, _c: &str, _t: &str) {}
        fn show_error_message(&mut self, _t: &str, _c: &str) {}
        fn close_ok(&mut self, _j: &str) {}
        fn close_cancel(&mut self) {}
    }

    fn run(devices: &FakeDevices) -> ReadOutcome {
        read_document(devices, &mut NullView, &ReaderResultBuilder::new(2026), &mut |_| {})
    }

    fn data(outcome: &ReadOutcome) -> serde_json::Value {
        match outcome {
            ReadOutcome::Data(j) => serde_json::from_str(j).unwrap(),
            ReadOutcome::Cancel => panic!("expected Data, got Cancel"),
        }
    }

    #[test]
    fn happy_chip_read_returns_the_chip_envelope() {
        let d = data(&run(&FakeDevices::default()));
        assert_eq!(d["AnswereComment"].as_str(), Some(t::FROM_CHIP));
        assert_eq!(d["Data"]["PersonPhoto"].as_str(), Some("/9j/AAA="));
        assert_eq!(d["Data"]["BirthDate"].as_str(), Some("12.08.1974"));
        assert_eq!(d["Data"]["InformationFrom"].as_i64(), Some(0));
    }

    #[test]
    fn no_port_cancels() {
        let devices = FakeDevices { port: Err("no ports".into()), ..Default::default() };
        assert_eq!(run(&devices), ReadOutcome::Cancel);
    }

    #[test]
    fn unreadable_mrz_cancels() {
        let devices = FakeDevices { mrz: String::new(), ..Default::default() };
        assert_eq!(run(&devices), ReadOutcome::Cancel);
    }

    #[test]
    fn no_contactless_reader_cancels() {
        let devices = FakeDevices { readers: vec!["Some Other Reader".into()], ..Default::default() };
        assert_eq!(run(&devices), ReadOutcome::Cancel);
    }

    #[test]
    fn card_absent_cancels() {
        let devices = FakeDevices { state: CardState::Absent, ..Default::default() };
        assert_eq!(run(&devices), ReadOutcome::Cancel);
    }

    #[test]
    fn chip_unavailable_with_reader_falls_back_to_the_mrz() {
        // The helper is down → the read returns the connect(-1) path (code null), so the
        // completion rebuilds from the MRZ: still a document, marked "from MRZ".
        let devices = FakeDevices {
            chip_available: false,
            chip_result: HelperMessage::result(1), // code None
            ..Default::default()
        };
        let d = data(&run(&devices));
        assert_eq!(d["AnswereComment"].as_str(), Some(t::FROM_MRZ));
        assert_eq!(d["Data"]["InformationFrom"].as_i64(), Some(1));
        assert_eq!(d["Data"]["DocumentNumber"].as_str(), Some("AA1234567"));
    }

    #[test]
    fn contactless_reader_regex_matches_like_csharp() {
        assert!(is_contactless_reader("ELYCTIS CL reader 0123456789ABCDEF 0"));
        assert!(!is_contactless_reader("Some Reader without the pattern"));
        assert!(!is_contactless_reader("reader 0123456789ABCDE 0")); // 15 hex, not 16
    }
}
