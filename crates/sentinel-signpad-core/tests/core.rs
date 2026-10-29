//! `sentinel-signpad-core` against the C# reference fixtures (the same PNG source
//! images + Framework ImageToByte JPEGs the C# tests use).

use std::path::PathBuf;

use sentinel_signpad_core::acceptance::{is_too_small, serialized_length};
use sentinel_signpad_core::image::{decode_png, Image};
use sentinel_signpad_core::session::{SaveOutcome, SignPadSession, Tablet};
use sentinel_signpad_core::{encoder, texts};

fn bytes(name: &str) -> Vec<u8> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name);
    std::fs::read(&p).unwrap_or_else(|e| panic!("read {name}: {e}"))
}

fn sig_image(name: &str) -> Image {
    decode_png(&bytes(name)).unwrap_or_else(|| panic!("decode {name}"))
}

// ---- texts ----------------------------------------------------------------

#[test]
fn texts_match_the_csharp_bytes() {
    // UTF-8 byte lengths pinned by the C# SignPadTextsTests (section 3.4 of the doc).
    assert_eq!(texts::WINDOW_TITLE.len(), 63);
    assert_eq!(texts::CAPTION_LINE.len(), 26);
    assert_eq!(texts::CLEAR.len(), 14);
    assert_eq!(texts::SAVE.len(), 12);
    assert_eq!(texts::TOO_SMALL_MESSAGE.len(), 88);
    assert_eq!(texts::TOO_SMALL_CAPTION.len(), 8);
    // The ҳ/қ look-alikes, not х/к/h/k.
    assert!(texts::TOO_SMALL_MESSAGE.contains('ҳ'));
    assert!(texts::SAVE.contains('қ'));
    assert_eq!(texts::TOO_SMALL_CAPTION, "Ҳато");
}

// ---- acceptance -----------------------------------------------------------

#[test]
fn boundary_is_exact() {
    // PNG 1337 + 162 = 1499 → rejected; 1338 + 162 = 1500 → accepted ("< 1500").
    assert!(is_too_small(1499));
    assert!(!is_too_small(1500));
}

#[test]
fn fixture_decisions_match_csharp() {
    // The exact serialized lengths (1444/1734/4289) are GDI+ PNG + 162; a Rust PNG
    // encoder gives different lengths, so we pin the DECISION, not the number.
    let blank = serialized_length(&sig_image("blank-sigimage.png"));
    let s3 = serialized_length(&sig_image("stroked3-source.png"));
    let s30 = serialized_length(&sig_image("stroked30-source.png"));
    eprintln!("serialized lengths (Rust PNG+162): blank={blank}, 3={s3}, 30={s30} (C#: 1444/1734/4289)");
    assert!(is_too_small(blank), "blank pad must be rejected (got {blank})");
    assert!(!is_too_small(s3), "3 strokes must be accepted (got {s3})");
    assert!(!is_too_small(s30), "30 strokes must be accepted (got {s30})");
}

// ---- encoder --------------------------------------------------------------

fn decode_jpeg(jpeg: &[u8]) -> (usize, usize, Vec<u8>) {
    let mut d = zune_jpeg::JpegDecoder::new(jpeg);
    let px = d.decode().expect("decode jpeg");
    let (w, h) = d.dimensions().expect("dims");
    let ch = px.len() / (w * h);
    let rgb = if ch == 3 {
        px
    } else {
        let mut out = Vec::with_capacity(w * h * 3);
        for p in px.chunks_exact(ch) {
            out.extend_from_slice(&[p[0], p[1.min(ch - 1)], p[2.min(ch - 1)]]);
        }
        out
    };
    (w, h, rgb)
}

#[test]
fn to_jpeg_is_495x285_and_close_to_the_csharp_bytes() {
    for name in ["blank-sigimage", "stroked3-source", "stroked30-source"] {
        let img = sig_image(&format!("{name}.png"));
        assert_eq!((img.width, img.height), (495, 285), "{name} source size");

        let actual = encoder::to_jpeg(&img).expect("encode");
        assert_eq!(&actual[actual.len() - 2..], &[0xFF, 0xD9], "{name} ends at EOI");

        let golden = bytes(&format!("{name}.imagetobyte.bin"));
        let (aw, ah, a) = decode_jpeg(&actual);
        let (_, _, r) = decode_jpeg(&golden);
        assert_eq!((aw, ah), (495, 285), "{name} decoded size");

        // jpeg-encoder at q90 4:2:0 reproduces the GDI+ default JPEG of these simple
        // (flat white + thin strokes) signature images: the same byte LENGTH as the
        // C# CImageWorks.ImageToByte fixture and PIXEL-IDENTICAL after decode (the
        // C# SignatureEncoderTests assert exact bytes against GDI+; a cross-encoder
        // port pins the decoded image exactly instead).
        let byte_diffs = actual.iter().zip(golden.iter()).filter(|(x, y)| x != y).count();
        eprintln!("{name}: len {} vs {}, byte diffs {}", actual.len(), golden.len(), byte_diffs);
        assert_eq!(actual.len(), golden.len(), "{name} JPEG length matches C#");
        assert_eq!(a, r, "{name} decodes to the exact same pixels as the C# bytes");
    }
}

