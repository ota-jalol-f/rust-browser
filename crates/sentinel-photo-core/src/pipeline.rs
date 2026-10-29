//! The reference's GDI+ document-photo image path, ported on pure-Rust codecs.
//!
//! `FormCaptureDocument` did, per shot: decode the camera file → `Clone` → crop
//! ([`crop_rect`]) → `ScaleByPercent` ([`scaled_size`], 24bpp HighQualityBicubic) →
//! on save, the default GDI+ JPEG at 96 dpi. [`capture`] runs decode→crop→scale and
//! [`to_jpeg`] the final encode; [`capture_to_jpeg`] chains them.
//!
//! **Parity.** [`crop_rect`] and [`scaled_size`] are exact: the same integer and
//! `f32` (SSE) arithmetic C# used, pinned against `DocumentPipelineTests`' vectors.
//! The decode/resample/encode produce a JPEG of the **same size, pixel layout and
//! 96 dpi** as the C# reference and valid JPEG markers, but not the same bytes —
//! a different decoder (`zune-jpeg`), a different resampler (the separable cubic
//! below) and a different encoder (`jpeg-encoder`) than GDI+. The C# test itself
//! only claims pixel *closeness* for its x64-vs-x86 GDI+ comparison; the Rust
//! pipeline is held to a measured, documented tolerance against the committed x86
//! goldens (see the crate's `tests/pipeline.rs`), and the exact pixels still need a
//! real-camera visual check (design §8).

use zune_jpeg::JpegDecoder;

/// The header dpi of every output JPEG (`DocumentPipeline.OutputDpi`). GDI+ wrote
/// the DPI-unaware desktop's 96; the WPF host is DPI-aware, so this is pinned.
pub const OUTPUT_DPI: u16 = 96;

/// GDI+'s default JPEG quality on the measured runtime (design §3). The encoder is
/// not GDI+, so this is the value that minimised the pixel diff against the goldens
/// (swept in the tests), kept here as the one knob the C# pipeline also left to the
/// default.
pub const OUTPUT_QUALITY: u8 = 91;

/// An integer crop rectangle (GDI+ `Rectangle`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    /// Left edge.
    pub x: i32,
    /// Top edge.
    pub y: i32,
    /// Width.
    pub w: i32,
    /// Height.
    pub h: i32,
}

/// A decoded 24bpp image, rows of interleaved `R,G,B` with no stride padding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rgb {
    /// Width in pixels.
    pub width: usize,
    /// Height in pixels.
    pub height: usize,
    /// `width * height * 3` bytes, row-major `RGB`.
    pub data: Vec<u8>,
}

/// A failure of the image path. The caller maps it to the C# "Cуратга олишда ҳато"
/// toast / the thrown capture exception.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PipelineError {
    /// The camera file could not be decoded (a RAW/CR2 file, or corrupt bytes) —
    /// C#'s `ByteArrayToImage` → `null` → `new Bitmap(null)` throws.
    Decode(String),
    /// The JPEG encoder failed.
    Encode(String),
    /// The crop or scaled size came out empty (a degenerate input).
    Empty,
}

impl std::fmt::Display for PipelineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PipelineError::Decode(m) => write!(f, "decode failed: {m}"),
            PipelineError::Encode(m) => write!(f, "encode failed: {m}"),
            PipelineError::Empty => write!(f, "image is empty"),
        }
    }
}

impl std::error::Error for PipelineError {}

/// `FormCaptureDocument.cs:273-276`: shots narrower than 4000 or lower than 3185
/// keep the centre half of the width at full height; bigger ones take the fixed
/// `(2000,800,2000,2385)`. Integer arithmetic, exactly as C#.
pub fn crop_rect(width: i32, height: i32) -> Rect {
    if width < 4000 || height < 3185 {
        Rect { x: width - width * 3 / 4, y: 0, w: width / 2, h: height }
    } else {
        Rect { x: 2000, y: 800, w: 2000, h: 2385 }
    }
}

