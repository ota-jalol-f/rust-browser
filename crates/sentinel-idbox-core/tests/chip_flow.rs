//! `ChipReadJob` + `ChipCompletion` against fake SDKs — the Rust counterparts of
//! `ChipReadJobTests` and `ChipCompletionTests`.

use std::collections::HashMap;

use sentinel_idbox_core::chip::{ChipReadJob, ElyMrtd, PhotoDecoder};
use sentinel_idbox_core::completion::{complete, CompletionError, IdBoxIcon, IconState, Outcome, View};
use sentinel_idbox_core::mrz::MrzLines;
use sentinel_idbox_core::protocol::{HelperMessage, MessageType};
use sentinel_idbox_core::reader_result::{MrzError, ReaderResultBuilder};
use sentinel_idbox_core::texts;

const READER: &str = "ELYCTIS CL reader 0123456789ABCDEF 0";
const INFO: &str = "AA1234567890010113001019";
const TD3: &str = "P<UZBTOSHMATOV<<ALISHER<<<<<<<<<<<<<<<<<<<<<\r\nAA12345678UZB9001011M30010193123456789012324\r\n";
const TD1: &str = "IDUZBAD1234567741508850123456<\r\n8508155F3208140UZB<<<<<<<<<<<6\r\nRASULOVA<<MALIKA<<<<<<<<<<<<<<\r\n";

// ---- fakes --------------------------------------------------------------------------

struct FakeEly {
    calls: Vec<String>,
    connect_result: i32,
    connect_throws: Option<String>,
    antenna: Vec<u8>,
    antenna_throws: Option<String>,
    bac: bool,
    dg1_result: i32,
    dg2_result: i32,
    dg6_result: i32,
    photo: Option<Vec<u8>>,
    categories: Option<Vec<String>>,
    disconnect_throws: Option<String>,
}

impl Default for FakeEly {
    fn default() -> Self {
        FakeEly {
            calls: Vec::new(),
            connect_result: 0,
            connect_throws: None,
            antenna: vec![0],
            antenna_throws: None,
            bac: true,
            dg1_result: 0,
            dg2_result: 0,
            dg6_result: 0,
            photo: Some(vec![0xFF, 0xD8, 0xFF, 0xD9]),
            categories: Some(Vec::new()),
            disconnect_throws: None,
        }
    }
}

impl ElyMrtd for FakeEly {
    fn connect(&mut self, reader_name: &str) -> Result<i32, String> {
        self.calls.push(format!("connect:{reader_name}"));
        match &self.connect_throws {
            Some(e) => Err(e.clone()),
            None => Ok(self.connect_result),
        }
    }
    fn get_select_antenna(&mut self) -> Result<Vec<u8>, String> {
        self.calls.push("getSelectAntenna".into());
        match &self.antenna_throws {
            Some(e) => Err(e.clone()),
            None => Ok(self.antenna.clone()),
        }
    }
    fn establish_bac(&mut self, mrz_info: &str) -> Result<bool, String> {
        self.calls.push(format!("establishBAC:{mrz_info}"));
        Ok(self.bac)
    }
    fn read_dg1(&mut self) -> Result<i32, String> {
        self.calls.push("readDG1".into());
        Ok(self.dg1_result)
    }
    fn read_dg2(&mut self) -> Result<i32, String> {
        self.calls.push("readDG2".into());
        Ok(self.dg2_result)
    }
    fn read_dg6(&mut self) -> Result<i32, String> {
        self.calls.push("readDG6".into());
        Ok(self.dg6_result)
    }
    fn get_name(&mut self) -> String { "ALISHER".into() }
    fn get_surname(&mut self) -> String { "TOSHMATOV".into() }
    fn get_birth_date(&mut self) -> String { "01/01/90".into() }
    fn get_nationality(&mut self) -> String { "UZB".into() }
    fn get_issuing_state(&mut self) -> String { "UZB".into() }
    fn get_country_name(&mut self, code: &str) -> String {
        if code == "UZB" { "Uzbekistan".into() } else { code.into() }
    }
    fn get_sex(&mut self) -> String { "M".into() }
    fn get_validity_date(&mut self) -> String { "01/01/30".into() }
    fn get_doc_num(&mut self) -> String { "AA1234567".into() }
    fn get_document_type(&mut self) -> String { "P".into() }
    fn get_optional_data(&mut self) -> String { "31234567890123".into() }
    fn get_family_name(&mut self) -> String { "TOSHMATOV".into() }
    fn get_given_names(&mut self) -> String { "ALISHER".into() }
    fn get_expiry_date(&mut self) -> String { "01/01/30".into() }
    fn get_issuing_date(&mut self) -> String { "01/01/20".into() }
    fn get_issuing_authority(&mut self) -> String { "IIV".into() }
    fn get_issuing_country(&mut self) -> String { "UZB".into() }
    fn get_licence_number(&mut self) -> String { "AF1234567".into() }
    fn get_categories(&mut self) -> Result<Option<Vec<String>>, String> {
        Ok(self.categories.clone())
    }
    fn get_photo(&mut self) -> Result<Option<Vec<u8>>, String> {
        self.calls.push("getPhoto".into());
        Ok(self.photo.clone())
    }
    fn disconnect(&mut self) -> Result<(), String> {
        self.calls.push("disconnect".into());
        match &self.disconnect_throws {
            Some(e) => Err(e.clone()),
            None => Ok(()),
        }
    }
}

