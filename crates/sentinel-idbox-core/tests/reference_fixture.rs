//! The Rust port pinned against `tests/fixtures/reference.json` — the very fixture the
//! C# suite uses, produced on .NET Framework (x86) from the real pv-winform binaries.
//! Mirrors `TextsAndChecksumTests`, `MrzTests` and `AnswereJsonTests`.

use serde_json::Value;

use sentinel_idbox_core::answere::{to_csharp_json, to_plugin_data};
use sentinel_idbox_core::checksum;
use sentinel_idbox_core::errortext;
use sentinel_idbox_core::mrz::MrzLines;
use sentinel_idbox_core::reader_result::{MrzError, ReaderResult, ReaderResultBuilder, ReadingType};
use sentinel_idbox_core::texts;

/// The year the fixture was generated in (`CMRZ`'s `DateTime.Today.Year`).
const GENERATED_YEAR: i32 = 2026;

const TD3_LINE1: &str = "P<UZBTOSHMATOV<<ALISHER<<<<<<<<<<<<<<<<<<<<<";
const TD3_LINE2: &str = "AA12345678UZB9001011M30010193123456789012324";

fn fixture() -> Value {
    let raw = include_str!("fixtures/reference.json");
    serde_json::from_str(raw).expect("reference.json parses")
}

fn array<'a>(root: &'a Value, name: &str) -> &'a Vec<Value> {
    root[name].as_array().unwrap_or_else(|| panic!("{name} is an array"))
}

// ---- texts & checksum ---------------------------------------------------------------