/// `ScaleByPercent`'s target size: `(int)(dim * (percent / 100f))`, computed in
/// `f32` (SSE) exactly as C#. 20 and 30 are identical on x86 and x64; 70 gives 1400
/// (SSE) where x87 extended precision gives 1399 — the `f32` product below yields
/// 1400, matching the x64 plugin and the committed goldens.
pub fn scaled_size(width: i32, height: i32, percent: i32) -> (i32, i32) {
    let n = percent as f32 / 100.0;
    ((width as f32 * n) as i32, (height as f32 * n) as i32)
}

/// Decode the camera file, crop ([`crop_rect`]) and scale ([`scaled_size`]) into the
/// pane image. `Err(Decode)` on an undecodable file, like C#'s RAW case.
pub fn capture(camera_file: &[u8], percent: i32) -> Result<Rgb, PipelineError> {
    let decoded = decode_rgb(camera_file)?;
    let rect = crop_rect(decoded.width as i32, decoded.height as i32);
    let cropped = crop(&decoded, rect);
    let (dw, dh) = scaled_size(cropped.width as i32, cropped.height as i32, percent);
    if dw <= 0 || dh <= 0 {
        return Err(PipelineError::Empty);
    }
    Ok(resize(&cropped, dw as usize, dh as usize))
}

/// Encode the pane image as JPEG at [`OUTPUT_DPI`], the bytes `TakeDocumentPhoto`
/// returns (base64) on save — C#'s `ImageToByte(new Bitmap(DocumentImage))`, trimmed
/// (no `GetBuffer` zero slack) and with the dpi pinned to 96.
pub fn to_jpeg(image: &Rgb) -> Result<Vec<u8>, PipelineError> {
    use jpeg_encoder::{ColorType, Density, Encoder, SamplingFactor};

    if image.width == 0 || image.height == 0 {
        return Err(PipelineError::Empty);
    }
    let w = u16::try_from(image.width).map_err(|_| PipelineError::Encode("width > 65535".into()))?;
    let h = u16::try_from(image.height).map_err(|_| PipelineError::Encode("height > 65535".into()))?;

    let mut out = Vec::new();
    let mut encoder = Encoder::new(&mut out, OUTPUT_QUALITY);
    encoder.set_density(Density::Inch { x: OUTPUT_DPI, y: OUTPUT_DPI });
    // Match the GDI+ goldens' chroma subsampling: Y 2x2, Cb/Cr 1x1 = 4:2:0. The
    // encoder would otherwise switch to 4:4:4 at quality >= 90, which fringes at
    // coloured edges against the 4:2:0 reference.
    encoder.set_sampling_factor(SamplingFactor::F_2_2);
    encoder
        .encode(&image.data, w, h, ColorType::Rgb)
        .map_err(|e| PipelineError::Encode(e.to_string()))?;
    Ok(out)
}

/// `capture` then `to_jpeg`: camera file → base64-ready JPEG bytes.
pub fn capture_to_jpeg(camera_file: &[u8], percent: i32) -> Result<Vec<u8>, PipelineError> {
    to_jpeg(&capture(camera_file, percent)?)
}