#[derive(Default)]
struct FakeDecoder {
    decode_fails: bool,
    encode_fails: bool,
}

impl PhotoDecoder for FakeDecoder {
    fn decode(&self, photo: &[u8]) -> Option<Vec<u8>> {
        if self.decode_fails {
            None
        } else {
            Some(photo.to_vec())
        }
    }
    fn to_jpeg_base64(&self, decoded: &[u8]) -> Option<String> {
        if self.encode_fails {
            None
        } else {
            Some(base64(decoded))
        }
    }
}

fn base64(data: &[u8]) -> String {
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in data.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        out.push(T[(n >> 18 & 63) as usize] as char);
        out.push(T[(n >> 12 & 63) as usize] as char);
        out.push(if chunk.len() > 1 { T[(n >> 6 & 63) as usize] as char } else { '=' });
        out.push(if chunk.len() > 2 { T[(n & 63) as usize] as char } else { '=' });
    }
    out
}

struct Ran {
    result: HelperMessage,
    statuses: Vec<String>,
    alerts: Vec<HelperMessage>,
    calls: Vec<String>,
}

fn run(mut ely: FakeEly, decoder: FakeDecoder) -> Ran {
    let mut sent: Vec<HelperMessage> = Vec::new();
    let result = {
        let mut push = |m: HelperMessage| sent.push(m);
        ChipReadJob::new(&mut ely, &decoder, 7, &mut push).run(READER, INFO)
    };
    let statuses = sent
        .iter()
        .filter(|m| m.type_ == MessageType::STATUS)
        .map(|m| m.text.clone().unwrap_or_default())
        .collect();
    let alerts = sent.iter().filter(|m| m.type_ == MessageType::ALERT).cloned().collect();
    Ran { result, statuses, alerts, calls: ely.calls }
}

// ---- ChipReadJob --------------------------------------------------------------------