/// Every `texts::*` constant (name, value), the Rust counterpart of reflecting over
/// `IdBoxTexts`. `MRZ_LENGTH_INVALID_FORMAT` uses Rust's `{}`; the C# literal is the
/// interpolated `{mrz.Length}` form, handled in the test.
fn all_texts() -> Vec<(&'static str, &'static str)> {
    vec![
        ("WINDOW_TITLE", texts::WINDOW_TITLE),
        ("DESIGNER_TITLE", texts::DESIGNER_TITLE),
        ("INITIAL_STATUS", texts::INITIAL_STATUS),
        ("GROUP_READING", texts::GROUP_READING),
        ("LABEL_MRZ_DETECTED", texts::LABEL_MRZ_DETECTED),
        ("LABEL_MRZ_READ", texts::LABEL_MRZ_READ),
        ("LABEL_CHIP_DETECTED", texts::LABEL_CHIP_DETECTED),
        ("LABEL_CHIP_READ", texts::LABEL_CHIP_READ),
        ("GROUP_MRZ_LINES", texts::GROUP_MRZ_LINES),
        ("READER_NOT_CONNECTED", texts::READER_NOT_CONNECTED),
        ("DEVICE_NOT_FOUND", texts::DEVICE_NOT_FOUND),
        ("PORT_ERROR_DETAIL", texts::PORT_ERROR_DETAIL),
        ("ERROR_CAPTION", texts::ERROR_CAPTION),
        ("PORT_LOOKUP_ERROR", texts::PORT_LOOKUP_ERROR),
        ("PORT_FAULT", texts::PORT_FAULT),
        ("PORT_FAULT_DETAIL", texts::PORT_FAULT_DETAIL),
        ("DOCUMENT_NOT_PLACED", texts::DOCUMENT_NOT_PLACED),
        ("DOCUMENT_NOT_PLACED_DETAIL", texts::DOCUMENT_NOT_PLACED_DETAIL),
        ("CONNECT_ERROR", texts::CONNECT_ERROR),
        ("READER_DISCONNECT_ERROR", texts::READER_DISCONNECT_ERROR),
        ("READING_DONE", texts::READING_DONE),
        ("READING_MRZ_AGAIN", texts::READING_MRZ_AGAIN),
        ("READING_MRZ", texts::READING_MRZ),
        ("MRZ_READ_OK", texts::MRZ_READ_OK),
        ("MRZ_CHECKSUM_ERROR", texts::MRZ_CHECKSUM_ERROR),
        ("DETECTING_CHIP", texts::DETECTING_CHIP),
        ("CARD_TIMEOUT", texts::CARD_TIMEOUT),
        ("CARD_NOT_FOUND", texts::CARD_NOT_FOUND),
        ("CONNECT_ELYCTIS_READER", texts::CONNECT_ELYCTIS_READER),
        ("READING_DOCUMENT", texts::READING_DOCUMENT),
        ("CONNECTING", texts::CONNECTING),
        ("APPLET_ERROR", texts::APPLET_ERROR),
        ("CONNECTED", texts::CONNECTED),
        ("FRONT_ANTENNA", texts::FRONT_ANTENNA),
        ("BACK_ANTENNA", texts::BACK_ANTENNA),
        ("BAC_CONNECTING", texts::BAC_CONNECTING),
        ("BAC_CONNECTED", texts::BAC_CONNECTED),
        ("BAC_ERROR", texts::BAC_ERROR),
        ("BAP_CONNECTING", texts::BAP_CONNECTING),
        ("BAP_CONNECTED", texts::BAP_CONNECTED),
        ("BAP_ERROR", texts::BAP_ERROR),
        ("DISCONNECTED", texts::DISCONNECTED),
        ("PHOTO_CLONE_ERROR", texts::PHOTO_CLONE_ERROR),
        ("DG1_READING", texts::DG1_READING),
        ("DG1_READ", texts::DG1_READ),
        ("DG1_ERROR", texts::DG1_ERROR),
        ("DG1_NOT_READ", texts::DG1_NOT_READ),
        ("DG2_READING", texts::DG2_READING),
        ("DG2_READ", texts::DG2_READ),
        ("DG2_PHOTO_ERROR", texts::DG2_PHOTO_ERROR),
        ("DG2_ERROR", texts::DG2_ERROR),
        ("DG6_READING", texts::DG6_READING),
        ("DG6_READ", texts::DG6_READ),
        ("DG6_PHOTO_ERROR", texts::DG6_PHOTO_ERROR),
        ("DG6_ERROR", texts::DG6_ERROR),
        ("NO_DATA", texts::NO_DATA),
        ("PASSPORT_READ", texts::PASSPORT_READ),
        ("FROM_CHIP", texts::FROM_CHIP),
        ("FROM_MRZ", texts::FROM_MRZ),
        ("MRZ_INVALID", texts::MRZ_INVALID),
        ("MRZ_LENGTH_INVALID_FORMAT", texts::MRZ_LENGTH_INVALID_FORMAT),
        ("SEARCH_READER_BUTTON", texts::SEARCH_READER_BUTTON),
        ("SEARCH_IDBOX_STARTING", texts::SEARCH_IDBOX_STARTING),
        ("SEARCH_IDBOX_STARTING_DETAIL", texts::SEARCH_IDBOX_STARTING_DETAIL),
        ("SEARCH_READ_ERROR", texts::SEARCH_READ_ERROR),
    ]
}

#[test]
fn every_text_is_a_string_literal_of_the_csharp_sources() {
    let root = fixture();
    let literals: std::collections::HashSet<&str> =
        array(&root, "sourceLiterals").iter().map(|t| t.as_str().unwrap()).collect();

    for (name, value) in all_texts() {
        let expected = value.replace("{}", "{mrz.Length}");
        assert!(
            literals.contains(expected.as_str()),
            "{name} = {value:?} must be byte-identical to a C# literal"
        );
    }
}

#[test]
fn ely_checksum_matches_every_probed_case() {
    let root = fixture();
    let cases = array(&root, "checksums");
    assert!(cases.len() > 200);
    let mut mismatches = Vec::new();
    for c in cases {
        let input = c["input"].as_str().unwrap();
        let ely = c["ely"].as_bool().unwrap();
        if checksum::verify(input) != ely {
            mismatches.push(format!("{input} → ely {ely}"));
        }
    }
    assert_eq!(mismatches.join("; "), "");
}

#[test]
fn ely_checksum_matches_the_per_character_map() {
    let root = fixture();
    let map = array(&root, "checksumCharMap");
    assert_eq!(map.len(), 95);
    let digits = ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "<", "A"];
    let mut mismatches = Vec::new();
    for entry in map {
        let code = entry["code"].as_u64().unwrap() as u8 as char;
        let expected = entry["accepted"].as_str().unwrap();
        let actual: String = digits
            .iter()
            .filter(|d| checksum::verify(&format!("00{code}{d}")))
            .copied()
            .collect();
        if actual != expected {
            mismatches.push(format!("'{code}': ely [{expected}] ours [{actual}]"));
        }
    }
    assert_eq!(mismatches.join("; "), "");
}

