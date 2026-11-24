//! `FormReader.backgroundWorker_RunWorkerCompleted`: the C# three-way outcome
//! (reference `Asbt.IdBox.Plugin.Chip.ChipCompletion`).
//!
//! 1. `code` null (connect -1, unknown applet, or the helper unavailable) → MRZ fallback:
//!    `CMRZ(line1 + line2 + line3)`, "Маълумотлар MRZ дан", OK.
//! 2. Chip read (ICAO with 10 DG1 values) → `CMRZ(ReaderResult from DG1 + DG2 photo)`,
//!    "Маълумотлар чипдан", OK.
//! 3. Anything else → the window stays open with a red status ([`Outcome::StayOpen`]).
//!
//! Returns the plugin-data JSON for OK, or stays open. Errors where C# throws (the
//! builder on empty/TD1 input, a missing IDl category list): the caller shows the
//! message in red instead of crashing.

use crate::answere;
use crate::mrz::MrzLines;
use crate::protocol::HelperMessage;
use crate::reader_result::{MrzError, ReaderResult, ReaderResultBuilder, ReadingType};
use crate::texts;

const ICAO: i32 = 0;
const IDL: i32 = 1;

/// `FormReader`'s four 16x16 status icons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IdBoxIcon {
    /// "MRZ аниқлаш".
    MrzDetected,
    /// "MRZ ўқиш".
    MrzRead,
    /// "Чипни аниқлаш".
    AntennaSelected,
    /// "Чипни ўқиш".
    ChipRead,
}

/// Icon images.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconState {
    /// The resx EditValue.
    Initial,
    /// In progress.
    Loading,
    /// Success.
    Tick,
    /// Failure.
    Cross,
    /// Warning.
    Caution,
}

/// What the read does to the front-end. In the no-modal design these are status events
/// pushed through the plugin API (not a window); only the methods the completion uses
/// are required here, the rest serve the orchestration.
pub trait View {
    /// Status line (colour unchanged).
    fn set_status(&mut self, text: &str);
    /// Status line in red (stays red).
    fn set_status_error(&mut self, text: &str);
    /// A status icon's state.
    fn set_icon(&mut self, icon: IdBoxIcon, state: IconState);
    /// The two MRZ lines.
    fn set_mrz_lines(&mut self, line1: &str, line2: &str);
    /// A toast (caption + text).
    fn show_toast(&mut self, caption: &str, text: &str);
    /// A modal error message box.
    fn show_error_message(&mut self, text: &str, caption: &str);
    /// Finish with the plugin-data JSON (`DialogResult = OK`).
    fn close_ok(&mut self, data_json: &str);
    /// Finish with no data (`DialogResult = Cancel`).
    fn close_cancel(&mut self);
}

/// Where C# throws (an unhandled exception leaves the window open): a builder failure or
/// the IDl missing-category-list `NullReferenceException`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompletionError {
    /// A `CMRZ` builder failure (the reference exception is preserved).
    Mrz(MrzError),
    /// C# `System.NullReferenceException` (IDl with no category list).
    NullReference,
}

impl From<MrzError> for CompletionError {
    fn from(e: MrzError) -> Self {
        CompletionError::Mrz(e)
    }
}

/// The completion result: finish with JSON, or stay open (a red status was set).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// OK: the plugin-data JSON to resolve `ReadDocument` with.
    Ok(String),
    /// The read failed; the window (front-end) stays open with a red status.
    StayOpen,
}