/// Decode a JPEG to 24bpp RGB. Grayscale is expanded, RGBA has its alpha dropped
/// (GDI+ produced a 24bpp bitmap either way).
fn decode_rgb(bytes: &[u8]) -> Result<Rgb, PipelineError> {
    let mut decoder = JpegDecoder::new(bytes);
    let pixels = decoder.decode().map_err(|e| PipelineError::Decode(e.to_string()))?;
    let (width, height) = decoder
        .dimensions()
        .ok_or_else(|| PipelineError::Decode("no dimensions".into()))?;
    let pixel_count = width.checked_mul(height).ok_or(PipelineError::Empty)?;
    if pixel_count == 0 {
        return Err(PipelineError::Empty);
    }
    let channels = pixels.len() / pixel_count;
    let data = match channels {
        3 => pixels,
        1 => {
            let mut rgb = Vec::with_capacity(pixel_count * 3);
            for &g in &pixels {
                rgb.extend_from_slice(&[g, g, g]);
            }
            rgb
        }
        4 => {
            let mut rgb = Vec::with_capacity(pixel_count * 3);
            for px in pixels.chunks_exact(4) {
                rgb.extend_from_slice(&px[..3]);
            }
            rgb
        }
        _ => return Err(PipelineError::Decode(format!("unsupported channel count {channels}"))),
    };
    Ok(Rgb { width, height, data })
}

/// Crop `src` to `rect` (clamped into the image, as GDI+'s `Clone(rect)` requires a
/// rectangle inside the bitmap — the crop rule always produces one).
fn crop(src: &Rgb, rect: Rect) -> Rgb {
    let x0 = rect.x.max(0) as usize;
    let y0 = rect.y.max(0) as usize;
    let w = (rect.w.max(0) as usize).min(src.width.saturating_sub(x0));
    let h = (rect.h.max(0) as usize).min(src.height.saturating_sub(y0));
    let mut data = Vec::with_capacity(w * h * 3);
    for y in 0..h {
        let row = (y0 + y) * src.width * 3 + x0 * 3;
        data.extend_from_slice(&src.data[row..row + w * 3]);
    }
    Rgb { width: w, height: h, data }
}

/// A Catmull-Rom cubic (a = -0.5), the usual bicubic kernel.
fn cubic(x: f32) -> f32 {
    const A: f32 = -0.5;
    let x = x.abs();
    if x < 1.0 {
        (A + 2.0) * x * x * x - (A + 3.0) * x * x + 1.0
    } else if x < 2.0 {
        A * x * x * x - 5.0 * A * x * x + 8.0 * A * x - 4.0 * A
    } else {
        0.0
    }
}

/// Precompute, for each output index, the first source index and the normalised
/// weights. On a downscale the kernel is widened by `1/ratio` so it averages over
/// the shrunk region (a high-quality downscale) rather than aliasing.
fn weights(src_len: usize, dst_len: usize) -> Vec<(isize, Vec<f32>)> {
    let ratio = dst_len as f32 / src_len as f32;
    let filter_scale = if ratio < 1.0 { 1.0 / ratio } else { 1.0 };
    let support = 2.0 * filter_scale;
    let mut table = Vec::with_capacity(dst_len);
    for o in 0..dst_len {
        let center = (o as f32 + 0.5) / ratio - 0.5;
        let left = (center - support).ceil() as isize;
        let right = (center + support).floor() as isize;
        let mut ws = Vec::with_capacity((right - left + 1).max(0) as usize);
        let mut sum = 0.0;
        for i in left..=right {
            let w = cubic((i as f32 - center) / filter_scale);
            ws.push(w);
            sum += w;
        }
        if sum != 0.0 {
            for w in &mut ws {
                *w /= sum;
            }
        }
        table.push((left, ws));
    }
    table
}

