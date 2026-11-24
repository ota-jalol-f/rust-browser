//! A 24bpp RGB image + PNG decode. The signature arrives either as a fixture PNG
//! (the C# `GetSigImage()` saved) or as the overlay canvas's PNG; both decode here.

/// A decoded 24bpp image: rows of interleaved `R,G,B`, no stride padding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    /// Width in pixels.
    pub width: usize,
    /// Height in pixels.
    pub height: usize,
    /// `width * height * 3` bytes, row-major `RGB`.
    pub rgb: Vec<u8>,
}

impl Image {
    /// A solid-white image (the canvas background / a blank pad).
    pub fn white(width: usize, height: usize) -> Self {
        Image { width, height, rgb: vec![0xFF; width * height * 3] }
    }
}

/// Decode a PNG to 24bpp RGB. Alpha is dropped (as GDI+'s `new Bitmap(argb)` → JPEG
/// does — JPEG has no alpha), grayscale and palette are expanded. `None` on any
/// decode failure.
pub fn decode_png(bytes: &[u8]) -> Option<Image> {
    let decoder = png::Decoder::new(bytes);
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).ok()?;
    let (width, height) = (info.width as usize, info.height as usize);
    let pixels = width.checked_mul(height)?;
    if pixels == 0 {
        return None;
    }
    let channels = match info.color_type {
        png::ColorType::Grayscale => 1,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        // Palette/indexed is expanded by the transform below; handled via bytes/pixel.
        png::ColorType::Indexed => 0,
    };
    // 16-bit depth doubles the bytes; only 8-bit is produced by the canvas / fixtures.
    if info.bit_depth != png::BitDepth::Eight {
        return None;
    }
    let data = &buf[..info.buffer_size()];
    let mut rgb = Vec::with_capacity(pixels * 3);
    match channels {
        1 => {
            for &g in &data[..pixels] {
                rgb.extend_from_slice(&[g, g, g]);
            }
        }
        2 => {
            for px in data.chunks_exact(2).take(pixels) {
                rgb.extend_from_slice(&[px[0], px[0], px[0]]);
            }
        }
        3 => rgb.extend_from_slice(&data[..pixels * 3]),
        4 => {
            for px in data.chunks_exact(4).take(pixels) {
                rgb.extend_from_slice(&px[..3]);
            }
        }
        _ => return None, // indexed without expansion — not produced here
    }
    if rgb.len() != pixels * 3 {
        return None;
    }
    Some(Image { width, height, rgb })
}