#[test]
fn icao_happy_statuses_dg1_and_photo() {
    let r = run(FakeEly::default(), FakeDecoder::default());
    assert_eq!(
        r.statuses,
        vec![
            texts::CONNECTING, texts::CONNECTED, texts::FRONT_ANTENNA, texts::BAC_CONNECTING, texts::BAC_CONNECTED,
            texts::DG1_READING, texts::DG1_READ, texts::DG2_READING, texts::DG2_READ, texts::DISCONNECTED,
        ]
    );
    assert_eq!(r.alerts.len(), 0);
    assert_eq!(r.result.type_, MessageType::RESULT);
    assert_eq!(r.result.id, 7);
    assert_eq!(r.result.code, Some(0));
    assert_eq!(r.result.applet_type, Some(0));
    assert_eq!(
        r.result.dg1.clone().unwrap().join("|"),
        "ALISHER|TOSHMATOV|01/01/90|Uzbekistan|M|01/01/30|AA1234567|P|Uzbekistan|31234567890123"
    );
    assert_eq!(r.result.photo.as_deref(), Some("/9j/2Q=="));
    assert_eq!(r.calls.first().unwrap(), &format!("connect:{READER}"));
    assert!(r.calls.contains(&format!("establishBAC:{INFO}")));
    assert_eq!(r.calls.last().unwrap(), "disconnect");
}

#[test]
fn connect_minus_one_e_result_null_no_disconnected_status() {
    let r = run(FakeEly { connect_result: -1, ..Default::default() }, FakeDecoder::default());
    assert_eq!(r.statuses, vec![texts::CONNECTING, texts::APPLET_ERROR]);
    assert_eq!(r.result.code, None);
    assert_eq!(r.result.fault, None);
    assert!(!r.calls.contains(&"disconnect".to_string()));
}

#[test]
fn connect_throws_toast_then_continues_as_icao() {
    let ely = FakeEly {
        connect_throws: Some("External component has thrown an exception.".into()),
        ..Default::default()
    };
    let r = run(ely, FakeDecoder::default());
    assert_eq!(r.alerts.len(), 1);
    assert_eq!(r.alerts[0].caption.as_deref(), Some(texts::CONNECT_ERROR));
    assert_eq!(r.alerts[0].text.as_deref(), Some("External component has thrown an exception.\r\n"));
    assert_eq!(r.statuses[1], texts::CONNECTED);
    assert_eq!(r.result.code, Some(0));
}

#[test]
fn antenna_throws_minus_one_with_disconnect() {
    let r = run(FakeEly { antenna_throws: Some("antenna".into()), ..Default::default() }, FakeDecoder::default());
    assert_eq!(r.result.code, Some(-1));
    assert_eq!(r.result.message.as_deref(), Some("antenna"));
    assert_eq!(r.statuses.last().unwrap(), texts::DISCONNECTED);
    assert_eq!(r.calls.last().unwrap(), "disconnect");
}

#[test]
fn back_and_unknown_antenna() {
    let back = run(FakeEly { antenna: vec![1], ..Default::default() }, FakeDecoder::default());
    let none = run(FakeEly { antenna: vec![2], ..Default::default() }, FakeDecoder::default());
    assert!(back.statuses.contains(&texts::BACK_ANTENNA.to_string()));
    assert!(!none.statuses.contains(&texts::FRONT_ANTENNA.to_string()));
    assert!(!none.statuses.contains(&texts::BACK_ANTENNA.to_string()));
}

#[test]
fn bac_fails_then_dg1_fails_minus_two() {
    let r = run(FakeEly { bac: false, dg1_result: -1, ..Default::default() }, FakeDecoder::default());
    assert!(r.statuses.contains(&texts::BAC_ERROR.to_string()));
    assert!(r.statuses.contains(&texts::DG1_ERROR.to_string()));
    assert_eq!(r.result.code, Some(-2));
    assert_eq!(r.result.message.as_deref(), Some(texts::DG1_NOT_READ));
    assert_eq!(r.result.dg1.unwrap().len(), 0);
}

#[test]
fn dg2_fails_is_still_read_without_photo() {
    let r = run(FakeEly { dg2_result: -1, ..Default::default() }, FakeDecoder::default());
    assert!(r.statuses.contains(&texts::DG2_ERROR.to_string()));
    assert_eq!(r.result.code, Some(0));
    assert_eq!(r.result.photo, None);
}

