//! Every user-visible text of the reference `FormFingerprintsV2` (.cs + .Designer),
//! `WSQProcesses` and the `AlertMessage` captions, copied from
//! `Asbt.Fingerprints.Plugin/Enroll/EnrollTexts.cs` byte-for-byte (UTF-8).
//!
//! The C# originals keep their typos ("Қуримла", "ажратилмаёпти", "Иккта", the
//! left-hand "Button" caption); do not "fix" them. `EnrollTextsTests` pins the UTF-8
//! bytes of all 87.

// --- Form / tabs ---
/// `FormFingerprintsV2.Text`.
pub const WINDOW_TITLE: &str = "Бармоқ изларини олиш";
/// `tabPaneFingers.Text`.
pub const TAB_PANE_TEXT: &str = "Бармоқ изларини тахрири";
/// `tabNavigationPage1.Caption`.
pub const TAB_ENROLL: &str = "Бармоқ изларини олиш";
/// `tabNavigationPage3.Caption`.
pub const TAB_SETTINGS: &str = "Созлашлар";
/// `pPersonTitle1` (hidden behind the tab pane in C#).
pub const CAPTION_LINE: &str = "Паспорт-Виза";

// --- Page 1 ---
/// `MsgPanel` initial text.
pub const MSG_PANEL_INITIAL: &str = "Бармоқ изини олишни бошланишини кутинг";
/// `PreviewWindow` NullText.
pub const PREVIEW_NULL_TEXT: &str = "бармоқ изларини олиш жараёни";
/// `sbTakeAuto`.
pub const TAKE_AUTO: &str = "Бармоқ изини олишни бошлаш";
/// `sbStop`.
pub const STOP: &str = "Тўхтатиш";

/// Group "Бош бармоқлар".
pub const GROUP_THUMBS: &str = "Бош бармоқлар";
/// Group "Ўнг қўл".
pub const GROUP_RIGHT_HAND: &str = "Ўнг қўл";
/// Group "Чап қўл".
pub const GROUP_LEFT_HAND: &str = "Чап қўл";

/// Header button "Тозалаш".
pub const HEADER_CLEAR: &str = "Тозалаш";
/// Header button "Олиш".
pub const HEADER_TAKE: &str = "Олиш";
/// Left-hand button 1 — C# typo, kept.
pub const HEADER_TAKE_LEFT: &str = "Button";
/// Hint.
pub const HINT_CLEAR_THUMBS: &str = "Бош бармоқларни тозалаш";
/// Hint.
pub const HINT_TAKE_THUMBS: &str = "Иккта бош бармоқ изларини алохида олиш";
/// Hint.
pub const HINT_CLEAR_RIGHT: &str = "Ўнг қўл бармоқларини тозалаш";
/// Hint.
pub const HINT_TAKE_RIGHT: &str = "Ўнг қўл бармоқ изларини алохида олиш";
/// Hint.
pub const HINT_CLEAR_LEFT: &str = "Чап қўл бармоқларини тозалаш";
/// Hint.
pub const HINT_TAKE_LEFT: &str = "Чап қўл бармоқ изларини алохида олиш";

// Tile captions.
/// Tile caption.
pub const TILE_THUMB_RIGHT: &str = "ўнг";
/// Tile caption.
pub const TILE_THUMB_LEFT: &str = "чап";
/// Tile caption.
pub const TILE_INDEX: &str = "кўрсатгич";
/// Tile caption.
pub const TILE_MIDDLE: &str = "ўрта бармоқ";
/// Tile caption.
pub const TILE_RING: &str = "номсиз бармоқ";
/// Tile caption.
pub const TILE_LITTLE: &str = "жимжилоқ";

// Radial menu.
/// Menu hint.
pub const MENU_ENROLL_HINT: &str = "Бармоқни алохида олиш";
/// Menu tip title.
pub const MENU_ENROLL_TIP_TITLE: &str = "Бармоқ изини олиш";
/// Menu tip text.
pub const MENU_ENROLL_TIP_TEXT: &str = "Танланган бармоқ изини алохида олиш";
/// Menu hint.
pub const MENU_DAMAGED_HINT: &str = "Бармоқ шикастланган";
/// Menu tip title.
pub const MENU_DAMAGED_TIP_TITLE: &str = "Шикастланган бармоқ";
/// Menu tip text.
pub const MENU_DAMAGED_TIP_TEXT: &str = "Танланган бармоқ изини шикастланган қилиб белгилаш";
/// Menu hint.
pub const MENU_ABSENT_HINT: &str = "Бармоқ йўқ";
/// Menu tip title.
pub const MENU_ABSENT_TIP_TITLE: &str = "Бармоқ изи йўқ";
/// Menu tip text.
pub const MENU_ABSENT_TIP_TEXT: &str = "Танланган бармоқ изи йўқ";

