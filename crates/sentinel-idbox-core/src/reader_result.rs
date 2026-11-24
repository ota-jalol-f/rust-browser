//! pv-winform Asbt.IdBox `ReaderResult.cs` + `ClassMRZ.cs` (class `CMRZ`), line by
//! line, quirks included — pinned against the real `Asbt.IdBox.exe` by
//! `tests/fixtures/reference.json` (`cmrzString`, `cmrzChip`). Do **not** "fix":
//!
//! - The century comes from `Pinpp[0]` (1/2 → 18, 3/4 → 19, 5/6 → 20); otherwise the
//!   inverted "`Today.Year >= 20yy` → 19" heuristic, which in the chip path reads the
//!   DAY digits.
//! - TD1 (90 chars): `Line3 = mrz.Substring(30, 30)` (= line 2), `Sex = line2[8]` (the
//!   first expiry digit), Surname/Name are cut with an index that in practice throws
//!   `ArgumentOutOfRangeException` — so the MRZ fallback never works for ID cards (TD1).
//! - An empty `Pinpp` (chip path with empty DG1 optional data) throws
//!   `IndexOutOfRangeException`.
//!
//! Ported from `Asbt.IdBox.Plugin.Mrz.{ReaderResult, ReadingType, ReaderResultBuilder}`.

use std::fmt;

use crate::texts;

/// `ReadingType`: Chip = 0, MRZ = 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadingType {
    /// Data came from the chip.
    Chip,
    /// Data came from the MRZ fallback.
    Mrz,
}

impl ReadingType {
    /// The integer the C# enum serializes to (Chip = 0, MRZ = 1).
    pub fn as_i32(self) -> i32 {
        match self {
            ReadingType::Chip => 0,
            ReadingType::Mrz => 1,
        }
    }
}

/// pv-winform Asbt.IdBox `ReaderResult`; fields in the C# declaration order (the JSON
/// order). String fields are `None` where C# has `null`.
#[derive(Debug, Clone, PartialEq)]
pub struct ReaderResult {
    /// `Additional`.
    pub additional: Option<String>,
    /// `BirthDate` ("dd.MM.yyyy" after the builder).
    pub birth_date: Option<String>,
    /// `DocumentNumber`.
    pub document_number: Option<String>,
    /// `DocumentType`.
    pub document_type: Option<String>,
    /// `ExpiryDate`.
    pub expiry_date: Option<String>,
    /// `Issuer`.
    pub issuer: Option<String>,
    /// `Nationality`.
    pub nationality: Option<String>,
    /// `Citizenship`.
    pub citizenship: Option<String>,
    /// `Pinpp`.
    pub pinpp: Option<String>,
    /// `Surname`.
    pub surname: Option<String>,
    /// `Name`.
    pub name: Option<String>,
    /// `Sex`.
    pub sex: Option<String>,
    /// `Line1`.
    pub line1: Option<String>,
    /// `Line2`.
    pub line2: Option<String>,
    /// `Line3`.
    pub line3: Option<String>,
    /// The chip photo as base64 JPEG, or `None` (C#: a `System.Drawing.Bitmap`).
    pub person_photo: Option<String>,
    /// `DocumentReadingTime` in seconds.
    pub document_reading_time: f64,
    /// `InformationFrom`.
    pub information_from: ReadingType,
}

impl Default for ReaderResult {
    fn default() -> Self {
        ReaderResult {
            additional: None,
            birth_date: None,
            document_number: None,
            document_type: None,
            expiry_date: None,
            issuer: None,
            nationality: None,
            citizenship: None,
            pinpp: None,
            surname: None,
            name: None,
            sex: None,
            line1: None,
            line2: None,
            line3: None,
            person_photo: None,
            document_reading_time: 0.0,
            // C#'s `new ReaderResult()` leaves InformationFrom = Chip (0).
            information_from: ReadingType::Chip,
        }
    }
}

/// The error a malformed MRZ raises, mirroring the C# exception the matching code path
/// threw. The .NET type name is preserved so the port can be pinned against the
/// fixture, which records the original exception.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MrzError {
    /// C# `System.IndexOutOfRangeException` (e.g. `Pinpp[0]` on an empty string).
    IndexOutOfRange,
    /// C# `System.ArgumentOutOfRangeException` (a `Substring` out of range).
    ArgumentOutOfRange,
    /// C# `System.FormatException` (`ToInt` on non-digits).
    Format,
    /// C# `Asbt.Models.AsbtException`: the MRZ fallback text was not 88/90 long.
    Length(usize),
}