#[test]
fn no_photo_bytes_no_dg2_read_status() {
    let r = run(FakeEly { photo: None, ..Default::default() }, FakeDecoder::default());
    assert!(!r.statuses.contains(&texts::DG2_READ.to_string()));
    assert_eq!(r.result.code, Some(0));
    assert_eq!(r.result.photo, None);
}

#[test]
fn photo_decode_fails_minus_two_but_dg1_complete() {
    let r = run(FakeEly::default(), FakeDecoder { decode_fails: true, ..Default::default() });
    assert!(r.statuses.contains(&texts::DG2_PHOTO_ERROR.to_string()));
    assert_eq!(r.result.code, Some(-2));
    assert_eq!(r.result.message.as_deref(), Some(texts::DG2_PHOTO_ERROR));
    assert_eq!(r.result.dg1.unwrap().len(), 10);
}

#[test]
fn photo_encode_fails_photo_error_flag() {
    let r = run(FakeEly::default(), FakeDecoder { encode_fails: true, ..Default::default() });
    assert_eq!(r.result.code, Some(0));
    assert_eq!(r.result.photo_error, Some(true));
}

#[test]
fn idl_bap_statuses_eight_values_dg6_photo() {
    let r = run(FakeEly { connect_result: 1, ..Default::default() }, FakeDecoder::default());
    assert_eq!(
        r.statuses,
        vec![
            texts::CONNECTING, texts::CONNECTED, texts::FRONT_ANTENNA, texts::BAP_CONNECTING, texts::BAP_CONNECTED,
            texts::DG1_READING, texts::DG1_READ, texts::DG6_READING, texts::DG6_READ, texts::DISCONNECTED,
        ]
    );
    assert_eq!(r.result.applet_type, Some(1));
    assert_eq!(r.result.dg1.unwrap().len(), 8);
    assert_eq!(r.result.categories_read, Some(true));
    assert!(r.result.photo.is_some());
    assert!(!r.calls.contains(&"readDG2".to_string()));
}

#[test]
fn idl_null_categories_minus_two() {
    let r = run(FakeEly { connect_result: 1, categories: None, ..Default::default() }, FakeDecoder::default());
    assert_eq!(r.result.code, Some(-2));
    assert_eq!(r.result.categories_read, Some(false));
}

#[test]
fn idl_dg6_photo_fails_the_k_spelling() {
    let r = run(FakeEly { connect_result: 1, ..Default::default() }, FakeDecoder { decode_fails: true, ..Default::default() });
    assert!(r.statuses.contains(&texts::DG6_PHOTO_ERROR.to_string()));
    assert_eq!(r.result.message.as_deref(), Some(texts::DG6_PHOTO_ERROR));
}

#[test]
fn unknown_applet_e_result_null_but_disconnected() {
    let r = run(FakeEly { connect_result: 2, ..Default::default() }, FakeDecoder::default());
    assert_eq!(r.result.code, None);
    assert_eq!(r.statuses.last().unwrap(), texts::DISCONNECTED);
    assert!(!r.calls.contains(&format!("establishBAC:{INFO}")));
}

#[test]
fn disconnect_throws_is_a_fault() {
    let r = run(FakeEly { disconnect_throws: Some("disconnect failed".into()), ..Default::default() }, FakeDecoder::default());
    assert_eq!(r.result.fault.as_deref(), Some("disconnect failed"));
    assert_eq!(r.result.code, None);
}

// ---- ChipCompletion -----------------------------------------------------------------

#[derive(Default)]
struct FakeView {
    events: Vec<String>,
    icons: HashMap<IdBoxIcon, IconState>,
    status: String,
    status_red: bool,
    ok_json: Option<String>,
    cancelled: bool,
}

