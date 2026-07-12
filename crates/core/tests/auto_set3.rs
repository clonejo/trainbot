use anyhow::{Context as _, Result};
use camino::Utf8Path;
use image::DynamicImage;
use testdata::{assert_ffmpeg_available, testdata_path};
use trainbot_core::{AutoStitcher, Config, FitMethod};

use vid::{FileSrc, FrameSource};

#[test]
fn test_masked_stitch() -> Result<()> {
    assert_ffmpeg_available();
    let mask = imutil::load(testdata_path("set3/masked-mask.png"))
        .unwrap()
        .into_rgba8();

    let config = Config {
        // Original masked.mp4 is scaled down by 0.8 to save space.
        pixels_per_m: 111.0 * 0.8,
        min_speed_kph: 1.0,
        max_speed_kph: 70.0,
        min_length_m: 10.0,
        max_frame_count_per_seq: 1500,
        mask: Some(mask),
        video_encoder: trainbot_core::VideoEncoder::Libx264,
    };

    let name = "set3/masked.mp4";
    let path = testdata_path(name);
    let reference_path = testdata_path("set3/masked.png");
    let mut src = FileSrc::open(&path).unwrap_or_else(|e| panic!("open {name}: {e}"));
    let mut stitcher = AutoStitcher::new(config, FitMethod::Ransac);

    let train = loop {
        match src.next_frame() {
            Ok(Some(frame)) => {
                if let Ok(Some(train)) = stitcher.frame(frame) {
                    break train;
                }
            }
            // end of src:
            Ok(None) => {
                break stitcher
                    .try_stitch_and_reset()
                    .unwrap()
                    .unwrap_or_else(|| panic!("{name}: no train found"))
            }
            Err(e) => panic!("{name}: frame error: {e}"),
        }
    };

    assert_image_snapshot(&reference_path, &train.image)?;

    Ok(())
}

/// Compare a freshly generated image against a stored snapshot.
/// Set UPDATE_SNAPSHOTS=1 to (re)generate the reference instead of asserting.
fn assert_image_snapshot(reference_path: &Utf8Path, generated: &image::RgbaImage) -> Result<()> {
    let generated_path = reference_path.with_extension("generated.png");
    imutil::save(
        &generated_path,
        &DynamicImage::ImageRgba8(generated.clone()),
    )
    .with_context(|| format!("save png {generated_path}"))?;

    let reference = image::open(reference_path)
        .with_context(|| format!("opening snapshot at {reference_path}"))?
        .into_rgba8();

    let result = image_compare::rgba_hybrid_compare(&reference, generated)
        .expect("images had different dimensions");

    const THRESHOLD: f64 = 0.98; // tune per test; 1.0 == identical

    if result.score < THRESHOLD {
        // Dump a diff image so you can see what changed.
        let diff_path = reference_path.with_extension("diff.png");
        result.image.to_color_map().save(&diff_path).ok();
        panic!(
            "image '{}' diverged: score {:.4} < {THRESHOLD} (diff at {diff_path})",
            reference_path.file_name().unwrap(),
            result.score
        );
    }
    Ok(())
}
