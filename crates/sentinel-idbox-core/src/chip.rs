//! The helper's port of `FormReader.backgroundWorker_DoWork` + `getInfosDG1/DG2/DG6`
//! (reference `Asbt.IdBox.Shared.ChipReadJob`), line by line, against the [`ElyMrtd`]
//! seam. Runs the chip read and streams `status`/`alert` lines through a `send`
//! callback, returning the single terminal `result` [`HelperMessage`].
//!
//! The dead branches stay dead (checkPA/AA/CA/TA and DG3/4/11/12 were constant-false in
//! C#). The result mirrors `e.Result` plus the form fields the completion reads (`dg1`,
//! `dg2`/`dg6`, `appletType`).

use crate::protocol::HelperMessage;
use crate::{errortext, texts};

const ICAO: i32 = 0;
const IDL: i32 = 1;

/// The `ElyMRTDDotNet` calls the chip read makes, one-to-one. The fallible members
/// return `Err(message)` where C# would throw; the plain getters cannot throw in the
/// read flow. Implemented by the real x86 SDK (`device` feature in the helper) and by
/// fakes in tests.
pub trait ElyMrtd {
    /// `connect(reader_name)`: the applet type (0 ICAO, 1 IDl) or -1.
    fn connect(&mut self, reader_name: &str) -> Result<i32, String>;
    /// `getSelectAntenna()`.
    fn get_select_antenna(&mut self) -> Result<Vec<u8>, String>;
    /// `establishBAC(mrz_info)`.
    fn establish_bac(&mut self, mrz_info: &str) -> Result<bool, String>;
    /// `readDG1()`.
    fn read_dg1(&mut self) -> Result<i32, String>;
    /// `readDG2()`.
    fn read_dg2(&mut self) -> Result<i32, String>;
    /// `readDG6()`.
    fn read_dg6(&mut self) -> Result<i32, String>;

    // ICAO DG1
    /// `getName()`.
    fn get_name(&mut self) -> String;
    /// `getSurname()`.
    fn get_surname(&mut self) -> String;
    /// `getBirthDate()`.
    fn get_birth_date(&mut self) -> String;
    /// `getNationality()`.
    fn get_nationality(&mut self) -> String;
    /// `getIssuingState()`.
    fn get_issuing_state(&mut self) -> String;
    /// `getCountryName(code)`.
    fn get_country_name(&mut self, code: &str) -> String;
    /// `getSex()`.
    fn get_sex(&mut self) -> String;
    /// `getValidityDate()`.
    fn get_validity_date(&mut self) -> String;
    /// `getDocNum()`.
    fn get_doc_num(&mut self) -> String;
    /// `getDocumentType()`.
    fn get_document_type(&mut self) -> String;
    /// `getOptionalData()`.
    fn get_optional_data(&mut self) -> String;

    // IDl DG1
    /// `getFamilyName()`.
    fn get_family_name(&mut self) -> String;
    /// `getGivenNames()`.
    fn get_given_names(&mut self) -> String;
    /// `getExpiryDate()`.
    fn get_expiry_date(&mut self) -> String;
    /// `getIssuingDate()`.
    fn get_issuing_date(&mut self) -> String;
    /// `getIssuingAuthority()`.
    fn get_issuing_authority(&mut self) -> String;
    /// `getIssuingCountry()`.
    fn get_issuing_country(&mut self) -> String;
    /// `getLicenceNumber()`.
    fn get_licence_number(&mut self) -> String;

    /// `getCategories()`: `None` is C#'s null (the IDl path then throws `ArgumentNullException`).
    fn get_categories(&mut self) -> Result<Option<Vec<String>>, String>;

    /// `getPhoto()`: the raw DG2/DG6 image, or `None`.
    fn get_photo(&mut self) -> Result<Option<Vec<u8>>, String>;

    /// `disconnect()`.
    fn disconnect(&mut self) -> Result<(), String>;
}

/// C#: `new Bitmap(new FreeImageBitmap(ms).ToBitmap())` and, for the result, the Bitmap
/// itself re-encoded as JPEG. The intermediate is opaque bytes the decoder owns.
pub trait PhotoDecoder {
    /// Decode `getPhoto()`'s bytes; `None` when FreeImage cannot (→ `Dg2/Dg6PhotoError`).
    fn decode(&self, photo: &[u8]) -> Option<Vec<u8>>;
    /// The decoded photo as base64 JPEG; `None` when re-encoding fails (→ `photoError`).
    fn to_jpeg_base64(&self, decoded: &[u8]) -> Option<String>;
}