impl MrzError {
    /// The full .NET type name C# threw, for fixture comparison.
    pub fn dotnet_type_name(&self) -> &'static str {
        match self {
            MrzError::IndexOutOfRange => "System.IndexOutOfRangeException",
            MrzError::ArgumentOutOfRange => "System.ArgumentOutOfRangeException",
            MrzError::Format => "System.FormatException",
            MrzError::Length(_) => "Asbt.Models.AsbtException",
        }
    }

    /// `AsbtException.Message` (only the [`MrzError::Length`] case has one).
    pub fn message(&self) -> Option<&'static str> {
        match self {
            MrzError::Length(_) => Some(texts::MRZ_INVALID),
            _ => None,
        }
    }

    /// `AsbtException.AnswereComment` for the [`MrzError::Length`] case.
    pub fn comment(&self) -> Option<String> {
        match self {
            MrzError::Length(n) => Some(texts::MRZ_LENGTH_INVALID_FORMAT.replace("{}", &n.to_string())),
            _ => None,
        }
    }

    /// `AsbtException.AnswereId` (2) for the [`MrzError::Length`] case.
    pub fn answere_id(&self) -> Option<i64> {
        match self {
            MrzError::Length(_) => Some(2),
            _ => None,
        }
    }
}

impl fmt::Display for MrzError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MrzError::Length(_) => write!(f, "{}", texts::MRZ_INVALID),
            other => write!(f, "{}", other.dotnet_type_name()),
        }
    }
}

impl std::error::Error for MrzError {}

/// `CMRZ`: the chip and MRZ-fallback field logic. `current_year` stands in for
/// `DateTime.Today.Year` (injectable, as in the C# tests).
pub struct ReaderResultBuilder {
    current_year: i32,
}

impl ReaderResultBuilder {
    /// A builder whose century heuristic uses `current_year`.
    pub fn new(current_year: i32) -> Self {
        ReaderResultBuilder { current_year }
    }

    /// `CMRZ(ReaderResult)`: the chip path (FormReader.cs:583-603). Mutates and returns
    /// `reader_data`.
    pub fn from_chip(&self, mut r: ReaderResult) -> Result<ReaderResult, MrzError> {
        let pinpp = r.pinpp.clone().unwrap_or_default();
        let birth = r.birth_date.clone().unwrap_or_default();
        let centure = match char_at(&pinpp, 0)? {
            '1' | '2' => ".18",
            '3' | '4' => ".19",
            '5' | '6' => ".20",
            _ => {
                if self.current_year >= to_int(&format!("20{}", substr(&birth, 0, 2)?))? {
                    ".19"
                } else {
                    ".20"
                }
            }
        };

        let new_birth = format!(
            "{}.{}{}{}",
            substr(&birth, 0, 2)?.replace("<<", "01"),
            substr(&birth, 3, 2)?.replace("<<", "01"),
            centure,
            substr(&birth, 6, 2)?
        );
        r.birth_date = Some(new_birth);

        let expiry = r.expiry_date.clone().unwrap_or_default();
        let new_expiry = format!(
            "{}.{}.20{}",
            substr(&expiry, 0, 2)?,
            substr(&expiry, 3, 2)?,
            substr(&expiry, 6, 2)?
        );
        r.expiry_date = Some(new_expiry);
        Ok(r)
    }

    /// `CMRZ(string)`: the MRZ fallback (FormReader.cs:609, line1 + line2 + line3).
    pub fn from_mrz(&self, mrz: &str) -> Result<ReaderResult, MrzError> {
        let mut r = ReaderResult {
            information_from: ReadingType::Mrz,
            ..Default::default()
        };

        let mrz = mrz
            .to_ascii_uppercase()
            .replace(' ', "")
            .replace('\r', "")
            .replace('\n', "");

        if mrz.len() == 88 {
            let line1 = substr(&mrz, 0, 44)?;
            let line2 = substr(&mrz, 44, 44)?;
            r.line1 = Some(line1.clone());
            r.line2 = Some(line2.clone());
            self.parse_mrz_2(&mut r, &line1, &line2)?;
        } else if mrz.len() == 90 {
            let line1 = substr(&mrz, 0, 30)?;
            let line2 = substr(&mrz, 30, 30)?;
            let line3 = substr(&mrz, 30, 30)?; // sic: C# bug, should be (60, 30)
            r.line1 = Some(line1.clone());
            r.line2 = Some(line2.clone());
            r.line3 = Some(line3.clone());
            self.parse_mrz_3(&mut r, &line1, &line2, &line3)?;
        } else {
            return Err(MrzError::Length(mrz.len()));
        }

        Ok(r)
    }

