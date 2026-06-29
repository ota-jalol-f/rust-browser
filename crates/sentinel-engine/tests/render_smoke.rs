//! Render smoke test: proves Ultralight actually produces pixels.
//!
//! Compiled only with `--features ultralight`, and `#[ignore]`d because it needs
//! the Ultralight SDK shared libraries on the path at runtime. Run with:
//!   `cargo test -p sentinel-engine --features ultralight -- --ignored`

#![cfg(feature = "ultralight")]

#[test]
#[ignore = "requires Ultralight SDK shared libraries at runtime; run with --ignored"]
fn red_page_produces_non_blank_pixels() {
    let html = "<html><body style='margin:0;background:#ff0000'></body></html>";
    let bgra = sentinel_engine::headless::render_html_to_bgra(html, 64, 64)
        .expect("render should succeed when the SDK is present");

    assert_eq!(bgra.len(), 64 * 64 * 4, "buffer must be width*height*4 bytes");

    // Premultiplied BGRA: assert at least one pixel is strongly single-channel
    // dominant (our red fill) without hard-coding channel order, which proves the
    // page was rasterized rather than left blank/transparent.
    let has_strong_color = bgra.chunks_exact(4).any(|px| {
        let rgb = &px[..3];
        let max = *rgb.iter().max().unwrap();
        let min = *rgb.iter().min().unwrap();
        max > 200 && min < 80
    });
    assert!(has_strong_color, "expected a strongly-colored pixel; surface was blank");
}
