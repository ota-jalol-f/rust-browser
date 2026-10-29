//! Every user-visible text of the reference `FormCaptureDocument`, copied from
//! `Asbt.Photo.Plugin/Document/DocumentPhotoTexts.cs` byte-for-byte.
//!
//! These are the C# originals; do NOT "fix" the spelling. The reference's own file
//! is generated from the pv-winform source bytes, so the inconsistencies are real:
//! - plain **Х** (U+0425) in `CAPTURED_NULL_TEXT`, `OVERLAY_DOCUMENT`, `OVERLAY_OWNER`
//!   next to **Ҳ** (U+04B2) in `TITLE_PREFIX`;
//! - a **Latin C** (U+0043) starting `DOWNLOAD_ERROR` ("Cуратга…");
//! - no space after `(v.` in `TITLE_PREFIX` (the canon-face form had `(v. `).
//!
//! A test pins these homoglyphs.

/// `FormCaptureDocument.cs:33`, `this.Text = TitlePrefix + ProductVersion + ")"`.
/// No space after `v.`.
pub const TITLE_PREFIX: &str = "Ҳужжатни фотосуратга олиш (v.";

/// `pCameraLive` NullText.
pub const LIVE_NULL_TEXT: &str = "Камерадан видео лавха";

/// `pCameraCaptured` NullText. Plain Х (U+0425), not Ҳ.
pub const CAPTURED_NULL_TEXT: &str = "Хужжатнинг олинган фотосурати";

/// `pLabelLine5.Text`.
pub const CAPTION_LINE: &str = "ID карта тизими";

/// `sbCapture.Text`.
pub const CAPTURE: &str = "Фотосуратни олиш";

/// `sbSave.Text`.
pub const SAVE: &str = "Сақлаш";

/// Drawn in the green rectangle (80,330,300,215).
pub const OVERLAY_DOCUMENT: &str = "Асос хужжат";

/// Drawn in the green ellipse (140,100,180,250).
pub const OVERLAY_OWNER: &str = "Хужжат эгаси";

/// The `XtraMessageBox` caption of `AlertMessage.ShowMessage(..., Error)`.
pub const ERROR_CAPTION: &str = "Ҳато";

/// Ctor `catch (DllNotFoundException)`. Box + toast (no detail).
pub const CANON_FILES_NOT_FOUND: &str = "Canon фотокамера файллари топилмади";

/// Ctor `catch (Exception)`. Box only (detail given).
pub const CAMERA_FILES_START_ERROR: &str = "Камера файлларини ишга туширишда ҳато";

/// `InitCamera` catch. Box only.
pub const CAMERA_START_ERROR: &str = "Камера ишга туширишда ҳато";

/// `MainCamera_StateChanged` catch. Box only.
pub const CAMERA_STATE_ERROR: &str = "Камера холатини ўзгартиришда ҳато";

/// `OpenSession` with an empty camera list. Box + toast.
pub const CAMERA_NOT_FOUND: &str = "Фотокамера топилмади";

/// `MainCamera_LiveViewUpdated` catch. Box only.
pub const LIVE_VIEW_ERROR: &str = "Камера видео кўрсатишида ҳато";

/// `MainCamera_DownloadReady` catch: a TOAST with the exception text. The first
/// letter is LATIN C (U+0043) in the C# source; keep it.
pub const DOWNLOAD_ERROR: &str = "Cуратга олишда ҳато";

/// `sbSave_Click` with an empty captured pane. Box + toast.
pub const NO_PHOTO: &str = "Фотосурат олинмаган";

/// `FormClosing` catch. Box only.
pub const CAMERA_CLOSE_ERROR: &str = "Камера ёпиш жараёнида ҳато";

/// `pCameraEnroll_Click` catch. Box only.
pub const CAPTURE_ERROR: &str = "Фотосурат олиш жараёнида ҳато";

/// The re-entrancy message returned when a document window is already open. This
/// is **not** a C# text (C# could not re-enter its modal loop); it follows the
/// pattern of the idbox / signpad busy messages, and needs user sign-off.
pub const DOCUMENT_BUSY: &str = "Ҳужжатни фотосуратга олиш ойнаси аллақачон очиқ";
