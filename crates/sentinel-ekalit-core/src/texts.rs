//! The user-visible strings of the reference `EKalitObj` / `SignService` /
//! `PinWindow`, byte-for-byte.

/// `EKalitId` error message.
pub const EKALIT_ID_ERROR: &str = "E-Kalit рақамини олишда ҳато";
/// Cancelled (PIN dialog closed).
pub const CANCELLED: &str = "Амал бекор қилинди";
/// Sign error message.
pub const SIGN_ERROR: &str = "Имзолашда ҳато";
/// GetCertificateInfo error message.
pub const CERT_INFO_ERROR: &str = "Сертификат маълумотини олишда ҳато";
/// Success message.
pub const OK: &str = "Ok";
/// Token not connected.
pub const NOT_CONNECTED: &str = "E-Kalit уланмаган";
/// Wrong PIN.
pub const WRONG_PIN: &str = "E-Kalit Pin код нотўғри";
/// PIN not supplied.
pub const PIN_MISSING: &str = "Пин код берилмаган";

// PinWindow.
/// The PIN window title.
pub const PIN_TITLE: &str = "E-Kalit";
/// The PIN prompt.
pub const PIN_PROMPT: &str = "E-Kalit пинни киритинг";
/// The PIN confirm button.
pub const PIN_SIGN: &str = "Имзолаш";
