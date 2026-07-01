use image::RgbaImage;
use std::time::SystemTime;

use crate::Config;

pub struct Train {
    pub start_ts: SystemTime,
    pub n_frames: usize,
    /// Always positive (absolute pixel length of the stitched image).
    pub length_px: f64,
    /// Positive = rightward, negative = leftward.
    pub speed_px_s: f64,
    pub accel_px_s2: f64,
    pub conf: Config,
    pub image: RgbaImage,
    pub gif_data: Vec<u8>,
    pub video_data: Vec<u8>,
}

impl Train {
    pub fn length_m(&self) -> f64 {
        self.length_px.abs() / self.conf.pixels_per_m
    }

    pub fn speed_m_ps(&self) -> f64 {
        self.speed_px_s.abs() / self.conf.pixels_per_m
    }

    /// Positive = accelerating in the direction of travel, negative = decelerating.
    pub fn accel_m_ps2(&self) -> f64 {
        self.accel_px_s2 / self.conf.pixels_per_m * self.speed_px_s.signum()
    }

    /// `true` = rightward, `false` = leftward.
    pub fn direction(&self) -> bool {
        self.speed_px_s > 0.0
    }
}