// --- Page 2 ---
/// Label.
pub const CONNECTED_SCANNER: &str = "Уланган сканер";
/// Group.
pub const DEVICE_INFO_GROUP: &str = "Қурилма маълумотлари";
/// Field label.
pub const INFO_MODEL: &str = "Модель";
/// Field label.
pub const INFO_FIRMWARE: &str = "Прошивка";
/// Field label.
pub const INFO_HARDWARE: &str = "Қурилма";
/// Field label.
pub const INFO_SDK_VERSION: &str = "SDK версияси";
/// Field label.
pub const INFO_DEVICE_ID: &str = "Қурилма ID";
/// Button.
pub const CALIBRATE: &str = "Калибровка";
/// Slider caption.
pub const SENSITIVITY: &str = "Сезиларлиги";
/// Slider tick.
pub const SENSITIVITY_NORMAL: &str = "Нормал";
/// Slider tick.
pub const SENSITIVITY_HIGH: &str = "Юқори";
/// Slider tick.
pub const SENSITIVITY_HIGHER: &str = "Энг юқори";
/// Slider tick.
pub const SENSITIVITY_DISABLED: &str = "Блокланган";
/// Legend.
pub const LEGEND_MISSING: &str = "йўқ бармоқ";
/// Legend.
pub const LEGEND_DAMAGED: &str = "шикастланган бармоқ";
/// Legend.
pub const LEGEND_SCANNING: &str = "сканерлаш жараёнида";
/// Button.
pub const INIT_SDK: &str = "Дастурни тайёрлаш";
/// Button.
pub const INIT_DEVICE: &str = "Қурилмани тайёрлаш";
/// Button.
pub const EXIT_DEVICE: &str = "Қурилмани тўхтатиш";
/// Button (dead in C#).
pub const SAVE_TO_BMP: &str = "Файлга (bmp)";
/// Button (dead in C#).
pub const SAVE_TO_WSQ: &str = "Файлга (wsq)";
/// Button.
pub const SAVE_TO_JSON: &str = "Файлга (json)";
/// Save-file dialog title.
pub const SAVE_FILE_DIALOG_TITLE: &str = "Олинган бармоқ изларини файлга сақлаш";
/// Folder-dialog description (dead in C#).
pub const FOLDER_DIALOG_DESCRIPTION: &str = "WSQ форматда файлларга сақлаш";

// --- Bottom bar ---
/// "Сақлаш".
pub const SAVE: &str = "Сақлаш";

// --- Runtime messages ---
/// SDK loaded.
pub const SDK_LOADED: &str = "SDK мувафақиятли юкланди";
/// Device list item prefix.
pub const DEVICE_ITEM_PREFIX: &str = "Сканер қурилмаси ";
/// Device ready (sic: "Қуримла").
pub const DEVICE_READY: &str = "Қуримла ишлашга тайёр бўлди";
/// Device disconnected.
pub const DEVICE_DISCONNECTED: &str = "Қурилма узилди";
/// Capture succeeded.
pub const CAPTURE_SUCCEEDED: &str = "Бармоқ излари мувафақиятли олинди";
/// Overlay-failed status prefix (sic: "ажратилмаёпти").
pub const OVERLAY_FAILED_PREFIX: &str = "Бармоқ излари ажратилмаёпти ";
/// "Бармоқлар №:" prefix.
pub const FINGERS_NUMBER_PREFIX: &str = "Бармоқлар №:";
/// Put-right prefix.
pub const PUT_RIGHT_PREFIX: &str = "Ўнг қўлнинг ";
/// Put-left prefix.
pub const PUT_LEFT_PREFIX: &str = "Чап қўлнинг ";
/// Put-four suffix.
pub const PUT_FOUR_SUFFIX: &str = "та бармоғини сканерга қўйинг";
/// Put-thumbs prefix.
pub const PUT_THUMBS_PREFIX: &str = "Бош бармоқларнинг ";
/// Put-thumbs suffix.
pub const PUT_THUMBS_SUFFIX: &str = "тасини сканерга қўйинг";
/// Single-finger put suffix.
pub const PUT_SINGLE_SUFFIX: &str = " бармоқни сканерга қўйинг";
/// Scanner-setup toast caption.
pub const SCANNER_SETUP_CAPTION: &str = "Сканер созлаш";
/// Calibration done.
pub const CALIBRATE_DONE: &str = "Калибровка мувафақиятли якунланди";
/// Save guard — `AsbtException.AnswereComment`.
pub const FINGERPRINTS_INCOMPLETE: &str = "Бармоқ излари тўлиқ берилмаган";
/// Group-clear question (right).
pub const CLEAR_RIGHT_QUESTION: &str = "Олинган ўнг қўл бармоқ излари тозалансинми?";
/// Group-clear question (left).
pub const CLEAR_LEFT_QUESTION: &str = "Олинган чап қўл бармоқ излари тозалансинми?";
/// Group-clear question (thumbs).
pub const CLEAR_THUMBS_QUESTION: &str = "Олинган бош бармоқлар тозалансинми?";
/// Data saved (dead in C#).
pub const DATA_SAVED: &str = "Маълумот сақланди";

/// The `RemoveNullElements` oversize warning (`i > 18000`).
pub const WSQ_TOO_LARGE: &str =
    "Сурат ўлчами тавсия этилган ўлчамдан катта \n Ушбу бармоқни қайта сканер қилинг";

// --- AlertMessage captions ---
/// "Маълумот".
pub const CAPTION_INFO: &str = "Маълумот";
/// "Диққат".
pub const CAPTION_WARNING: &str = "Диққат";
/// "Ҳато".
pub const CAPTION_ERROR: &str = "Ҳато";
/// "Савол".
pub const CAPTION_QUESTION: &str = "Савол";
/// Alert details hint.
pub const ALERT_DETAILS_HINT: &str = "Тўлиқ маълумот қўриш";

/// `FingerPosition.ToStr()` for 0..10, the .NET Framework `Enum.ToString` of the
/// aliased enum (mixed: `RightIndex` but `RightRingFinger`). Index 0 is `Unknown`.
pub const FINGER_POSITION_NAMES: [&str; 11] = [
    "Unknown",
    "RightThumb",
    "RightIndex",
    "RightMiddle",
    "RightRingFinger",
    "RightLittleFinger",
    "LeftThumb",
    "LeftIndexFinger",
    "LeftMiddle",
    "LeftRingFinger",
    "LeftLittleFinger",
];

/// The printed name of a 0..10 single-finger position.
pub fn finger_position_name(number: i32) -> &'static str {
    FINGER_POSITION_NAMES.get(number as usize).copied().unwrap_or("Unknown")
}