/// Separable cubic resample of a 24bpp image to `dw x dh`. Identity when the size is
/// unchanged. Edge samples are clamped (replicate).
fn resize(src: &Rgb, dw: usize, dh: usize) -> Rgb {
    if dw == src.width && dh == src.height {
        return src.clone();
    }
    let (sw, sh) = (src.width, src.height);

    // Horizontal: src (sw x sh) -> tmp (dw x sh), kept as f32 to feed the vertical pass.
    let hw = weights(sw, dw);
    let mut tmp = vec![0f32; dw * sh * 3];
    for y in 0..sh {
        let srow = y * sw * 3;
        let trow = y * dw * 3;
        for (x, (left, ws)) in hw.iter().enumerate() {
            let (mut r, mut g, mut b) = (0f32, 0f32, 0f32);
            for (k, &wgt) in ws.iter().enumerate() {
                let sx = (*left + k as isize).clamp(0, sw as isize - 1) as usize;
                let p = srow + sx * 3;
                r += src.data[p] as f32 * wgt;
                g += src.data[p + 1] as f32 * wgt;
                b += src.data[p + 2] as f32 * wgt;
            }
            let t = trow + x * 3;
            tmp[t] = r;
            tmp[t + 1] = g;
            tmp[t + 2] = b;
        }
    }

    // Vertical: tmp (dw x sh) -> out (dw x dh).
    let vw = weights(sh, dh);
    let mut data = vec![0u8; dw * dh * 3];
    for (y, (left, ws)) in vw.iter().enumerate() {
        let orow = y * dw * 3;
        for x in 0..dw {
            let (mut r, mut g, mut b) = (0f32, 0f32, 0f32);
            for (k, &wgt) in ws.iter().enumerate() {
                let sy = (*left + k as isize).clamp(0, sh as isize - 1) as usize;
                let p = sy * dw * 3 + x * 3;
                r += tmp[p] * wgt;
                g += tmp[p + 1] * wgt;
                b += tmp[p + 2] * wgt;
            }
            let o = orow + x * 3;
            data[o] = r.clamp(0.0, 255.0).round() as u8;
            data[o + 1] = g.clamp(0.0, 255.0).round() as u8;
            data[o + 2] = b.clamp(0.0, 255.0).round() as u8;
        }
    }
    Rgb { width: dw, height: dh, data }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crop_rect_is_the_csharp_rule() {
        // (w, h) -> (x, y, w, h), from DocumentPipelineTests.CropRect_isTheCSharpRule.
        assert_eq!(crop_rect(5184, 3456), Rect { x: 2000, y: 800, w: 2000, h: 2385 });
        assert_eq!(crop_rect(6000, 4000), Rect { x: 2000, y: 800, w: 2000, h: 2385 });
        assert_eq!(crop_rect(4000, 3185), Rect { x: 2000, y: 800, w: 2000, h: 2385 });
        assert_eq!(crop_rect(3999, 3456), Rect { x: 1000, y: 0, w: 1999, h: 3456 });
        assert_eq!(crop_rect(5184, 3184), Rect { x: 1296, y: 0, w: 2592, h: 3184 });
        assert_eq!(crop_rect(3456, 2304), Rect { x: 864, y: 0, w: 1728, h: 2304 });
    }

    #[test]
    fn scaled_size_is_float_truncation() {
        // From DocumentPipelineTests.ScaledSize_isFloatTruncation.
        assert_eq!(scaled_size(2000, 2385, 20), (400, 477));
        assert_eq!(scaled_size(2000, 2385, 30), (600, 715));
        assert_eq!(scaled_size(1728, 2304, 20), (345, 460));
        assert_eq!(scaled_size(1728, 2304, 30), (518, 691));
        assert_eq!(scaled_size(2592, 3184, 20), (518, 636));
        assert_eq!(scaled_size(2592, 3184, 30), (777, 955));
        assert_eq!(scaled_size(2000, 2385, 100), (2000, 2385));
        assert_eq!(scaled_size(2000, 2385, 1), (20, 23));
        // SSE: 70% of 2000 is 1400 (x87 extended precision would give 1399).
        assert_eq!(scaled_size(2000, 2385, 70), (1400, 1669));
    }

    #[test]
    fn decode_of_a_raw_file_errors_like_csharp() {
        // A Canon CR2 header: not a JPEG, GDI+ could not decode it.
        let raw = [0x49u8, 0x49, 0x2A, 0x00, 0x10, 0x00, 0x00, 0x00, 0x43, 0x52, 0x02, 0x00];
        assert!(matches!(capture(&raw, 20), Err(PipelineError::Decode(_))));
    }
}
