//! The EKalit models serialized into `Answere.Data`.

use serde_json::{json, Value};

use crate::base64;

/// `ModelCertificate` — what `GetCertificateInfo` returns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelCertificate {
    /// The certificate subject's CN (or "Аниқ эмас").
    pub owner: String,
    /// The serial number, uppercase hex (C#'s `x509.SerialNumber.ToUpper()`).
    pub certificate_guid: String,
    /// `NotBefore`, ISO-8601 (UTC; C# used local time — a documented tz divergence).
    pub certificate_date_from: String,
    /// `NotAfter`, ISO-8601 (UTC).
    pub certificate_date_till: String,
}

impl ModelCertificate {
    /// The `Data` object (`Owner`/`CertificateGuid`/`CertificateDateFrom`/`CertificateDateTill`).
    pub fn to_json(&self) -> Value {
        json!({
            "Owner": self.owner,
            "CertificateGuid": self.certificate_guid,
            "CertificateDateFrom": self.certificate_date_from,
            "CertificateDateTill": self.certificate_date_till,
        })
    }
}

/// `ModelSignature<string>` — the signed-string result in `Answere.Data`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelSignature {
    /// The exact string that was signed.
    pub model_to_sign: String,
    /// The signer certificate bytes (serialized as base64, as Newtonsoft does for `byte[]`).
    pub public_key: Vec<u8>,
    /// The base64 signature.
    pub signed_data: String,
    /// `SignedDate`, ISO-8601.
    pub signed_date: String,
    /// 0 (V1) or 2 (V2).
    pub version: i32,
}

impl ModelSignature {
    /// The `Data` object (member order as the C# class declares).
    pub fn to_json(&self) -> Value {
        json!({
            "ModelToSign": self.model_to_sign,
            "PublicKey": base64::encode(&self.public_key),
            "SignedData": self.signed_data,
            "SignedDate": self.signed_date,
            "Version": self.version,
        })
    }
}