#[test]
fn get_all_messages_matches_asbt_utils_v2() {
    let root = fixture();
    assert_eq!(errortext::get_all_messages(&["single"]), root["getAllMessages"]["one"].as_str().unwrap());
    assert_eq!(
        errortext::get_all_messages(&["outer", "middle", "inner"]),
        root["getAllMessages"]["three"].as_str().unwrap()
    );
}

// ---- MrzLines -----------------------------------------------------------------------

#[test]
fn mrz_lines_matches_mrz_cs() {
    let root = fixture();
    for c in array(&root, "mrz") {
        let name = c["name"].as_str().unwrap();
        let input = c["input"].as_str().unwrap();

        if let Some(exc) = c["exception"].as_str() {
            let err = MrzLines::new(input).expect_err(name);
            assert_eq!(err.dotnet_type_name(), exc, "{name}");
            continue;
        }

        let m = MrzLines::new(input).unwrap_or_else(|_| panic!("{name} parses"));
        assert_eq!(m.line1(), c["line1"].as_str().unwrap(), "{name} line1");
        assert_eq!(m.line2(), c["line2"].as_str().unwrap(), "{name} line2");
        assert_eq!(m.line3(), c["line3"].as_str().unwrap(), "{name} line3");
        assert_eq!(m.mrz_info(), c["mrzInfo"].as_str(), "{name} mrzInfo");
        assert_eq!(m.checksum(), c["checksum"].as_bool().unwrap(), "{name} checksum");
    }
}

// ---- CMRZ (ReaderResultBuilder) -----------------------------------------------------

fn field<'a>(r: &'a ReaderResult, name: &str) -> Option<&'a str> {
    match name {
        "Additional" => r.additional.as_deref(),
        "BirthDate" => r.birth_date.as_deref(),
        "DocumentNumber" => r.document_number.as_deref(),
        "DocumentType" => r.document_type.as_deref(),
        "ExpiryDate" => r.expiry_date.as_deref(),
        "Issuer" => r.issuer.as_deref(),
        "Nationality" => r.nationality.as_deref(),
        "Citizenship" => r.citizenship.as_deref(),
        "Pinpp" => r.pinpp.as_deref(),
        "Surname" => r.surname.as_deref(),
        "Name" => r.name.as_deref(),
        "Sex" => r.sex.as_deref(),
        "Line1" => r.line1.as_deref(),
        "Line2" => r.line2.as_deref(),
        "Line3" => r.line3.as_deref(),
        other => panic!("unknown field {other}"),
    }
}

const FIELDS: [&str; 15] = [
    "Additional", "BirthDate", "DocumentNumber", "DocumentType", "ExpiryDate", "Issuer", "Nationality",
    "Citizenship", "Pinpp", "Surname", "Name", "Sex", "Line1", "Line2", "Line3",
];

fn assert_fields(r: &ReaderResult, expected: &Value, label: &str) {
    for f in FIELDS {
        assert_eq!(field(r, f), expected[f].as_str(), "{label} {f}");
    }
    assert_eq!(r.information_from.as_i32(), expected["InformationFrom"].as_i64().unwrap() as i32, "{label} InformationFrom");
}

#[test]
fn from_mrz_matches_cmrz_string() {
    let root = fixture();
    let builder = ReaderResultBuilder::new(GENERATED_YEAR);
    for c in array(&root, "cmrzString") {
        let name = c["name"].as_str().unwrap();
        let input = c["input"].as_str().unwrap();

        if let Some(exc) = c["exception"].as_str() {
            let err = builder.from_mrz(input).expect_err(name);
            if exc == "Asbt.Models.AsbtException" {
                assert_eq!(err, MrzError::Length(err_len(&err)), "{name} is Length");
                assert_eq!(err.message().unwrap(), c["message"].as_str().unwrap(), "{name} message");
                assert_eq!(err.comment().unwrap(), c["comment"].as_str().unwrap(), "{name} comment");
                assert_eq!(err.answere_id().unwrap(), c["answereId"].as_i64().unwrap(), "{name} answereId");
            } else {
                assert_eq!(err.dotnet_type_name(), exc, "{name}");
            }
            continue;
        }

        let r = builder.from_mrz(input).unwrap_or_else(|_| panic!("{name} parses"));
        assert_fields(&r, &c["result"], name);
    }
}