    /// TD2/TD3 two-line `ParseMRZ`.
    fn parse_mrz_2(&self, r: &mut ReaderResult, mrz1: &str, mrz2: &str) -> Result<(), MrzError> {
        r.document_type = Some(substr(mrz1, 0, 2)?.replace('<', ""));
        r.nationality = Some(substr(mrz1, 2, 3)?);

        let dd = index_of(mrz1, "<<");
        r.surname = Some(substr_i(mrz1, 5, dd - 5)?.replace('<', " "));
        let a = index_of_any_from(mrz1, 5)?;
        let b = index_of_any_from(mrz1, a + 2)?;
        r.name = Some(substr_i(mrz1, a + 2, b - a - 2)?.replace('<', " "));
        r.document_number = Some(substr(mrz2, 0, 9)?.replace('<', ""));

        r.citizenship = Some(substr(mrz2, 10, 3)?);

        r.birth_date = Some(substr(mrz2, 13, 6)?.replace("<<", "01").replace('O', "0"));

        r.pinpp = Some(substr(mrz2, 28, 14)?);
        self.apply_birth_century(r)?;

        r.sex = Some(char_at(mrz2, 20)?.to_string().replace('H', "M").replace('N', "M"));
        apply_sex_fallback(r)?;

        r.expiry_date = Some(substr(mrz2, 21, 6)?.replace('O', "0"));
        apply_expiry(r)?;
        Ok(())
    }

    /// TD1 three-line `ParseMRZ`.
    fn parse_mrz_3(&self, r: &mut ReaderResult, mrz1: &str, mrz2: &str, mrz3: &str) -> Result<(), MrzError> {
        r.document_type = Some(substr(mrz1, 0, 2)?.replace('<', ""));
        r.nationality = Some(substr(mrz1, 2, 3)?);

        r.document_number = Some(substr(mrz1, 5, 9)?.replace('<', "").replace('O', "0"));
        r.pinpp = Some(substr(mrz1, 15, 14)?);
        r.birth_date = Some(substr(mrz2, 0, 6)?.replace("<<", "01").replace('O', "0"));
        self.apply_birth_century(r)?;

        r.sex = Some(char_at(mrz2, 8)?.to_string().replace('H', "M").replace('N', "M")); // sic: [8] is expiry's first digit
        apply_sex_fallback(r)?;

        r.expiry_date = Some(substr(mrz2, 8, 6)?.replace('O', "0"));
        apply_expiry(r)?;

        r.citizenship = Some(substr(mrz2, 15, 3)?);

        let dd = index_of(mrz3, "<<");
        r.surname = Some(substr_i(mrz3, 0, dd - 5)?.replace('<', " "));
        let a3 = index_of_any_from(mrz3, 5)?;
        let a1 = index_of_any_from(mrz1, 5)?; // sic: inner start comes from mrz1
        let b = index_of_any_from(mrz3, a1 + 2)?;
        r.name = Some(substr_i(mrz3, a3 + 2, b - a3 - 2)?.replace('<', " "));
        Ok(())
    }

    fn apply_birth_century(&self, r: &mut ReaderResult) -> Result<(), MrzError> {
        let birth = r.birth_date.clone().unwrap_or_default();
        if birth.chars().any(|c| c.is_ascii_alphabetic()) {
            return Ok(());
        }

        let pinpp = r.pinpp.clone().unwrap_or_default();
        let centure = match char_at(&pinpp, 0)? {
            '1' | '2' => ".18",
            '3' | '4' => ".19",
            '5' | '6' => ".20",
            _ => {
                if self.current_year >= to_int(&format!("20{}", substr(&birth, 0, 2)?))? {
                    ".19"
                } else {
                    ".20"
                }
            }
        };

        r.birth_date = Some(format!(
            "{}.{}{}{}",
            substr(&birth, 4, 2)?.replace("<<", "01"),
            substr(&birth, 2, 2)?.replace("<<", "01"),
            centure,
            substr(&birth, 0, 2)?
        ));
        Ok(())
    }
}