/// One chip read against an [`ElyMrtd`]. Build with [`ChipReadJob::new`] and drive with
/// [`ChipReadJob::run`].
pub struct ChipReadJob<'a> {
    ely: &'a mut dyn ElyMrtd,
    decoder: &'a dyn PhotoDecoder,
    request_id: i32,
    send: &'a mut dyn FnMut(HelperMessage),

    applet_type: i32,
    dg1: Vec<String>,
    dg1_categories: Option<Vec<String>>,
    dg2: Option<Vec<u8>>,
    dg6: Option<Vec<u8>>,
    mrz_info: String,
}

impl<'a> ChipReadJob<'a> {
    /// A job for `request_id`, sending its status/alert lines through `send`.
    pub fn new(
        ely: &'a mut dyn ElyMrtd,
        decoder: &'a dyn PhotoDecoder,
        request_id: i32,
        send: &'a mut dyn FnMut(HelperMessage),
    ) -> Self {
        ChipReadJob {
            ely,
            decoder,
            request_id,
            send,
            // appletType starts at 0 (ICAO) and keeps it when connect throws, as in C#.
            applet_type: 0,
            dg1: Vec::new(),
            dg1_categories: None,
            dg2: None,
            dg6: None,
            mrz_info: String::new(),
        }
    }

    /// `backgroundWorker_DoWork`, with C#'s outer safety net: an escaped error (e.g. a
    /// disconnect failure) becomes a `result` with `fault` set.
    pub fn run(&mut self, reader: &str, mrz_info: &str) -> HelperMessage {
        self.mrz_info = mrz_info.to_string();
        match self.do_work(reader) {
            Ok(msg) => msg,
            Err(e) => {
                let mut result = self.result(None, None);
                result.fault = Some(e);
                result
            }
        }
    }

    fn do_work(&mut self, reader: &str) -> Result<HelperMessage, String> {
        self.status(texts::CONNECTING);

        match self.ely.connect(reader) {
            Ok(t) => self.applet_type = t,
            Err(err) => {
                let id = self.request_id;
                (self.send)(HelperMessage::alert(id, texts::CONNECT_ERROR, &errortext::get_all_messages(&[&err])));
            }
        }

        if self.applet_type == -1 {
            // e.Result stays null -> the completion takes the MRZ fallback. No "Узилди":
            // the return is before the try/finally.
            self.status(texts::APPLET_ERROR);
            return Ok(self.result(None, None));
        }

        // OUTER try (catch -> code = -1).
        let (code, message) = match self.outer_body() {
            Ok((c, m)) => (c, m),
            Err(e) => (Some(-1), Some(e)),
        };

        // finally: disconnect + "Узилди". A disconnect failure escapes DoWork (-> fault).
        self.ely.disconnect()?;
        self.status(texts::DISCONNECTED);

        Ok(self.result(code, message))
    }

    /// The OUTER try body: antenna + the applet branch. `Err` is the outer (-1) case;
    /// `Ok((code, message))` carries the inner outcome (0, -2, or null for an unknown applet).
    fn outer_body(&mut self) -> Result<(Option<i32>, Option<String>), String> {
        self.status(texts::CONNECTED);

        let antenna = self.ely.get_select_antenna()?;
        match antenna.first() {
            Some(0) => self.status(texts::FRONT_ANTENNA),
            Some(1) => self.status(texts::BACK_ANTENNA),
            _ => {}
        }

        // INNER try (catch -> code = -2).
        match self.inner_branch() {
            Ok(true) => Ok((Some(0), None)),
            Ok(false) => Ok((None, None)), // unknown applet: e.Result null, as in C#
            Err(e) => Ok((Some(-2), Some(e))),
        }
    }

