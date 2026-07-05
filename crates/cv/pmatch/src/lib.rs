use image::RgbaImage;
use rayon::prelude::*;

/// cos² of the cosine similarity between the patch at `(ox, oy)` in `img` and `pat`.
/// All arithmetic is integer until the final division — matches the Go/C kernel exactly.
/// Hottest function in trainbot.
fn compute_cos2(
    img: &[u8],
    img_stride: usize,
    pat: &[u8],
    pat_stride: usize,
    (ox, oy): (usize, usize),
    (pat_w, pat_h): (usize, usize),
) -> f64 {
    let mut dot: u64 = 0;
    let mut abs_i2: u64 = 0;
    let mut abs_p2: u64 = 0;

    for v in 0..pat_h {
        let row_i = (oy + v) * img_stride + ox * 4;
        let row_p = v * pat_stride;
        for u in 0..pat_w {
            for c in 0..3usize {
                let pi = img[row_i + u * 4 + c] as u64;
                let pp = pat[row_p + u * 4 + c] as u64;
                dot += pi * pp;
                abs_i2 += pi * pi;
                abs_p2 += pp * pp;
            }
        }
    }

    let abs2 = abs_i2 as f64 * abs_p2 as f64;
    if abs2 == 0.0 {
        1.0
    } else {
        (dot as f64 * dot as f64) / abs2
    }
}

/// Cosine similarity of `pat` placed at offset `(ox, oy)` in `img`. Alpha ignored.
pub fn score_rgba_cos(img: &RgbaImage, pat: &RgbaImage, ox: u32, oy: u32) -> f64 {
    compute_cos2(
        img.as_raw(),
        img.width() as usize * 4,
        pat.as_raw(),
        pat.width() as usize * 4,
        (ox as usize, oy as usize),
        (pat.width() as usize, pat.height() as usize),
    )
    .sqrt()
}

/// Search for the best position of `pat` inside `img` using cosine similarity.
///
/// Returns `(x, y, cos)`. On tied cos², the position with the lowest `(y, x)` wins,
/// matching Go's serial scan order (`y` outer, `x` inner, strict `>`).
pub fn search_rgba(img: &RgbaImage, pat: &RgbaImage) -> (u32, u32, f64) {
    let img_w = img.width() as usize;
    let img_h = img.height() as usize;
    let pat_w = pat.width() as usize;
    let pat_h = pat.height() as usize;

    assert!(pat_w <= img_w && pat_h <= img_h, "patch too large");

    let search_w = img_w - pat_w + 1;
    let search_h = img_h - pat_h + 1;

    let img_raw = img.as_raw();
    let pat_raw = pat.as_raw();
    let img_stride = img_w * 4;
    let pat_stride = pat_w * 4;

    // Parallelize over rows. Sequential x scan within each row keeps tie-break
    // deterministic (strict > means first/leftmost x wins).
    let (best_y, best_x, best_cos2) = (0..search_h)
        .into_par_iter()
        .map(|y| {
            let mut row_cos2 = -1.0f64;
            let mut row_x = 0usize;
            for x in 0..search_w {
                let cos2 = compute_cos2(
                    img_raw,
                    img_stride,
                    pat_raw,
                    pat_stride,
                    (x, y),
                    (pat_w, pat_h),
                );
                if cos2 > row_cos2 {
                    row_cos2 = cos2;
                    row_x = x;
                }
            }
            (y, row_x, row_cos2)
        })
        .reduce_with(|a, b| {
            // Higher cos² wins; on exact tie prefer lower (y, x).
            if b.2 > a.2 || (b.2 == a.2 && (b.0, b.1) < (a.0, a.1)) {
                b
            } else {
                a
            }
        })
        .expect("empty search space");

    (best_x as u32, best_y as u32, best_cos2.sqrt())
}

#[cfg(test)]
mod tests {
    use camino::Utf8PathBuf;
    use image::RgbaImage;

    use super::*;

    fn load_bird_jpg() -> RgbaImage {
        let path = Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../pkg/pmatch/testdata/bird.jpg");
        image::open(&path).unwrap().to_rgba8()
    }

    /// PNG saved by Go's decoder — pixel-identical to Go's test images.
    fn load_bird_png() -> RgbaImage {
        let path = Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../pkg/pmatch/testdata/bird.png");
        image::open(&path).unwrap().to_rgba8()
    }

    // Patch position and size matching Go's test constants.
    const X0: u32 = 65;
    const Y0: u32 = 35;
    const W: u32 = 30;
    const H: u32 = 20;

    #[test]
    fn test_score_rgba_cos_perfect() {
        let img = load_bird_jpg();
        let pat = image::imageops::crop_imm(&img, X0, Y0, W, H).to_image();
        let score = score_rgba_cos(&img, &pat, X0, Y0);
        assert!((score - 1.0).abs() < 1e-14, "score={score}");
    }

    #[test]
    fn test_score_rgba_cos_offsets() {
        let img = load_bird_jpg();
        let pat = image::imageops::crop_imm(&img, X0, Y0, W, H).to_image();
        let s0 = score_rgba_cos(&img, &pat, X0, Y0);
        let s1 = score_rgba_cos(&img, &pat, X0 + 1, Y0);
        let s2 = score_rgba_cos(&img, &pat, X0, Y0 + 10);
        let s3 = score_rgba_cos(&img, &pat, X0 + 1, Y0 + 1);
        let s4 = score_rgba_cos(&img, &pat, X0 + 3, Y0 + 3);
        assert!(s1 < s0, "shifted x should score lower");
        assert!(s2 < s0, "shifted y should score lower");
        assert!(s3 < s0, "shifted xy should score lower");
        assert!(s4 < s3, "larger shift should score even lower");
    }

    /// Cross-check against Go's ScoreRGBACosSlow using PNG saved by Go's jpeg decoder.
    /// PNG is lossless, so both sides operate on bit-identical pixels.
    /// Expected values: Go ScoreRGBACosSlow(img, pat, offset) on the same PNG.
    #[test]
    fn test_score_rgba_cos_known_offset() {
        let img = load_bird_png();
        let pat = image::imageops::crop_imm(&img, X0, Y0, W, H).to_image();

        let cases: &[(u32, u32, f64)] = &[
            (X0 + 1, Y0, 0.980_922_400_512_665_1),
            (X0, Y0 + 10, 0.818_700_463_044_673_3),
            (X0 + 3, Y0 + 3, 0.926_046_112_930_290_2),
        ];
        for &(x, y, want) in cases {
            let got = score_rgba_cos(&img, &pat, x, y);
            assert!(
                (got - want).abs() < 1e-12,
                "score_rgba_cos at ({x},{y}): got {got:.20}, want {want:.20}"
            );
        }
    }

    #[test]
    fn test_search_rgba() {
        let img = load_bird_jpg();
        let pat = image::imageops::crop_imm(&img, X0, Y0, W, H).to_image();
        let (x, y, score) = search_rgba(&img, &pat);
        assert!((score - 1.0).abs() < 1e-14, "score={score}");
        assert_eq!(x, X0);
        assert_eq!(y, Y0);
    }
}