#[test]
fn to_jpeg_of_empty_is_none() {
    assert!(encoder::to_jpeg(&Image { width: 0, height: 0, rgb: vec![] }).is_none());
}

// ---- session --------------------------------------------------------------

struct FakeTablet {
    image: Image,
    open_count: u32,
    clear_count: u32,
    close_count: u32,
    get_image_count: u32,
    tablet_state: i32,
}

impl FakeTablet {
    fn new(fixture: &str) -> Self {
        FakeTablet {
            image: sig_image(fixture),
            open_count: 0,
            clear_count: 0,
            close_count: 0,
            get_image_count: 0,
            tablet_state: 0,
        }
    }
}

impl Tablet for FakeTablet {
    fn open(&mut self) {
        self.open_count += 1;
        self.tablet_state = 1;
    }
    fn clear(&mut self) {
        self.clear_count += 1;
    }
    fn get_image(&mut self) -> Image {
        self.get_image_count += 1;
        self.image.clone()
    }
    fn close(&mut self) {
        self.close_count += 1;
        self.tablet_state = 0;
    }
}

// A handle so the test can inspect the fake after the session has taken it.
use std::cell::RefCell;
use std::rc::Rc;

struct SharedTablet(Rc<RefCell<FakeTablet>>);
impl Tablet for SharedTablet {
    fn open(&mut self) {
        self.0.borrow_mut().open();
    }
    fn clear(&mut self) {
        self.0.borrow_mut().clear();
    }
    fn get_image(&mut self) -> Image {
        self.0.borrow_mut().get_image()
    }
    fn close(&mut self) {
        self.0.borrow_mut().close();
    }
}

fn session_with(fixture: &str) -> (SignPadSession, Rc<RefCell<FakeTablet>>) {
    let t = Rc::new(RefCell::new(FakeTablet::new(fixture)));
    (SignPadSession::new(Box::new(SharedTablet(t.clone()))), t)
}

#[test]
fn open_turns_the_tablet_on() {
    let (mut s, t) = session_with("blank-sigimage.png");
    s.open();
    assert_eq!(t.borrow().open_count, 1);
    assert_eq!(t.borrow().tablet_state, 1);
}

#[test]
fn save_blank_is_too_small_and_keeps_the_tablet_on() {
    let (mut s, t) = session_with("blank-sigimage.png");
    s.open();
    assert_eq!(s.save(), SaveOutcome::TooSmall);
    assert!(s.jpeg().is_none());
    assert_eq!(t.borrow().close_count, 0);
    assert_eq!(t.borrow().tablet_state, 1);
}

#[test]
fn save_signed_is_accepted_and_turns_the_tablet_off() {
    let (mut s, t) = session_with("stroked3-source.png");
    s.open();
    assert_eq!(s.save(), SaveOutcome::Accepted);
    assert!(s.jpeg().is_some());
    assert_eq!(t.borrow().close_count, 1);
    assert_eq!(t.borrow().tablet_state, 0);
}

#[test]
fn save_after_too_small_can_be_retried() {
    let (mut s, t) = session_with("blank-sigimage.png");
    s.open();
    assert_eq!(s.save(), SaveOutcome::TooSmall);
    assert_eq!(s.save(), SaveOutcome::TooSmall);
    assert_eq!(t.borrow().get_image_count, 2);
}

#[test]
fn clear_clears_the_tablet() {
    let (mut s, t) = session_with("blank-sigimage.png");
    s.clear();
    assert_eq!(t.borrow().clear_count, 1);
}

#[test]
fn close_turns_the_tablet_off() {
    let (mut s, t) = session_with("blank-sigimage.png");
    s.open();
    s.close();
    assert_eq!(t.borrow().tablet_state, 0);
}
