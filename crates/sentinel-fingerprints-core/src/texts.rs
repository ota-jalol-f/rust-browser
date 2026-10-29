//! User-visible texts of the reference `FormFingerprintsV2`, copied from
//! `Asbt.Fingerprints.Plugin/Enroll/EnrollTexts.cs` byte-for-byte (UTF-8).
//!
//! This is the subset the device-free core needs (the save-guard message, the
//! oversize warning, and the single-finger prompt names). The remaining window
//! labels are added with the enrollment overlay. The C# originals keep their typos
//! ("Қуримла", "ажратилмаёпти", "Иккта", "Button"); do not "fix" them.

/// `GetFingerprints` save guard — `AsbtException.AnswereComment`.
pub const FINGERPRINTS_INCOMPLETE: &str = "Бармоқ излари тўлиқ берилмаган";

/// The `RemoveNullElements` oversize warning (`i > 18000`).
pub const WSQ_TOO_LARGE: &str =
    "Сурат ўлчами тавсия этилган ўлчамдан катта \n Ушбу бармоқни қайта сканер қилинг";

/// `FingerPosition.ToStr()` for 0..10, the .NET Framework `Enum.ToString` of the
/// aliased enum (mixed: `RightIndex` but `RightRingFinger`). Index 0 is `Unknown`.
pub const FINGER_POSITION_NAMES: [&str; 11] = [
    "Unknown",
    "RightThumb",
    "RightIndex",
    "RightMiddle",
    "RightRingFinger",
    "RightLittleFinger",
    "LeftThumb",
    "LeftIndexFinger",
    "LeftMiddle",
    "LeftRingFinger",
    "LeftLittleFinger",
];

/// The printed name of a 0..10 single-finger position (the single-finger enroll
/// prompt uses `"{name} бармоқни сканерга қўйинг"`).
pub fn finger_position_name(number: i32) -> &'static str {
    FINGER_POSITION_NAMES.get(number as usize).copied().unwrap_or("Unknown")
}
