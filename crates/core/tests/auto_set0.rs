use testdata::{assert_ffmpeg_available, assert_near, testdata_path};
use trainbot_core::{AutoStitcher, Config, FitMethod, Train};
use vid::{FileSrc, FrameSource};

#[test]
fn test_set0_day() {
    run_test_detailed("day", 86.0, 21.53, 0.27, false);
}
#[test]
fn test_set0_night() {
    run_test_detailed("night", 83.0, 22.7, -0.5, true);
}
#[test]
fn test_set0_rain() {
    run_test_detailed("rain", 82.0, 17.9, 0.0, true);
}
#[test]
fn test_set0_snow() {
    run_test_detailed("snow", 56.0, 20.5, -0.75, true);
}

fn run_test_detailed(
    video_stem: &str,
    length_m: f64,
    speed_m_ps: f64,
    accell_m_ps2: f64,
    direction: bool,
) {
    assert_ffmpeg_available();
    // RANSAC: different RNG from Go's math/rand; ±0.15 m/s tolerance.
    // OLS: deterministic, matches Go within ±0.1 m/s.
    for (method, speed_tol) in [(FitMethod::Ransac, 0.15_f64), (FitMethod::Ols, 0.1)] {
        let trains = run_set0(video_stem, method);
        assert_eq!(
            trains.len(),
            1,
            "{video_stem}/{method:?}: expected 1 train, got {}",
            trains.len()
        );
        let t = &trains[0];
        save_image(t, video_stem, method);
        assert_near(
            t.length_m(),
            length_m,
            5.0,
            &format!("{video_stem}/{method:?} length_m"),
        );
        // In Go trainbot, frame time was incorrectly calculated for file sources. So we correct for this test that was taken from the Go code. However, this only affects videos with 29.83 fps, but not the videos with 30fps.
        let time_correction_factor = match video_stem {
            "night" | "rain" | "snow" => 1.028736,
            _ => 1.0,
        };
        assert_near(
            t.speed_m_ps(),
            speed_m_ps * time_correction_factor,
            speed_tol,
            &format!("{video_stem}/{method:?} speed_m_ps"),
        );
        assert_near(
            t.accel_m_ps2(),
            accell_m_ps2 * time_correction_factor,
            0.1,
            &format!("{video_stem}/{method:?} accel_m_ps2"),
        );
        assert_eq!(
            t.direction(),
            direction,
            "{video_stem}/{method:?} direction should be left"
        );
    }
}

fn run_set0(video_stem: &str, fit_method: FitMethod) -> Vec<Train> {
    let config = Config {
        pixels_per_m: 50.0,
        min_speed_kph: 10.0,
        max_speed_kph: 160.0,
        min_length_m: 10.0,
        max_frame_count_per_seq: 1500,
        mask: None,
        video_encoder: trainbot_core::VideoEncoder::Libx264,
    };

    let path = testdata_path(&format!("set0/{}.mp4", video_stem));
    let mut src = FileSrc::open(&path).unwrap_or_else(|e| panic!("open {video_stem}: {e}"));
    let mut stitcher = AutoStitcher::new(config, fit_method);
    let mut trains = Vec::new();

    loop {
        match src.next_frame() {
            Ok(Some(mut frame)) => {
                // Crop to 300×300 (matching Go test's `r = image.Rect(0, 0, 300, 300)`).
                frame.image = image::imageops::crop_imm(&frame.image, 0, 0, 300, 300).to_image();
                if let Ok(Some(t)) = stitcher.frame(frame) {
                    trains.push(t);
                }
            }
            Ok(None) => break,
            Err(e) => panic!("{video_stem}: frame error: {e}"),
        }
    }

    if let Ok(Some(t)) = stitcher.try_stitch_and_reset() {
        trains.push(t);
    }

    trains
}

fn save_image(train: &Train, stem: &str, method: FitMethod) {
    let dir = std::path::Path::new("/tmp/trainbot-set0");
    std::fs::create_dir_all(dir).unwrap();
    let method_str = match method {
        FitMethod::Ols => "ols",
        FitMethod::Ransac => "ransac",
    };
    let path = dir.join(format!("{stem}-rust-{method_str}.jpg"));
    image::DynamicImage::ImageRgba8(train.image.clone())
        .save(&path)
        .unwrap_or_else(|e| eprintln!("save {path:?}: {e}"));
}
