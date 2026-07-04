use image::RgbaImage;

use crate::video::Encoder;

#[derive(Clone)]
pub struct Config {
    pub pixels_per_m: f64,
    pub min_speed_kph: f64,
    pub max_speed_kph: f64,
    pub min_length_m: f64,
    pub max_frame_count_per_seq: usize,
    pub mask: Option<RgbaImage>,
    pub video_encoder: Encoder,
}

impl Config {
    /// Minimum pixel displacement per frame at the given inter-frame period.
    /// Matches Go's `int(MinSpeedKPH/3.6*PixelsPerM/fps) - 1`, clamped to ≥1.
    pub(crate) fn min_px_per_frame(&self, frame_period_s: f64) -> i32 {
        if frame_period_s == 0.0 {
            return 1;
        }
        let fps = 1.0 / frame_period_s;
        ((self.min_speed_kph / 3.6 * self.pixels_per_m / fps) as i32 - 1).max(1)
    }

    /// Maximum pixel displacement per frame at the given inter-frame period.
    /// Matches Go's `int(MaxSpeedKPH/3.6*PixelsPerM/fps) + 1`, clamped to ≥1.
    pub(crate) fn max_px_per_frame(&self, frame_period_s: f64) -> i32 {
        if frame_period_s == 0.0 {
            return 1;
        }
        let fps = 1.0 / frame_period_s;
        ((self.max_speed_kph / 3.6 * self.pixels_per_m / fps) as i32 + 1).max(1)
    }

    pub(crate) fn min_speed_px_ps(&self) -> f64 {
        self.min_speed_kph / 3.6 * self.pixels_per_m
    }

    pub(crate) fn min_length_px(&self) -> f64 {
        self.min_length_m * self.pixels_per_m
    }
}

impl std::fmt::Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mask = self
            .mask
            .as_ref()
            .map(|mask| format!("RgbaImage ({}x{})", mask.width(), mask.height()))
            .unwrap_or_default();
        f.debug_struct("Config")
            .field("pixels_per_m", &self.pixels_per_m)
            .field("min_speed_kph", &self.min_speed_kph)
            .field("max_speed_kph", &self.max_speed_kph)
            .field("min_length_m", &self.min_length_m)
            .field("max_frame_count_per_seq", &self.max_frame_count_per_seq)
            .field("mask", &mask)
            .field("video_encoder", &self.video_encoder)
            .finish()
    }
}