    /// The applet branch. `Ok(true)` = a known applet was read; `Ok(false)` = unknown.
    fn inner_branch(&mut self) -> Result<bool, String> {
        let info = self.mrz_info.clone();
        if self.applet_type == ICAO {
            self.status(texts::BAC_CONNECTING);
            let bac = self.ely.establish_bac(&info)?;
            self.status(if bac { texts::BAC_CONNECTED } else { texts::BAC_ERROR });
            self.get_infos_dg1()?;
            self.get_infos_dg2()?;
            Ok(true)
        } else if self.applet_type == IDL {
            self.status(texts::BAP_CONNECTING);
            let bac = self.ely.establish_bac(&info)?;
            self.status(if bac { texts::BAP_CONNECTED } else { texts::BAP_ERROR });
            self.get_infos_dg1()?;
            self.get_infos_dg6()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn get_infos_dg1(&mut self) -> Result<(), String> {
        self.status(texts::DG1_READING);

        let i_res = self.ely.read_dg1()?;
        if i_res >= 0 {
            self.dg1 = Vec::new();
            match self.applet_type {
                ICAO => {
                    self.dg1.push(self.ely.get_name());
                    self.dg1.push(self.ely.get_surname());
                    self.dg1.push(self.ely.get_birth_date());
                    let nat = self.ely.get_nationality();
                    self.dg1.push(self.ely.get_country_name(&nat));
                    self.dg1.push(self.ely.get_sex());
                    self.dg1.push(self.ely.get_validity_date());
                    self.dg1.push(self.ely.get_doc_num());
                    self.dg1.push(self.ely.get_document_type());
                    let issuing = self.ely.get_issuing_state();
                    self.dg1.push(self.ely.get_country_name(&issuing));
                    self.dg1.push(self.ely.get_optional_data());
                }
                IDL => {
                    self.dg1.push(self.ely.get_family_name());
                    self.dg1.push(self.ely.get_given_names());
                    self.dg1.push(self.ely.get_birth_date());
                    self.dg1.push(self.ely.get_expiry_date());
                    self.dg1.push(self.ely.get_issuing_date());
                    self.dg1.push(self.ely.get_issuing_authority());
                    self.dg1.push(self.ely.get_issuing_country());
                    self.dg1.push(self.ely.get_licence_number());

                    // new List<VehiculeCategorie>(getCategories()): ArgumentNullException for null.
                    match self.ely.get_categories()? {
                        Some(categories) => self.dg1_categories = Some(categories),
                        None => return Err("Value cannot be null.\r\nParameter name: collection".to_string()),
                    }
                }
                _ => {}
            }

            self.status(texts::DG1_READ);
            Ok(())
        } else {
            self.status(texts::DG1_ERROR);
            Err(texts::DG1_NOT_READ.to_string())
        }
    }

    fn get_infos_dg2(&mut self) -> Result<(), String> {
        self.status(texts::DG2_READING);

        let i_res = self.ely.read_dg2()?;
        if i_res >= 0 {
            if self.applet_type == ICAO {
                if let Some(photo) = self.ely.get_photo()? {
                    match self.decoder.decode(&photo) {
                        Some(decoded) => {
                            self.dg2 = Some(decoded);
                            self.status(texts::DG2_READ);
                        }
                        None => {
                            self.status(texts::DG2_PHOTO_ERROR);
                            return Err(texts::DG2_PHOTO_ERROR.to_string());
                        }
                    }
                }
            }
        } else {
            self.status(texts::DG2_ERROR);
        }
        Ok(())
    }

    fn get_infos_dg6(&mut self) -> Result<(), String> {
        self.status(texts::DG6_READING);

        let i_res = self.ely.read_dg6()?;
        if i_res >= 0 {
            if self.applet_type == IDL {
                if let Some(photo) = self.ely.get_photo()? {
                    match self.decoder.decode(&photo) {
                        Some(decoded) => {
                            self.dg6 = Some(decoded);
                            self.status(texts::DG6_READ);
                        }
                        None => {
                            self.status(texts::DG6_PHOTO_ERROR);
                            return Err(texts::DG6_PHOTO_ERROR.to_string());
                        }
                    }
                }
            }
        } else {
            self.status(texts::DG6_ERROR);
        }
        Ok(())
    }

    fn result(&self, code: Option<i32>, message: Option<String>) -> HelperMessage {
        let mut result = HelperMessage::result(self.request_id);
        result.code = code;
        result.message = message;
        result.applet_type = Some(self.applet_type);
        result.dg1 = Some(self.dg1.clone());
        result.categories_read = if self.applet_type == IDL {
            Some(self.dg1_categories.is_some())
        } else {
            None
        };

        // The completion clones dg2 (ICAO) or dg6 (IDl) into BoxPicture.
        let photo = if self.applet_type == IDL { self.dg6.as_ref() } else { self.dg2.as_ref() };
        if let Some(decoded) = photo {
            match self.decoder.to_jpeg_base64(decoded) {
                Some(b64) => result.photo = Some(b64),
                None => result.photo_error = Some(true),
            }
        }

        result
    }

    fn status(&mut self, text: &str) {
        let id = self.request_id;
        (self.send)(HelperMessage::status(id, text, false));
    }
}
