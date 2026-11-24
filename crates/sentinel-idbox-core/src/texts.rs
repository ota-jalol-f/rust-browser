//! Every user-visible text of pv-winform IDCard/Asbt.IdBox `FormReader`, byte-for-byte
//! (UTF-8). Do **not** "fix" spelling or mixed scripts: `ҳато`, `кетти`, `аниклашда`,
//! `қўрсатишда` (DG2) vs `кўрсатишда` (DG6) and the English "Reading MRZ..." /
//! "Please connect an Elyctis reader." are the C# originals. A test checks that every
//! constant here is a string literal of the C# sources
//! (`tests/fixtures/reference.json`, `sourceLiterals`). Ported from the reference
//! `Asbt.IdBox.Shared.IdBoxTexts`.

// ---- window -------------------------------------------------------------------------

/// Set on Load (the window title "Ҳужжат чипини ўқиш...").
pub const WINDOW_TITLE: &str = "Ҳужжат чипини ўқиш...";
/// The Designer title, overridden on Load; kept for reference only.
pub const DESIGNER_TITLE: &str = "Маълумотларни ўқиш...";
/// Initial status label.
pub const INITIAL_STATUS: &str = "Ўқишни бошлаш...";
/// Reading-data group caption.
pub const GROUP_READING: &str = "Ўқиш маълумотлари...";
/// "MRZ аниқлаш" label.
pub const LABEL_MRZ_DETECTED: &str = "MRZ аниқлаш";
/// "MRZ ўқиш" label.
pub const LABEL_MRZ_READ: &str = "MRZ ўқиш";
/// "Чипни аниқлаш" label.
pub const LABEL_CHIP_DETECTED: &str = "Чипни аниқлаш";
/// "Чипни ўқиш" label.
pub const LABEL_CHIP_READ: &str = "Чипни ўқиш";
/// MRZ-lines group caption.
pub const GROUP_MRZ_LINES: &str = "Ҳужжатнинг MRZ қаторлари";

// ---- alerts / message boxes ---------------------------------------------------------

/// Toast caption when the SCard manager / antenna selector fails (ctor).
pub const READER_NOT_CONNECTED: &str = "Ўқиш қурилмаси уланмаган";
/// MessageBox text (connect failure) and toast caption (port lookup failure).
pub const DEVICE_NOT_FOUND: &str = "Қурилма топилмади";
/// `ShowMessage` alert detail that C# only logs (never displays).
pub const PORT_ERROR_DETAIL: &str = "Порт да ҳато, илтимос тўғри портни белгиланг";
/// Caption of error message boxes ("Ҳато").
pub const ERROR_CAPTION: &str = "Ҳато";
/// Toast text when the port lookup throws.
pub const PORT_LOOKUP_ERROR: &str = "Қурилма топилмади, ёки порт аниклашда ҳато";
/// Toast caption when the MRZ cannot be marshalled to the UI thread.
pub const PORT_FAULT: &str = "Қурилма портида ҳато";
/// Toast text for [`PORT_FAULT`].
pub const PORT_FAULT_DETAIL: &str = "Қурилма портида ҳато, илтимос тўғри портни белгиланг";
/// Toast caption (Warning) when the MRZ cannot be parsed.
pub const DOCUMENT_NOT_PLACED: &str = "Ҳужжат ўқиш қурилмасига қўйилмаган";
/// Toast text for [`DOCUMENT_NOT_PLACED`].
pub const DOCUMENT_NOT_PLACED_DETAIL: &str = "Илтимос, ҳужжатни ўқиш қурилмасига қўйинг";
/// Toast caption when `connect` throws.
pub const CONNECT_ERROR: &str = "Қурилмага уланишда ҳато";
/// Toast caption when closing the reader fails.
pub const READER_DISCONNECT_ERROR: &str = "Ўқиш қурилмасини узишда ҳато";
/// Alert caption after a successful read (body is the Answere JSON).
pub const READING_DONE: &str = "Ҳужжатни ўқиш якунланди";

// ---- status line --------------------------------------------------------------------