/// `ChipCompletion.Complete`. `reading_seconds` is `DocumentReadingTime`.
pub fn complete(
    res: &HelperMessage,
    mrz: &MrzLines,
    reading_seconds: f64,
    builder: &ReaderResultBuilder,
    view: &mut dyn View,
) -> Result<Outcome, CompletionError> {
    if let Some(fault) = &res.fault {
        view.set_icon(IdBoxIcon::ChipRead, IconState::Cross);
        view.set_status_error(fault);
        return Ok(Outcome::StayOpen);
    }

    if res.code.is_none() {
        let mut from_mrz = builder.from_mrz(&format!(
            "{}{}{}",
            mrz.line1(),
            mrz.line2(),
            mrz.line3()
        ))?;
        from_mrz.document_reading_time = reading_seconds;
        return Ok(Outcome::Ok(answere::to_plugin_data(
            1,
            texts::PASSPORT_READ,
            texts::FROM_MRZ,
            &from_mrz,
        )));
    }

    // FormReader's Box* fields: "" until DG1 fills them.
    let (mut box_name, mut box_surname, mut box_birth_date, mut box_nationality, mut box_sex) =
        (String::new(), String::new(), String::new(), String::new(), String::new());
    let (mut box_expiry_date, mut box_document_number, mut box_document_type, mut box_issuer, mut box_optional_data) =
        (String::new(), String::new(), String::new(), String::new(), String::new());
    let mut box_picture: Option<String> = None;

    let applet_type = res.applet_type.unwrap_or(ICAO);
    let dg1 = res.dg1.clone().unwrap_or_default();

    let code = res.code.unwrap();
    if code == -1 {
        view.set_icon(IdBoxIcon::ChipRead, IconState::Cross);
        view.set_status_error(res.message.as_deref().unwrap_or(""));
        // C# falls through to CMRZ with empty Box fields and throws there (Pinpp[0] of "").
        return Ok(Outcome::StayOpen);
    }

    if code == -2 {
        view.set_status(res.message.as_deref().unwrap_or(""));
    }

    if applet_type == ICAO {
        if dg1.len() == 10 {
            box_name = dg1[0].clone();
            box_surname = dg1[1].clone();
            box_birth_date = dg1[2].clone();
            box_nationality = dg1[3].clone();
            box_sex = dg1[4].clone();
            box_expiry_date = dg1[5].clone();
            box_document_number = dg1[6].clone();
            box_document_type = dg1[7].clone();
            box_issuer = dg1[8].clone();
            box_optional_data = dg1[9].clone();

            box_picture = res.photo.clone();
            if res.photo_error == Some(true) {
                view.set_status_error(texts::PHOTO_CLONE_ERROR);
            }
        } else {
            view.set_icon(IdBoxIcon::ChipRead, IconState::Cross);
            view.set_status_error(res.message.as_deref().unwrap_or(""));
            return Ok(Outcome::StayOpen);
        }
    } else if applet_type == IDL {
        // dg1Categories.Clear() throws for null.
        if res.categories_read != Some(true) {
            return Err(CompletionError::NullReference);
        }

        box_picture = res.photo.clone();
        if res.photo_error == Some(true) {
            view.set_status_error(texts::PHOTO_CLONE_ERROR);
        }
    }

    view.set_icon(IdBoxIcon::ChipRead, IconState::Tick);

    // For IDl the ICAO Box fields are still "": the builder throws on Pinpp[0], as in C#.
    let from_chip = builder.from_chip(ReaderResult {
        additional: Some(String::new()),
        birth_date: Some(box_birth_date),
        document_number: Some(box_document_number),
        document_type: Some(box_document_type),
        expiry_date: Some(box_expiry_date),
        issuer: Some(box_issuer),
        nationality: Some(box_nationality),
        name: Some(box_name),
        pinpp: Some(box_optional_data),
        sex: Some(box_sex),
        surname: Some(box_surname),
        line1: Some(mrz.line1().to_string()),
        line2: Some(mrz.line2().to_string()),
        line3: Some(mrz.line3().to_string()),
        person_photo: box_picture,
        document_reading_time: reading_seconds,
        information_from: ReadingType::Chip,
        ..Default::default()
    })?;

    Ok(Outcome::Ok(answere::to_plugin_data(
        1,
        texts::PASSPORT_READ,
        texts::FROM_CHIP,
        &from_chip,
    )))
}
