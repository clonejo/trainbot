use image::RgbaImage;
use std::time::SystemTime;

pub(crate) struct Sequence {
    /// Timestamp of the frame immediately before the first recorded frame.
    pub start_ts: Option<SystemTime>,
    pub frames: Vec<RgbaImage>,
    pub dx: Vec<i32>,
    pub ts: Vec<SystemTime>,
}

impl Sequence {
    pub fn new() -> Self {
        Self {
            start_ts: None,
            frames: Vec::new(),
            dx: Vec::new(),
            ts: Vec::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.dx.is_empty()
    }

    pub fn len(&self) -> usize {
        self.dx.len()
    }
}
