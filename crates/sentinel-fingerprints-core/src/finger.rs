//! The finger model: the enums (`FingerStatus`, `FingerType`, `FingerPosition`,
//! `TileBack`), an 8-bit grey image, and `EnrolledFinger` — `FormFingerprintsV2`'s
//! `ModelFinger` plus its tile state.

/// `Asbt.Media.Models.FingerStatus` [measured].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FingerStatus {
    /// The finger exists and should be captured.
    Excist = 1,
    /// Marked damaged.
    Damaged = 2,
    /// Marked absent.
    Absent = 3,
}

/// `Asbt.Media.Models.FingerTypes` (1..10); also the RS_FGP_* SDK finger codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FingerType {
    /// Finger 1.
    RightThumb = 1,
    /// Finger 2.
    RightIndex = 2,
    /// Finger 3.
    RightMiddle = 3,
    /// Finger 4.
    RightRing = 4,
    /// Finger 5.
    RightLittle = 5,
    /// Finger 6.
    LeftThumb = 6,
    /// Finger 7.
    LeftIndex = 7,
    /// Finger 8.
    LeftMiddle = 8,
    /// Finger 9.
    LeftRing = 9,
    /// Finger 10.
    LeftLittle = 10,
}

impl FingerType {
    /// The 1..10 finger number.
    pub fn number(self) -> i32 {
        self as i32
    }

    /// The finger type for a 1..10 number, or `None`.
    pub fn from_number(n: i32) -> Option<FingerType> {
        use FingerType::*;
        Some(match n {
            1 => RightThumb,
            2 => RightIndex,
            3 => RightMiddle,
            4 => RightRing,
            5 => RightLittle,
            6 => LeftThumb,
            7 => LeftIndex,
            8 => LeftMiddle,
            9 => LeftRing,
            10 => LeftLittle,
            _ => return None,
        })
    }
}

/// `FingerSelectorV2.FingerPosition`: 1..10 single fingers, 13..15 the plain slaps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum FingerPosition {
    /// 0.
    Unknown = 0,
    /// 1.
    RightThumb = 1,
    /// 2.
    RightIndex = 2,
    /// 3.
    RightMiddle = 3,
    /// 4.
    RightRing = 4,
    /// 5.
    RightLittle = 5,
    /// 6.
    LeftThumb = 6,
    /// 7.
    LeftIndex = 7,
    /// 8.
    LeftMiddle = 8,
    /// 9.
    LeftRing = 9,
    /// 10.
    LeftLittle = 10,
    /// 13: the right-hand four-finger slap.
    PlainRightFourFingers = 13,
    /// 14: the left-hand four-finger slap.
    PlainLeftFourFingers = 14,
    /// 15: the two thumbs.
    PlainThumbs = 15,
}

impl FingerPosition {
    /// The numeric position value.
    pub fn value(self) -> i32 {
        self as i32
    }

    /// The position for a value, or `None`.
    pub fn from_value(v: i32) -> Option<FingerPosition> {
        use FingerPosition::*;
        Some(match v {
            0 => Unknown,
            1 => RightThumb,
            2 => RightIndex,
            3 => RightMiddle,
            4 => RightRing,
            5 => RightLittle,
            6 => LeftThumb,
            7 => LeftIndex,
            8 => LeftMiddle,
            9 => LeftRing,
            10 => LeftLittle,
            13 => PlainRightFourFingers,
            14 => PlainLeftFourFingers,
            15 => PlainThumbs,
            _ => return None,
        })
    }
}

/// `PictureEdit.BackColor` of a tile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TileBack {
    /// The skin default.
    Default,
    /// White.
    White,
    /// Orange (damaged).
    Orange,
    /// Red (absent).
    Red,
}

/// An 8-bit greyscale image (what `RS_TakeImageData` / `RS_Segment4` return).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GreyImage {
    /// One byte per pixel, row-major.
    pub pixels: Vec<u8>,
    /// Width in pixels.
    pub width: i32,
    /// Height in pixels.
    pub height: i32,
}

/// `FormFingerprintsV2.ModelFinger` plus the state of its tile.
#[derive(Debug, Clone)]
pub struct EnrolledFinger {
    /// The finger's type (fixed).
    pub finger_type: FingerType,
    /// Whether a WSQ has been captured for it.
    pub is_enrolled: bool,
    /// Excist / Damaged / Absent.
    pub status: FingerStatus,
    /// `RS_EncodeWSQ` output after the trailing-zero trim, or `None`.
    pub wsq: Option<Vec<u8>>,
    /// asbtQuality (0 or `5 - sdk_quality`), `None` when never captured / cleared.
    pub quality: Option<i32>,
    /// What the tile would show (`None` when the WSQ is null).
    pub bitmap: Option<GreyImage>,
    /// Segmented width.
    pub width: Option<i32>,
    /// Segmented height.
    pub height: Option<i32>,
    /// `pictureFingerN.Image`.
    pub tile_image: Option<GreyImage>,
    /// `qualityFingerN.Rating`.
    pub tile_rating: i32,
    /// `pictureFingerN.BackColor`.
    pub tile_back: TileBack,
}

impl EnrolledFinger {
    /// A fresh finger of `finger_type`, status Excist (the ctor default).
    pub fn new(finger_type: FingerType) -> Self {
        EnrolledFinger {
            finger_type,
            is_enrolled: false,
            status: FingerStatus::Excist,
            wsq: None,
            quality: None,
            bitmap: None,
            width: Some(0),
            height: Some(0),
            tile_image: None,
            tile_rating: 0,
            tile_back: TileBack::Default,
        }
    }

    /// The 1..10 finger number.
    pub fn number(&self) -> i32 {
        self.finger_type.number()
    }
}