impl View for FakeView {
    fn set_status(&mut self, text: &str) {
        self.status = text.to_string();
        self.events.push(format!("status:{text}"));
    }
    fn set_status_error(&mut self, text: &str) {
        self.status_red = true;
        self.status = text.to_string();
        self.events.push(format!("error:{text}"));
    }
    fn set_icon(&mut self, icon: IdBoxIcon, state: IconState) {
        self.icons.insert(icon, state);
    }
    fn set_mrz_lines(&mut self, _l1: &str, _l2: &str) {}
    fn show_toast(&mut self, caption: &str, _text: &str) {
        self.events.push(format!("toast:{caption}"));
    }
    fn show_error_message(&mut self, text: &str, _caption: &str) {
        self.events.push(format!("message:{text}"));
    }
    fn close_ok(&mut self, data_json: &str) {
        self.ok_json = Some(data_json.to_string());
    }
    fn close_cancel(&mut self) {
        self.cancelled = true;
    }
}

fn builder() -> ReaderResultBuilder {
    ReaderResultBuilder::new(2026)
}

fn dg1() -> Vec<String> {
    ["ALISHER", "TOSHMATOV", "12/08/74", "Uzbekistan", "M", "15/04/32", "AA1234567", "P", "Uzbekistan", "31208740000000"]
        .iter()
        .map(|s| s.to_string())
        .collect()
}

fn ok_json(o: Outcome) -> String {
    match o {
        Outcome::Ok(j) => j,
        Outcome::StayOpen => panic!("expected Ok, got StayOpen"),
    }
}

#[test]
fn completion_null_result_is_the_mrz_fallback() {
    let mrz = MrzLines::new(TD3).unwrap();
    let mut view = FakeView::default();
    let out = complete(&HelperMessage::result(0), &mrz, 1.5, &builder(), &mut view).unwrap();
    let data: serde_json::Value = serde_json::from_str(&ok_json(out)).unwrap();
    assert_eq!(data["AnswereComment"].as_str(), Some(texts::FROM_MRZ));
    assert_eq!(data["Data"]["DocumentReadingTime"].as_f64(), Some(1.5));
    assert_eq!(view.events.len(), 0);
}

#[test]
fn completion_icao_ten_values_is_the_chip_result_with_tick() {
    let mrz = MrzLines::new(TD3).unwrap();
    let mut view = FakeView::default();
    let mut res = HelperMessage::result(0);
    res.code = Some(0);
    res.applet_type = Some(0);
    res.dg1 = Some(dg1());
    res.photo = Some("AAAA".into());
    let data: serde_json::Value = serde_json::from_str(&ok_json(complete(&res, &mrz, 1.5, &builder(), &mut view).unwrap())).unwrap();

    assert_eq!(data["AnswereComment"].as_str(), Some(texts::FROM_CHIP));
    assert_eq!(data["Data"]["BirthDate"].as_str(), Some("12.08.1974"));
    assert_eq!(data["Data"]["ExpiryDate"].as_str(), Some("15.04.2032"));
    assert_eq!(data["Data"]["Nationality"].as_str(), Some("Uzbekistan"));
    assert_eq!(data["Data"]["Pinpp"].as_str(), Some("31208740000000"));
    assert_eq!(data["Data"]["Additional"].as_str(), Some(""));
    assert!(data["Data"]["Citizenship"].is_null());
    assert_eq!(data["Data"]["Line3"].as_str(), Some(""));
    assert_eq!(data["Data"]["PersonPhoto"].as_str(), Some("AAAA"));
    assert_eq!(view.icons[&IdBoxIcon::ChipRead], IconState::Tick);
}

#[test]
fn completion_minus_two_with_ten_values_still_ok() {
    let mrz = MrzLines::new(TD3).unwrap();
    let mut view = FakeView::default();
    let mut res = HelperMessage::result(0);
    res.code = Some(-2);
    res.message = Some(texts::DG2_PHOTO_ERROR.into());
    res.applet_type = Some(0);
    res.dg1 = Some(dg1());
    let _ = ok_json(complete(&res, &mrz, 1.5, &builder(), &mut view).unwrap());
    assert_eq!(view.events[0], format!("status:{}", texts::DG2_PHOTO_ERROR));
    assert!(!view.status_red);
}

