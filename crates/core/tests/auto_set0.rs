use testdata::{assert_ffmpeg_available, assert_near, testdata_path};
use trainbot_core::{AutoStitcher, Config, FitMethod, Train};
use vid::{FileSrc, FrameSource};

fn run_set0(name: &str, fit_method: FitMethod) -> Vec<Train> {
    let config = Config {
        pixels_per_m: 50.0,
        min_speed_kph: 10.0,
        max_speed_kph: 160.0,
        min_length_m: 10.0,
        max_frame_count_per_seq: 1500,
        mask: None,
        video_encoder: trainbot_core::VideoEncoder::Libx264,
    };

    let path = testdata_path(&format!("set0/{}", name));
    let mut src = FileSrc::open(&path).unwrap_or_else(|e| panic!("open {name}: {e}"));
    let mut stitcher = AutoStitcher::new(config, fit_method);
    let mut trains = Vec::new();

    loop {
        match src.next_frame() {
            Ok(Some(frame)) => {
                // Crop to 300×300 (matching Go test's `r = image.Rect(0, 0, 300, 300)`).
                let img = image::imageops::crop_imm(&frame.image, 0, 0, 300, 300).to_image();
                if let Ok(Some(t)) = stitcher.frame(img, frame.ts) {
                    trains.push(t);
                }
            }
            Ok(None) => break,
            Err(e) => panic!("{name}: frame error: {e}"),
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

#[test]
fn test_set0_day() {
    assert_ffmpeg_available();
    // OLS: deterministic, matches Go within ±0.1 m/s.
    // RANSAC: different RNG from Go's math/rand; ±0.15 m/s tolerance.
    for (method, speed_tol) in [(FitMethod::Ols, 0.1_f64), (FitMethod::Ransac, 0.15)] {
        let trains = run_set0("day.mp4", method);
        assert_eq!(
            trains.len(),
            1,
            "day/{method:?}: expected 1 train, got {}",
            trains.len()
        );
        let t = &trains[0];
        save_image(t, "day", method);
        assert_near(t.length_m(), 86.0, 5.0, &format!("day/{method:?} length_m"));
        assert_near(
            t.speed_m_ps(),
            21.53,
            speed_tol,
            &format!("day/{method:?} speed_m_ps"),
        );
        assert_near(
            t.accel_m_ps2(),
            0.27,
            0.1,
            &format!("day/{method:?} accel_m_ps2"),
        );
        assert!(!t.direction(), "day/{method:?} direction should be left");
    }
}

#[test]
fn test_set0_night() {
    assert_ffmpeg_available();
    for (method, speed_tol) in [(FitMethod::Ols, 0.1_f64), (FitMethod::Ransac, 0.15)] {
        let trains = run_set0("night.mp4", method);
        assert_eq!(
            trains.len(),
            1,
            "night/{method:?}: expected 1 train, got {}",
            trains.len()
        );
        let t = &trains[0];
        save_image(t, "night", method);
        assert_near(
            t.length_m(),
            83.0,
            5.0,
            &format!("night/{method:?} length_m"),
        );
        assert_near(
            t.speed_m_ps(),
            22.7,
            speed_tol,
            &format!("night/{method:?} speed_m_ps"),
        );
        assert_near(
            t.accel_m_ps2(),
            -0.5,
            0.1,
            &format!("night/{method:?} accel_m_ps2"),
        );
        assert!(t.direction(), "night/{method:?} direction should be right");
    }
}

#[test]
fn test_set0_rain() {
    assert_ffmpeg_available();
    for (method, speed_tol) in [(FitMethod::Ols, 0.1_f64), (FitMethod::Ransac, 0.15)] {
        let trains = run_set0("rain.mp4", method);
        assert_eq!(
            trains.len(),
            1,
            "rain/{method:?}: expected 1 train, got {}",
            trains.len()
        );
        let t = &trains[0];
        save_image(t, "rain", method);
        assert_near(
            t.length_m(),
            82.0,
            5.0,
            &format!("rain/{method:?} length_m"),
        );
        assert_near(
            t.speed_m_ps(),
            17.9,
            speed_tol,
            &format!("rain/{method:?} speed_m_ps"),
        );
        assert_near(
            t.accel_m_ps2(),
            0.0,
            0.1,
            &format!("rain/{method:?} accel_m_ps2"),
        );
        assert!(t.direction(), "rain/{method:?} direction should be right");
    }
}

#[test]
fn test_set0_snow() {
    assert_ffmpeg_available();
    for (method, speed_tol) in [(FitMethod::Ols, 0.1_f64), (FitMethod::Ransac, 0.15)] {
        let trains = run_set0("snow.mp4", method);
        assert_eq!(
            trains.len(),
            1,
            "snow/{method:?}: expected 1 train, got {}",
            trains.len()
        );
        let t = &trains[0];
        save_image(t, "snow", method);
        assert_near(
            t.length_m(),
            56.0,
            5.0,
            &format!("snow/{method:?} length_m"),
        );
        assert_near(
            t.speed_m_ps(),
            20.5,
            speed_tol,
            &format!("snow/{method:?} speed_m_ps"),
        );
        assert_near(
            t.accel_m_ps2(),
            -0.75,
            0.1,
            &format!("snow/{method:?} accel_m_ps2"),
        );
        assert!(t.direction(), "snow/{method:?} direction should be right");
    }
}
