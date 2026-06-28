use trainbot_core::{AutoStitcher, Config, Train};
use vid::{FileSrc, FrameSource};

fn set0_path(name: &str) -> String {
    format!(
        "{}/../../internal/pkg/stitch/testdata/set0/{name}",
        env!("CARGO_MANIFEST_DIR")
    )
}

fn ffmpeg_available() -> bool {
    std::process::Command::new("ffprobe")
        .arg("-version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn run_set0(name: &str) -> Vec<Train> {
    let config = Config {
        pixels_per_m: 50.0,
        min_speed_kph: 10.0,
        max_speed_kph: 160.0,
        min_length_m: 10.0,
        max_frame_count_per_seq: 1500,
        mask: None,
    };

    let path = set0_path(name);
    let mut src = FileSrc::open(&path).unwrap_or_else(|e| panic!("open {name}: {e}"));
    let mut stitcher = AutoStitcher::new(config);
    let mut trains = Vec::new();

    loop {
        match src.next_frame() {
            Ok(Some(frame)) => {
                // Crop to 300×300 (matching Go test's `r = image.Rect(0, 0, 300, 300)`).
                let img =
                    image::imageops::crop_imm(&frame.image, 0, 0, 300, 300).to_image();
                if let Some(t) = stitcher.frame(img, frame.ts) {
                    trains.push(t);
                }
            }
            Ok(None) => break,
            Err(e) => panic!("{name}: frame error: {e}"),
        }
    }

    if let Some(t) = stitcher.try_stitch_and_reset() {
        trains.push(t);
    }

    trains
}

fn assert_near(got: f64, want: f64, tol: f64, label: &str) {
    assert!(
        (got - want).abs() <= tol,
        "{label}: got {got:.3}, want {want:.3} ± {tol}"
    );
}

#[test]
fn test_set0_day() {
    if !ffmpeg_available() {
        eprintln!("skip: ffprobe not found");
        return;
    }
    let trains = run_set0("day.mp4");
    assert_eq!(trains.len(), 1, "day: expected 1 train, got {}", trains.len());
    let t = &trains[0];
    assert_near(t.length_m(), 86.0, 5.0, "day length_m");
    assert_near(t.speed_m_ps(), 21.53, 0.1, "day speed_m_ps");
    assert_near(t.accel_m_ps2(), 0.27, 0.1, "day accel_m_ps2");
    assert!(!t.direction(), "day direction should be left");
}

#[test]
fn test_set0_night() {
    if !ffmpeg_available() {
        eprintln!("skip: ffprobe not found");
        return;
    }
    let trains = run_set0("night.mp4");
    assert_eq!(trains.len(), 1, "night: expected 1 train, got {}", trains.len());
    let t = &trains[0];
    assert_near(t.length_m(), 83.0, 5.0, "night length_m");
    assert_near(t.speed_m_ps(), 22.7, 0.1, "night speed_m_ps");
    assert_near(t.accel_m_ps2(), -0.5, 0.1, "night accel_m_ps2");
    assert!(t.direction(), "night direction should be right");
}

#[test]
fn test_set0_rain() {
    if !ffmpeg_available() {
        eprintln!("skip: ffprobe not found");
        return;
    }
    let trains = run_set0("rain.mp4");
    assert_eq!(trains.len(), 1, "rain: expected 1 train, got {}", trains.len());
    let t = &trains[0];
    assert_near(t.length_m(), 82.0, 5.0, "rain length_m");
    assert_near(t.speed_m_ps(), 17.9, 0.1, "rain speed_m_ps");
    assert_near(t.accel_m_ps2(), 0.0, 0.1, "rain accel_m_ps2");
    assert!(t.direction(), "rain direction should be right");
}

#[test]
fn test_set0_snow() {
    if !ffmpeg_available() {
        eprintln!("skip: ffprobe not found");
        return;
    }
    let trains = run_set0("snow.mp4");
    assert_eq!(trains.len(), 1, "snow: expected 1 train, got {}", trains.len());
    let t = &trains[0];
    assert_near(t.length_m(), 56.0, 5.0, "snow length_m");
    assert_near(t.speed_m_ps(), 20.5, 0.1, "snow speed_m_ps");
    assert_near(t.accel_m_ps2(), -0.75, 0.1, "snow accel_m_ps2");
    assert!(t.direction(), "snow direction should be right");
}