/// The "already connected" branch of ReadClick.
pub const READING_MRZ_AGAIN: &str = "MRZ ни ўқиш...";
/// English in C#.
pub const READING_MRZ: &str = "Reading MRZ...";
/// MRZ read OK.
pub const MRZ_READ_OK: &str = "MRZ ўқилди";
/// MRZ check-digit error (red).
pub const MRZ_CHECKSUM_ERROR: &str = "MRZ махсус рақамларида ҳато";
/// Detecting the chip.
pub const DETECTING_CHIP: &str = "Чипни аниқлаш...";
/// Card read timed out (red).
pub const CARD_TIMEOUT: &str = "Картани ўқиш вақти ўтиб кетти";
/// Card not found (red).
pub const CARD_NOT_FOUND: &str = "Карта топилмади";
/// English in C# (red).
pub const CONNECT_ELYCTIS_READER: &str = "Please connect an Elyctis reader.";
/// Reading the document.
pub const READING_DOCUMENT: &str = "Ҳужжатни ўқиш...";
/// Connecting to the device.
pub const CONNECTING: &str = "Қурилмага уланиш...";
/// `connect` returned -1.
pub const APPLET_ERROR: &str = "Қурилма номи ёки аплет турида ҳато";
/// Connected successfully.
pub const CONNECTED: &str = "Уланиш муваффақиятли";
/// Front antenna selected.
pub const FRONT_ANTENNA: &str = "Олдинги антенна танланди...";
/// Back antenna selected.
pub const BACK_ANTENNA: &str = "Орқа антенна танланди...";
/// Connecting to BAC.
pub const BAC_CONNECTING: &str = "BAC га уланиш...";
/// Connected to BAC.
pub const BAC_CONNECTED: &str = "BAC га уланди";
/// BAC connection error.
pub const BAC_ERROR: &str = "BAC га уланишда ҳато";
/// Connecting to BAP.
pub const BAP_CONNECTING: &str = "BAP га уланиш...";
/// Connected to BAP.
pub const BAP_CONNECTED: &str = "BAP га уланди";
/// BAP connection error.
pub const BAP_ERROR: &str = "BAP га уланишда ҳато";
/// Disconnected (finally of the chip read).
pub const DISCONNECTED: &str = "Узилди";
/// Cloning the chip photo failed (red).
pub const PHOTO_CLONE_ERROR: &str = "Фотосуратни олишда ҳато";
/// Reading DG1.
pub const DG1_READING: &str = "DG1 ўқиш...";
/// DG1 read.
pub const DG1_READ: &str = "DG1 ўқилди";
/// DG1 read error.
pub const DG1_ERROR: &str = "DG1 ўқишда ҳато";
/// The exception message that ends the read (-2).
pub const DG1_NOT_READ: &str = "DG1 ўқилмади";
/// Reading DG2.
pub const DG2_READING: &str = "DG2 ўқиш...";
/// DG2 read.
pub const DG2_READ: &str = "DG2 ўқилди";
/// DG2 photo error — note `қ`, unlike DG6.
pub const DG2_PHOTO_ERROR: &str = "Фотосуратни қўрсатишда ҳато";
/// DG2 read error.
pub const DG2_ERROR: &str = "DG2 ўқишда ҳато";
/// Reading DG6.
pub const DG6_READING: &str = "DG6 ўқиш...";
/// DG6 read.
pub const DG6_READ: &str = "DG6 ўқилди";
/// DG6 photo error — note `к`, unlike DG2.
pub const DG6_PHOTO_ERROR: &str = "Фотосуратни кўрсатишда ҳато";
/// DG6 read error.
pub const DG6_ERROR: &str = "DG6 ўқиш жараёнида ҳато";

// ---- result -------------------------------------------------------------------------

/// `readerResult` before a successful read.
pub const NO_DATA: &str = "Маълумотлар олинмаган";
/// AnswereMessage of a successful read.
pub const PASSPORT_READ: &str = "Паспорт ўқилди";
/// AnswereComment of the chip path.
pub const FROM_CHIP: &str = "Маълумотлар чипдан";
/// AnswereComment of the MRZ fallback.
pub const FROM_MRZ: &str = "Маълумотлар MRZ дан";
/// AsbtException message when the MRZ fallback text is not 88/90 long.
pub const MRZ_INVALID: &str = "MRZ ҳато берилган";
/// Its comment, a format string with one `{}` for the length.
pub const MRZ_LENGTH_INVALID_FORMAT: &str = "Берилган MRZ узунлиги {} ҳато";

// ---- PersonSearchForm (shown by the Vue side, listed for reference) -----------------

/// Reader button text.
pub const SEARCH_READER_BUTTON: &str = "Ҳужжатни ўқиш қурилмасига қўйинг ва шу тугмани босинг";
/// Toast caption before the reader starts.
pub const SEARCH_IDBOX_STARTING: &str = "Id Box ни ишга тушириш";
/// Toast text for [`SEARCH_IDBOX_STARTING`].
pub const SEARCH_IDBOX_STARTING_DETAIL: &str = "Чипни ўқиш қурилмасини ишга тушириш";
/// MessageBox text when the reader throws.
pub const SEARCH_READ_ERROR: &str = "Биометрик паспорт маълумотларни ўқишда ҳато";

/// Window-busy message (ReadDocument re-entered). Not a C# text — C# could not re-enter.
pub const BUSY_MESSAGE: &str = "Ҳужжат чипини ўқиш ойнаси аллақачон очиқ";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dg2_and_dg6_photo_errors_differ_in_one_letter() {
        assert_eq!(DG2_PHOTO_ERROR, "Фотосуратни қўрсатишда ҳато");
        assert_eq!(DG6_PHOTO_ERROR, "Фотосуратни кўрсатишда ҳато");
        assert_eq!(DG2_PHOTO_ERROR.chars().nth(12), Some('қ'));
        assert_eq!(DG6_PHOTO_ERROR.chars().nth(12), Some('к'));
    }

    #[test]
    fn english_texts_stay_english() {
        assert_eq!(READING_MRZ, "Reading MRZ...");
        assert_eq!(CONNECT_ELYCTIS_READER, "Please connect an Elyctis reader.");
    }
}
