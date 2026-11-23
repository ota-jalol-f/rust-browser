//! The E-Kalit token seam (`SignService` over `libesigner`). The real backend needs
//! the vendor **native** SDK (the source only has the managed `libesigner.net`
//! wrapper, like SignPad's Topaz), so the shipped build drives a stub signer; the
//! plugin orchestrates these calls with the PIN from its overlay.

/// A successful `SignUSB`: the signer identity + certificate + signature.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignOutcome {
    /// The certificate serial number, uppercase (C#'s `outUserId`).
    pub user_id: String,
    /// The signer certificate (DER), from `ReadCertificate`.
    pub certificate_der: Vec<u8>,
    /// The base64 signature (`GetBytes` → base64).
    pub signed_base64: String,
}

/// The E-Kalit USB token as `SignService` drives it.
pub trait Signer {
    /// `IsConnected()`.
    fn is_connected(&mut self) -> bool;
    /// `GetKalitID()`: the token UID (upper-cased, dashes removed). `Err` when not
    /// connected.
    fn get_uid(&mut self) -> Result<String, String>;
    /// `SignUSB(pin, value)`: hash + sign `value` with the token key, read the
    /// certificate. `Err` on a wrong PIN / not connected.
    fn sign(&mut self, pin: &str, value: &str) -> Result<SignOutcome, String>;
}