#[test]
fn completion_photo_error_red_but_ok() {
    let mrz = MrzLines::new(TD3).unwrap();
    let mut view = FakeView::default();
    let mut res = HelperMessage::result(0);
    res.code = Some(0);
    res.applet_type = Some(0);
    res.dg1 = Some(dg1());
    res.photo_error = Some(true);
    let _ = ok_json(complete(&res, &mrz, 1.5, &builder(), &mut view).unwrap());
    assert!(view.events.contains(&format!("error:{}", texts::PHOTO_CLONE_ERROR)));
}

#[test]
fn completion_minus_two_incomplete_dg1_stays_open_red() {
    let mrz = MrzLines::new(TD3).unwrap();
    let mut view = FakeView::default();
    let mut res = HelperMessage::result(0);
    res.code = Some(-2);
    res.message = Some(texts::DG1_NOT_READ.into());
    res.applet_type = Some(0);
    res.dg1 = Some(Vec::new());
    assert_eq!(complete(&res, &mrz, 1.5, &builder(), &mut view).unwrap(), Outcome::StayOpen);
    assert_eq!(view.status, texts::DG1_NOT_READ);
    assert!(view.status_red);
    assert_eq!(view.icons[&IdBoxIcon::ChipRead], IconState::Cross);
}

#[test]
fn completion_minus_one_stays_open_red() {
    let mrz = MrzLines::new(TD3).unwrap();
    let mut view = FakeView::default();
    let mut res = HelperMessage::result(0);
    res.code = Some(-1);
    res.message = Some("antenna".into());
    res.applet_type = Some(0);
    res.dg1 = Some(Vec::new());
    assert_eq!(complete(&res, &mrz, 1.5, &builder(), &mut view).unwrap(), Outcome::StayOpen);
    assert_eq!(view.status, "antenna");
    assert!(view.status_red);
}

#[test]
fn completion_fault_stays_open_red() {
    let mrz = MrzLines::new(TD3).unwrap();
    let mut view = FakeView::default();
    let mut res = HelperMessage::result(0);
    res.fault = Some("Asbt.IdBox.ChipHelper.exe exited (exit code 0xC0000005)".into());
    assert_eq!(complete(&res, &mrz, 1.0, &builder(), &mut view).unwrap(), Outcome::StayOpen);
    assert!(view.status.contains("0xC0000005"));
}

#[test]
fn completion_idl_always_throws_like_csharp() {
    let mrz = MrzLines::new(TD3).unwrap();

    let mut with = HelperMessage::result(0);
    with.code = Some(0);
    with.applet_type = Some(1);
    with.dg1 = Some(vec!["a", "b", "c", "d", "e", "f", "g", "h"].into_iter().map(String::from).collect());
    with.categories_read = Some(true);
    let mut view = FakeView::default();
    // ICAO Box fields stay "" for a driving licence, so CMRZ fails on Pinpp[0].
    assert_eq!(complete(&with, &mrz, 1.0, &builder(), &mut view), Err(CompletionError::Mrz(MrzError::IndexOutOfRange)));
    assert_eq!(view.icons[&IdBoxIcon::ChipRead], IconState::Tick);

    let mut without = HelperMessage::result(0);
    without.code = Some(0);
    without.applet_type = Some(1);
    without.dg1 = Some(Vec::new());
    without.categories_read = Some(false);
    assert_eq!(
        complete(&without, &mrz, 1.0, &builder(), &mut FakeView::default()),
        Err(CompletionError::NullReference)
    );
}

#[test]
fn completion_td1_fallback_throws_like_csharp() {
    let td1 = MrzLines::new(TD1).unwrap();
    let mut view = FakeView::default();
    assert_eq!(
        complete(&HelperMessage::result(0), &td1, 1.0, &builder(), &mut view),
        Err(CompletionError::Mrz(MrzError::ArgumentOutOfRange))
    );
}
