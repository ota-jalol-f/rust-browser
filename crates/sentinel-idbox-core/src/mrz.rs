//! pv-winform Asbt.IdBox `MRZ.cs`, line by line: split the text the ID BOX scanner
//! delivers (lines end with `"\r"`, which `Scanner.ReadMRZ` turned into `"\r\n"`) and
//! build `MRZ_info`, the BAC key seed (document number, birth date, expiry date, each
//! with its check digit). Fails exactly where C# throws (fewer than two lines →
//! `IndexOutOfRangeException` → [`MrzError::IndexOutOfRange`]); `FormReader.readMRZ`
//! turns that into the "Ҳужжат ўқиш қурилмасига қўйилмаган" warning. The only change:
//! the check digits go through [`crate::checksum`] instead of the x86
//! `ElyMRTDDotNet.calculateChecksum`. Ported from `Asbt.IdBox.Plugin.Mrz.MrzLines`.

use crate::checksum;
use crate::reader_result::MrzError;

/// The parsed MRZ lines and the derived `MRZ_info`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MrzLines {
    line1: String,
    line2: String,
    line3: String,
    checksum: bool,
    mrz_info: Option<String>,
}

impl MrzLines {
    /// Parse the scanner text. `Err(MrzError::IndexOutOfRange)` when there are fewer
    /// than two lines (C#'s `lines[1]` throw).
    pub fn new(mrz: &str) -> Result<Self, MrzError> {
        let trimmed = mrz.trim_end_matches('\0').trim_end_matches('\r');
        let lines: Vec<&str> = trimmed.split('\r').collect();

        // C# reads lines[0] then lines[1] unconditionally; a single line throws here.
        let line1_raw = lines[0];
        let line2_raw = *lines.get(1).ok_or(MrzError::IndexOutOfRange)?;

        let line1 = trim_cr_nl(line1_raw);
        let line2 = trim_cr_nl(line2_raw);

        let mut this = MrzLines {
            line1,
            line2,
            line3: String::new(),
            checksum: false,
            mrz_info: None,
        };

        if lines.len() > 2 && line2_raw.len() < 34 {
            this.line3 = trim_cr_nl(lines[2]);
            this.extract_mrz_info(3);
        } else {
            // lines.len() > 1 always holds here (lines[1] succeeded).
            this.extract_mrz_info(2);
        }

        Ok(this)
    }

    /// `MRZ.Line1` (`pMRZ1.Text`).
    pub fn line1(&self) -> &str {
        &self.line1
    }

    /// `MRZ.Line2` (`pMRZ2.Text`).
    pub fn line2(&self) -> &str {
        &self.line2
    }

    /// `MRZ.Line3` (empty for two-line documents).
    pub fn line3(&self) -> &str {
        &self.line3
    }

    /// `MRZ.checksum`: all three check digits valid.
    pub fn checksum(&self) -> bool {
        self.checksum
    }

    /// `MRZ.MRZ_info`; `None` when a line is too short (C# then fails on `mrz_info.Length`).
    pub fn mrz_info(&self) -> Option<&str> {
        self.mrz_info.as_deref()
    }

    fn extract_mrz_info(&mut self, nblines: i32) {
        if nblines == 3 {
            if self.line1.len() > 14 && self.line2.len() > 14 {
                let a = slice(&self.line1, 5, 15);
                let b = slice(&self.line2, 0, 7);
                let c = slice(&self.line2, 8, 15);
                self.checksum = checksum::verify(&a) && checksum::verify(&b) && checksum::verify(&c);
                self.mrz_info = Some(format!("{a}{b}{c}"));
            }
        } else if nblines == 2 {
            if self.line2.len() > 27 {
                let a = slice(&self.line2, 0, 10);
                let b = slice(&self.line2, 13, 20);
                let c = slice(&self.line2, 21, 28);
                self.checksum = checksum::verify(&a) && checksum::verify(&b) && checksum::verify(&c);
                self.mrz_info = Some(format!("{a}{b}{c}"));
            }
        } else if self.line1.len() > 29 {
            self.mrz_info = Some(slice(&self.line1, 1, 29));
        }
    }
}

/// `string.Trim('\r').Trim('\n')`: strip `'\r'` from both ends, then `'\n'`.
fn trim_cr_nl(s: &str) -> String {
    s.trim_matches('\r').trim_matches('\n').to_string()
}

/// `s[start..end]` on ASCII (byte range; MRZ is ASCII). Callers guard the length first.
fn slice(s: &str, start: usize, end: usize) -> String {
    String::from_utf8_lossy(&s.as_bytes()[start..end]).into_owned()
}
