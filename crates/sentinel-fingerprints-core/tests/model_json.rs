//! `ModelFingerprint` → JSON against the .NET Framework golden strings (the port of
//! the C# `ModelFingerprintJsonTests`).

use sentinel_fingerprints_core::finger::{EnrolledFinger, FingerStatus, FingerType};
use sentinel_fingerprints_core::model_json::{build, serialize, Dto};
use sentinel_fingerprints_core::texts;

const FRAMEWORK_EMPTY: &str = "{\"fingerStatus1\":3,\"fingerStatus2\":3,\"fingerStatus3\":3,\"fingerStatus4\":3,\"fingerStatus5\":3,\"fingerStatus6\":3,\"fingerStatus7\":3,\"fingerStatus8\":3,\"fingerStatus9\":3,\"fingerStatus10\":3}";

const FRAMEWORK_SAMPLE: &str = "{\"Finger1\":\"/6AB/6E=\",\"FingerQuality1\":\"4\",\"fingerStatus1\":1,\"Finger2\":\"/6AC/6E=\",\"FingerQuality2\":\"0\",\"fingerStatus2\":2,\"FingerQuality3\":\"\",\"fingerStatus3\":3,\"Finger4\":\"/6AE/6E=\",\"FingerQuality4\":\"4\",\"fingerStatus4\":1,\"Finger5\":\"/6AF/6E=\",\"FingerQuality5\":\"4\",\"fingerStatus5\":1,\"Finger6\":\"/6AG/6E=\",\"FingerQuality6\":\"4\",\"fingerStatus6\":1,\"Finger7\":\"/6AH/6E=\",\"FingerQuality7\":\"4\",\"fingerStatus7\":1,\"Finger8\":\"/6AI/6E=\",\"FingerQuality8\":\"4\",\"fingerStatus8\":1,\"Finger9\":\"/6AJ/6E=\",\"FingerQuality9\":\"4\",\"fingerStatus9\":1,\"Finger10\":\"/6AK/6E=\",\"FingerQuality10\":\"4\",\"fingerStatus10\":1}";

fn sample() -> Vec<EnrolledFinger> {
    let mut fingers: Vec<EnrolledFinger> = (1..=10)
        .map(|n| {
            let mut f = EnrolledFinger::new(FingerType::from_number(n).unwrap());
            f.wsq = Some(vec![0xFF, 0xA0, n as u8, 0xFF, 0xA1]);
            f.quality = Some(4);
            f.is_enrolled = true;
            f
        })
        .collect();
    fingers[1].status = FingerStatus::Damaged;
    fingers[1].quality = Some(0);
    fingers[2].status = FingerStatus::Absent;
    fingers[2].wsq = None;
    fingers[2].quality = None;
    fingers[2].is_enrolled = false;
    fingers
}

#[test]
fn build_matches_the_framework_json_byte_for_byte() {
    assert_eq!(serialize(&build(&sample()).unwrap()), FRAMEWORK_SAMPLE);
}

#[test]
fn empty_model_matches_a_fresh_csharp_model() {
    assert_eq!(serialize(&Dto::new()), FRAMEWORK_EMPTY);
}

#[test]
fn build_always_emits_all_ten_statuses_and_no_nulls() {
    let json = serialize(&build(&sample()).unwrap());
    for n in 1..=10 {
        assert!(json.contains(&format!("\"fingerStatus{n}\":")));
    }
    assert!(!json.contains("FingerStatus")); // lower-camel, as the C# property
    assert!(!json.contains("\"Id\""));
    assert!(!json.contains("null"));
}

#[test]
fn excist_finger_without_wsq_is_rejected_with_the_csharp_message() {
    let mut fingers = sample();
    fingers[6].wsq = None; // finger 7 Excist, no WSQ
    let err = build(&fingers).unwrap_err();
    assert_eq!(err.message(), texts::FINGERPRINTS_INCOMPLETE);
    assert_eq!(err.message(), "Бармоқ излари тўлиқ берилмаган");
}

#[test]
fn excist_finger_with_one_byte_wsq_is_rejected() {
    let mut fingers = sample();
    fingers[0].wsq = Some(vec![0xFF]);
    assert!(build(&fingers).is_err());
}

#[test]
fn damaged_without_wsq_is_accepted_with_empty_quality() {
    let mut fingers = sample();
    fingers[4].status = FingerStatus::Damaged;
    fingers[4].wsq = None;
    fingers[4].quality = None;
    let json = serialize(&build(&fingers).unwrap());
    assert!(json.contains("\"FingerQuality5\":\"\",\"fingerStatus5\":2,\"Finger6\""));
}

#[test]
fn negative_quality_is_printed_as_is() {
    let mut fingers = sample();
    fingers[0].quality = Some(-1);
    let json = serialize(&build(&fingers).unwrap());
    assert!(json.contains("\"FingerQuality1\":\"-1\""));
}

#[test]
fn finger_position_names_are_the_framework_enum_names() {
    assert_eq!(texts::finger_position_name(4), "RightRingFinger");
    assert_eq!(texts::finger_position_name(2), "RightIndex");
    assert_eq!(texts::finger_position_name(9), "LeftRingFinger");
    assert_eq!(texts::finger_position_name(0), "Unknown");
}
