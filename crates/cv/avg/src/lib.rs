use rayon::prelude::*;

/// Compute per-channel pixel average and average absolute deviation for an RGBA image.
///
/// Returns `(avg, avg_dev)` — each is `[r, g, b]` in `[0.0, 1.0]`. Alpha ignored.
/// Integer-truncating division is used for the per-channel mean, matching the Go/C kernel.
pub fn rgba(img: &image::RgbaImage) -> ([f64; 3], [f64; 3]) {
    let cnt = (img.width() as u64) * (img.height() as u64);
    assert!(cnt > 0, "empty image");

    // Pass 1: per-channel sum.
    let sum = img
        .as_raw()
        .par_chunks(4)
        .fold(
            || [0u64; 3],
            |mut acc, px| {
                acc[0] += px[0] as u64;
                acc[1] += px[1] as u64;
                acc[2] += px[2] as u64;
                acc
            },
        )
        .reduce(|| [0u64; 3], |a, b| [a[0] + b[0], a[1] + b[1], a[2] + b[2]]);

    // Integer-truncating division — intentional, matches Go.
    let avg_px = [sum[0] / cnt, sum[1] / cnt, sum[2] / cnt];

    // Pass 2: sum of absolute deviations from the integer mean.
    let dev_sum = img
        .as_raw()
        .par_chunks(4)
        .fold(
            || [0u64; 3],
            |mut acc, px| {
                for c in 0..3 {
                    acc[c] += (px[c] as i64 - avg_px[c] as i64).unsigned_abs();
                }
                acc
            },
        )
        .reduce(|| [0u64; 3], |a, b| [a[0] + b[0], a[1] + b[1], a[2] + b[2]]);

    let avg = avg_px.map(|v| v as f64 / 255.0);
    let avg_dev = dev_sum.map(|v| v as f64 / cnt as f64 / 255.0);

    (avg, avg_dev)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Expected values computed by Go from the same PNG pixel data.
    // Tolerances are tight because all arithmetic is integer-exact.

    #[test]
    fn test_rgba_high() {
        let (avg, dev) = rgba(&load_png("high.png"));
        assert_near(avg[0], 0.450980, 1e-5, "high avg R");
        assert_near(avg[1], 0.388235, 1e-5, "high avg G");
        assert_near(avg[2], 0.447059, 1e-5, "high avg B");
        assert_near(dev[0], 0.255289, 1e-5, "high dev R");
        assert_near(dev[1], 0.220371, 1e-5, "high dev G");
        assert_near(dev[2], 0.186564, 1e-5, "high dev B");
    }

    #[test]
    fn test_rgba_mid() {
        let (avg, dev) = rgba(&load_png("mid.png"));
        assert_near(avg[0], 0.019608, 1e-5, "mid avg R");
        assert_near(avg[1], 0.015686, 1e-5, "mid avg G");
        assert_near(avg[2], 0.007843, 1e-5, "mid avg B");
        assert_near(dev[0], 0.023127, 1e-5, "mid dev R");
        assert_near(dev[1], 0.016720, 1e-5, "mid dev G");
        assert_near(dev[2], 0.009078, 1e-5, "mid dev B");
    }

    #[test]
    fn test_rgba_low() {
        let (avg, dev) = rgba(&load_png("low.png"));
        assert_near(avg[0], 0.0, 1e-8, "low avg R");
        assert_near(avg[1], 0.0, 1e-8, "low avg G");
        assert_near(avg[2], 0.0, 1e-8, "low avg B");
        assert_near(dev[0], 0.003860, 1e-5, "low dev R");
        assert_near(dev[1], 0.002980, 1e-5, "low dev G");
        assert_near(dev[2], 0.002277, 1e-5, "low dev B");
    }

    /// Load a PNG generated from the Go test images (Go's jpeg decoder → PNG write).
    /// PNG is lossless so both decoders produce bit-identical pixel data.
    fn load_png(name: &str) -> image::RgbaImage {
        let path = camino::Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../pkg/avg/testdata")
            .join(name);
        image::open(&path).unwrap().to_rgba8()
    }

    fn assert_near(got: f64, want: f64, tol: f64, label: &str) {
        assert!(
            (got - want).abs() <= tol,
            "{label}: got {got}, want {want} ± {tol}"
        );
    }
}
