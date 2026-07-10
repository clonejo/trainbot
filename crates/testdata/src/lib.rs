use std::str::FromStr as _;

use camino::Utf8PathBuf;

pub fn testdata_path(name: &str) -> Utf8PathBuf {
    Utf8PathBuf::from_str(env!("CARGO_MANIFEST_DIR"))
        .unwrap()
        .join("../../internal/pkg/stitch/testdata")
        .join(name)
}

pub fn assert_ffmpeg_available() {
    let ffmpeg_available = std::process::Command::new("ffprobe")
        .arg("-version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !ffmpeg_available {
        panic!("ffprobe not found");
    }
}
pub fn assert_near(got: f64, want: f64, tol: f64, label: &str) {
    assert!(
        (got - want).abs() <= tol,
        "{label}: got {got:.3}, want {want:.3} ± {tol}"
    );
}
