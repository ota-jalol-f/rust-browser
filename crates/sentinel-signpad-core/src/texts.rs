//! Every user-visible text of the reference `FormSignature`, copied from
//! `Asbt.SignPad.Plugin/SignPadTexts.cs` byte-for-byte (UTF-8).
//!
//! These are the C# originals; do not "fix" the spelling — `ҳато` (ҳ U+04B3),
//! `Мурожат`, `Сақлаш` (қ U+049B) are as pv-winform wrote them. A test pins the
//! look-alikes (ҳ/х/h, қ/к/k).

/// `FormSignature.Designer.cs:118` — the window title.
pub const WINDOW_TITLE: &str = "Мурожат қилувчининг имзосини олиш";

/// `FormSignature.Designer.cs:63` — `pLabelLine5.Text`.
pub const CAPTION_LINE: &str = "ID карта тизими";

/// `FormSignature.Designer.cs:73` — `sbClear.Text`.
pub const CLEAR: &str = "Тозалаш";

/// `FormSignature.Designer.cs:96` — `sbSave.Text`.
pub const SAVE: &str = "Сақлаш";

/// `FormSignature.cs:31` — the too-small MessageBox text.
pub const TOO_SMALL_MESSAGE: &str = "Фуқаро имзоси ҳато олинган, уни қайта олиш лозим";

/// `FormSignature.cs:31` — the too-small MessageBox caption.
pub const TOO_SMALL_CAPTION: &str = "Ҳато";

/// The re-entrancy message returned when a signature window is already open
/// (`SignPadObj.BusyMessage`).
pub const BUSY: &str = "Имзо олиш ойнаси аллақачон очиқ";