fn err_len(e: &MrzError) -> usize {
    match e {
        MrzError::Length(n) => *n,
        _ => usize::MAX,
    }
}

#[test]
fn from_chip_matches_cmrz_reader_result() {
    let root = fixture();
    let builder = ReaderResultBuilder::new(GENERATED_YEAR);
    for c in array(&root, "cmrzChip") {
        let name = c["name"].as_str().unwrap();
        let input = ReaderResult {
            additional: Some(String::new()),
            birth_date: Some(c["birthDate"].as_str().unwrap().to_string()),
            document_number: Some("AA1234567".to_string()),
            document_type: Some("P".to_string()),
            expiry_date: Some(c["expiryDate"].as_str().unwrap().to_string()),
            issuer: Some("Uzbekistan".to_string()),
            name: Some("ALISHER".to_string()),
            nationality: Some("Uzbekistan".to_string()),
            pinpp: Some(c["pinpp"].as_str().unwrap().to_string()),
            sex: Some("M".to_string()),
            surname: Some("TOSHMATOV".to_string()),
            line1: Some(TD3_LINE1.to_string()),
            line2: Some(TD3_LINE2.to_string()),
            line3: Some(String::new()),
            information_from: ReadingType::Chip,
            ..Default::default()
        };

        if let Some(exc) = c["exception"].as_str() {
            let err = builder.from_chip(input).expect_err(name);
            assert_eq!(err.dotnet_type_name(), exc, "{name}");
            continue;
        }

        let r = builder.from_chip(input).unwrap_or_else(|_| panic!("{name} parses"));
        assert_fields(&r, &c["result"], name);
    }
}

#[test]
fn century_heuristic_uses_the_injected_year() {
    let root = fixture();
    let td3 = array(&root, "cmrzString")
        .iter()
        .find(|x| x["name"] == "td3-filler-pinpp")
        .unwrap()["input"]
        .as_str()
        .unwrap();

    assert_eq!(
        ReaderResultBuilder::new(2009).from_mrz(td3).unwrap().birth_date.as_deref(),
        Some("03.02.2010")
    );
    assert_eq!(
        ReaderResultBuilder::new(2010).from_mrz(td3).unwrap().birth_date.as_deref(),
        Some("03.02.1910")
    );
}

// ---- Answere JSON -------------------------------------------------------------------

fn golden(root: &Value, name: &str) -> String {
    root["answereJson"][name].as_str().unwrap().to_string()
}

fn mrz_result() -> ReaderResult {
    let mut r = ReaderResultBuilder::new(GENERATED_YEAR)
        .from_mrz(&format!("{TD3_LINE1}{TD3_LINE2}"))
        .unwrap();
    r.document_reading_time = 3.2517;
    r
}

fn chip_result(photo: Option<&str>) -> ReaderResult {
    ReaderResultBuilder::new(GENERATED_YEAR)
        .from_chip(ReaderResult {
            additional: Some(String::new()),
            birth_date: Some("12/08/74".to_string()),
            document_number: Some("AA1234567".to_string()),
            document_type: Some("P".to_string()),
            expiry_date: Some("15/04/32".to_string()),
            issuer: Some("Uzbekistan".to_string()),
            name: Some("ALISHER".to_string()),
            nationality: Some("Uzbekistan".to_string()),
            pinpp: Some("31208740000000".to_string()),
            sex: Some("M".to_string()),
            surname: Some("TOSHMATOV".to_string()),
            line1: Some(TD3_LINE1.to_string()),
            line2: Some(TD3_LINE2.to_string()),
            line3: Some(String::new()),
            person_photo: photo.map(str::to_string),
            document_reading_time: 7.125,
            information_from: ReadingType::Chip,
            ..Default::default()
        })
        .unwrap()
}

#[test]
fn alert_text_mrz_fallback_is_byte_identical() {
    let root = fixture();
    let json = to_csharp_json(1, Some(texts::PASSPORT_READ), Some(texts::FROM_MRZ), Some(&mrz_result()));
    assert_eq!(json, golden(&root, "mrz"));
}

