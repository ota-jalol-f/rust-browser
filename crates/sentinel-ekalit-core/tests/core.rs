//! `sentinel-ekalit-core` against golden output extracted from real Newtonsoft (the
//! C# `SumValues`/`GetValues` repro) and a C#-generated X.509 certificate.

use std::path::PathBuf;

use sentinel_ekalit_core::{answere_json, parse_certificate, sign_string_v2};

// ---- canonicalisation (pinned to real Newtonsoft output) -----------------

#[test]
fn sign_string_v2_matches_newtonsoft() {
    // (input JSON, expected signed+uppercased string) — exact for string/int/bool/
    // object/array values.
    let cases: &[(&str, &str)] = &[
        (r#"{"b":2,"a":"x"}"#, "X2"),
        (r#"{"a":"hello","b":123,"c":true}"#, "HELLO123TRUE"),
        (r#"{"z":"last","a":"first","m":"mid"}"#, "FIRSTMIDLAST"),
        (r#"{"a":{"c":3,"d":4},"b":"top"}"#, "34TOP"),
        (r#"{"a":[1,2,3],"b":"x"}"#, "123X"),
        (r#"[{"b":2,"a":1},{"a":"y"}]"#, "12Y"),
        (r#"{"val":null,"keep":"yes"}"#, "YES"),
        (r#"{"Name":"Ali","Age":30,"Active":false}"#, "FALSE30ALI"),
        (r#""plainstring""#, "PLAINSTRING"),
        ("42", "42"),
        (r#"{"a":"x"}"#, "X"),
        (r#"{"a":{"b":{"c":5}},"z":"end"}"#, "5END"),
        (r#"{"a":[{"x":1}],"b":2}"#, "2"),
        ("{}", ""),
        (r#"{"a":{}}"#, ""),
        (r#"{"a":{"b":5},"c":6}"#, "56"),
        // The crux of document-order nesting (only the top level is sorted):
        (r#"{"a":{"d":4,"c":3},"b":1}"#, "431"),
        (r#"{"outer":{"zz":"Z","aa":"A"}}"#, "ZA"),
    ];
    for (input, expected) in cases {
        assert_eq!(sign_string_v2(input).unwrap().as_str(), *expected, "input {input}");
    }
}

#[test]
fn sign_string_v2_rejects_invalid_json() {
    assert!(sign_string_v2("{not json").is_err());
}

// ---- Answere envelope (pinned to real Newtonsoft output) -----------------

#[test]
fn answere_envelope_matches_newtonsoft() {
    assert_eq!(
        answere_json(1, "Ok", "", Some(serde_json::Value::String("ABC123".into()))),
        r#"{"AnswereId":1,"AnswereMessage":"Ok","AnswereComment":"","Data":"ABC123"}"#
    );
    // Null Data is omitted.
    assert_eq!(
        answere_json(0, "E-Kalit рақамини олишда ҳато", "boom", None),
        r#"{"AnswereId":0,"AnswereMessage":"E-Kalit рақамини олишда ҳато","AnswereComment":"boom"}"#
    );
    assert_eq!(
        answere_json(10, "Амал бекор қилинди", "", None),
        r#"{"AnswereId":10,"AnswereMessage":"Амал бекор қилинди","AnswereComment":""}"#
    );
}

// ---- certificate parsing -------------------------------------------------

#[test]
fn parse_certificate_reads_cn_serial_and_validity() {
    let der = {
        let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/test-cert.der");
        std::fs::read(p).expect("test-cert.der")
    };
    let cert = parse_certificate(&der).expect("parse");
    assert_eq!(cert.owner, "Test Owner Ali");
    assert_eq!(cert.certificate_guid, "3CC928A72E428738");
    assert_eq!(cert.certificate_date_from, "2026-01-02T03:04:05Z");
    assert_eq!(cert.certificate_date_till, "2028-01-02T03:04:05Z");
}