fn apply_sex_fallback(r: &mut ReaderResult) -> Result<(), MrzError> {
    let sex = r.sex.clone().unwrap_or_default();
    if sex != "M" && sex != "F" && sex != "<" {
        let pinpp = r.pinpp.clone().unwrap_or_default();
        let s = match char_at(&pinpp, 0)? {
            '1' | '3' | '5' => "M",
            _ => "F",
        };
        r.sex = Some(s.to_string());
    }
    Ok(())
}

fn apply_expiry(r: &mut ReaderResult) -> Result<(), MrzError> {
    let expiry = r.expiry_date.clone().unwrap_or_default();
    if expiry.chars().any(|c| c.is_ascii_alphabetic()) {
        r.expiry_date = Some(String::new());
    } else {
        r.expiry_date = Some(format!(
            "{}.{}.20{}",
            substr(&expiry, 4, 2)?,
            substr(&expiry, 2, 2)?,
            substr(&expiry, 0, 2)?
        ));
    }
    Ok(())
}

/// Asbt.UtilsV2 `StringExtensions.ToInt`: spaces are ignored, a sign is accepted,
/// letters throw `FormatException`.
pub fn to_int(value: &str) -> Result<i32, MrzError> {
    value.replace(' ', "").parse::<i32>().map_err(|_| MrzError::Format)
}

// ---- C# string-op shims (ASCII; the ID BOX sends ASCII only) ------------------------

/// `string.Substring(start, len)` with C# bounds (throws `ArgumentOutOfRangeException`).
fn substr(s: &str, start: usize, len: usize) -> Result<String, MrzError> {
    substr_i(s, start as i64, len as i64)
}

/// `Substring` with signed args, so a negative computed length maps to the C# throw.
fn substr_i(s: &str, start: i64, len: i64) -> Result<String, MrzError> {
    if start < 0 || len < 0 {
        return Err(MrzError::ArgumentOutOfRange);
    }
    let b = s.as_bytes();
    let (start, len) = (start as usize, len as usize);
    let end = start.checked_add(len).ok_or(MrzError::ArgumentOutOfRange)?;
    if end > b.len() {
        return Err(MrzError::ArgumentOutOfRange);
    }
    Ok(String::from_utf8_lossy(&b[start..end]).into_owned())
}

/// `string[i]` (throws `IndexOutOfRangeException`).
fn char_at(s: &str, i: usize) -> Result<char, MrzError> {
    s.as_bytes().get(i).map(|&b| b as char).ok_or(MrzError::IndexOutOfRange)
}

/// `string.IndexOf(pat, StringComparison.Ordinal)`: byte index, or -1.
fn index_of(s: &str, pat: &str) -> i64 {
    s.find(pat).map(|i| i as i64).unwrap_or(-1)
}

/// `string.IndexOfAny(['<','<'], startIndex)`: first `'<'` at or after `start`, or -1;
/// throws `ArgumentOutOfRangeException` when `start` is negative or past the end.
fn index_of_any_from(s: &str, start: i64) -> Result<i64, MrzError> {
    let b = s.as_bytes();
    if start < 0 || start as usize > b.len() {
        return Err(MrzError::ArgumentOutOfRange);
    }
    let start = start as usize;
    for (i, &c) in b[start..].iter().enumerate() {
        if c == b'<' {
            return Ok((start + i) as i64);
        }
    }
    Ok(-1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_int_matches_asbt_utils() {
        assert_eq!(to_int("2085"), Ok(2085));
        assert_eq!(to_int(" 20"), Ok(20));
        assert_eq!(to_int("1 2"), Ok(12));
        assert_eq!(to_int("+12"), Ok(12));
        assert_eq!(to_int("20AB"), Err(MrzError::Format));
    }

    #[test]
    fn mrz_length_error_carries_the_csharp_texts() {
        let e = MrzError::Length(87);
        assert_eq!(e.message(), Some(texts::MRZ_INVALID));
        assert_eq!(e.comment().as_deref(), Some("Берилган MRZ узунлиги 87 ҳато"));
        assert_eq!(e.answere_id(), Some(2));
    }

    // The quirky MRZ/century behaviour is pinned end-to-end against the real C# outputs
    // in tests/reference_fixture.rs (the `cmrzString` / `cmrzChip` cases), not with
    // hand-written expectations here.
}
