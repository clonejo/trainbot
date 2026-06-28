use image::RgbaImage;

use crate::sequence::Sequence;

/// Encode an animated GIF from the sequence frames.
///
/// Mirrors Go's `createGIF` except for the palette algorithm:
/// - **Go** uses `github.com/mccutchen/palettor` — k-means clustering, 100 iterations.
/// - **Rust** uses `color_quant::NeuQuant` — neural quantization, sample_factor=10.
///
/// Both extract 20 colors from a ≤300×300 thumbnail of the stitched image.
/// The resulting palettes differ, so GIF pixel data is not byte-identical to Go's,
/// but perceptual quality is equivalent.
///
/// Frame timing and skip logic (even-indexed only, prevTS updated only on drawn
/// frames) match Go exactly.
pub(crate) fn create_gif(seq: &Sequence, stitched: &RgbaImage) -> Vec<u8> {
    // Thumbnail the stitched image for palette extraction.
    let stitched_dyn = image::DynamicImage::ImageRgba8(stitched.clone());
    let thumb = stitched_dyn.thumbnail(300, 300).to_rgba8();

    // Extract 20-color palette with NeuQuant (sample_factor=10 = medium quality).
    let nq = color_quant::NeuQuant::new(10, 20, thumb.as_raw());
    let palette_rgba = nq.color_map_rgba();
    // GIF encoder expects RGB palette.
    let palette_rgb: Vec<u8> = palette_rgba
        .chunks(4)
        .flat_map(|c| [c[0], c[1], c[2]])
        .collect();

    let fw = seq.frames[0].width() as u16;
    let fh = seq.frames[0].height() as u16;

    let mut output = Vec::new();
    {
        let mut encoder =
            gif::Encoder::new(&mut output, fw, fh, &palette_rgb).expect("gif encoder init");
        encoder
            .set_repeat(gif::Repeat::Infinite)
            .expect("gif set repeat");

        // prevTS starts at startTS; only updated for even-indexed frames (matching Go).
        let mut prev_ts = seq.start_ts.expect("start_ts must be set");

        for i in 0..seq.frames.len() {
            let ts = seq.ts[i];
            let dt = ts.duration_since(prev_ts).unwrap_or_default();

            if i % 2 == 1 {
                continue;
            }

            let delay = (dt.as_secs_f64() * 100.0) as u16;

            let pixels: Vec<u8> = seq.frames[i]
                .pixels()
                .map(|p| nq.index_of(&[p[0], p[1], p[2], p[3]]) as u8)
                .collect();

            let frame = gif::Frame {
                delay,
                width: fw,
                height: fh,
                buffer: std::borrow::Cow::Owned(pixels),
                ..gif::Frame::default()
            };
            encoder.write_frame(&frame).expect("gif write frame");

            prev_ts = ts;
        }
    }

    output
}
