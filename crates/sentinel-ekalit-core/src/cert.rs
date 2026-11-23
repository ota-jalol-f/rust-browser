//! `SignService.GetCertificateInfo`: parse an X.509 certificate into a
//! [`ModelCertificate`] (subject CN, serial, validity). C# used `X509Certificate2`;
//! this uses the pure-Rust `x509-parser`.

use crate::models::ModelCertificate;

/// Parse a DER certificate. `None` when it does not parse.
pub fn parse_certificate(der: &[u8]) -> Option<ModelCertificate> {
    let (_, cert) = x509_parser::parse_x509_certificate(der).ok()?;

    // C#: subject.Split(',') → the "CN=" entry, trimmed; else "Аниқ эмас".
    let owner = cert
        .subject()
        .iter_common_name()
        .next()
        .and_then(|cn| cn.as_str().ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "Аниқ эмас".to_string());

    // C#: x509.SerialNumber.ToUpper() — uppercase hex of the serial bytes.
    let certificate_guid = cert.raw_serial().iter().map(|b| format!("{b:02X}")).collect::<String>();

    Some(ModelCertificate {
        owner,
        certificate_guid,
        certificate_date_from: iso_utc(&cert.validity().not_before),
        certificate_date_till: iso_utc(&cert.validity().not_after),
    })
}

/// Render an ASN.1 time as ISO-8601 UTC (`YYYY-MM-DDThh:mm:ssZ`).
fn iso_utc(t: &x509_parser::time::ASN1Time) -> String {
    let dt = t.to_datetime();
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        dt.year(),
        dt.month() as u8,
        dt.day(),
        dt.hour(),
        dt.minute(),
        dt.second()
    )
}