#[test]
fn alert_text_chip_with_photo_renders_the_bitmap() {
    let root = fixture();
    let json = to_csharp_json(1, Some(texts::PASSPORT_READ), Some(texts::FROM_CHIP), Some(&chip_result(Some("/9j/AAA="))));
    assert_eq!(json, golden(&root, "chipWithBitmap"));
    assert!(json.contains("\"PersonPhoto\":\"System.Drawing.Bitmap\""));
}

#[test]
fn alert_text_chip_without_photo_is_byte_identical() {
    let root = fixture();
    let json = to_csharp_json(1, Some(texts::PASSPORT_READ), Some(texts::FROM_CHIP), Some(&chip_result(None)));
    assert_eq!(json, golden(&root, "chipNoPhoto"));
}

#[test]
fn alert_text_minimal_and_initial_are_byte_identical() {
    let root = fixture();
    let minimal = ReaderResult {
        name: Some("ALISHER".to_string()),
        person_photo: Some("x".to_string()),
        document_reading_time: 0.5,
        information_from: ReadingType::Chip,
        ..Default::default()
    };
    assert_eq!(
        to_csharp_json(1, Some(texts::PASSPORT_READ), Some(texts::FROM_CHIP), Some(&minimal)),
        golden(&root, "chipMinimalWithBitmap")
    );
    assert_eq!(
        to_csharp_json(0, Some(texts::NO_DATA), Some(texts::NO_DATA), None),
        golden(&root, "initialReaderResult")
    );
}

#[test]
fn plugin_data_mrz_fallback_exact_shape() {
    let root = fixture();
    let json = to_plugin_data(1, texts::PASSPORT_READ, texts::FROM_MRZ, &mrz_result());

    let alert = serde_json::to_string(&golden(&root, "mrz")).unwrap();
    let expected = format!(
        "{{\"AnswereId\":1,\"AnswereMessage\":\"Паспорт ўқилди\",\"AnswereComment\":\"Маълумотлар MRZ дан\",\
\"Data\":{{\"Additional\":null,\"BirthDate\":\"01.01.1990\",\"DocumentNumber\":\"AA1234567\",\"DocumentType\":\"P\",\
\"ExpiryDate\":\"01.01.2030\",\"Issuer\":null,\"Nationality\":\"UZB\",\"Citizenship\":\"UZB\",\"Pinpp\":\"31234567890123\",\
\"Surname\":\"TOSHMATOV\",\"Name\":\"ALISHER\",\"Sex\":\"M\",\"Line1\":\"P<UZBTOSHMATOV<<ALISHER<<<<<<<<<<<<<<<<<<<<<\",\
\"Line2\":\"AA12345678UZB9001011M30010193123456789012324\",\"Line3\":null,\"PersonPhoto\":null,\
\"DocumentReadingTime\":3.2517,\"InformationFrom\":1}},\
\"AlertText\":{alert}}}"
    );
    assert_eq!(json, expected);
}

#[test]
fn plugin_data_chip_carries_photo_and_alert() {
    let root = fixture();
    let json = to_plugin_data(1, texts::PASSPORT_READ, texts::FROM_CHIP, &chip_result(Some("/9j/AAA=")));
    let data: Value = serde_json::from_str(&json).unwrap();

    // Key order is pinned on the raw bytes (a parsed Value sorts its keys).
    let order = ["AnswereId", "AnswereMessage", "AnswereComment", "Data", "AlertText"];
    let positions: Vec<usize> = order
        .iter()
        .map(|k| json.find(&format!("\"{k}\":")).unwrap_or_else(|| panic!("{k} present")))
        .collect();
    assert!(positions.windows(2).all(|w| w[0] < w[1]), "keys in order: {positions:?}");
    assert_eq!(data["Data"]["PersonPhoto"].as_str(), Some("/9j/AAA="));
    assert_eq!(data["Data"]["InformationFrom"].as_i64(), Some(0));
    assert_eq!(data["Data"]["BirthDate"].as_str(), Some("12.08.1974"));
    assert_eq!(data["AlertText"].as_str(), Some(golden(&root, "chipWithBitmap").as_str()));
}
