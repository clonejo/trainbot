use image::RgbaImage;
use std::time::{Instant, SystemTime};

pub(crate) struct Sequence {
    /// Wall clock time of the first frame.
    pub start_wall: Option<SystemTime>,
    /// Timestamp of the frame immediately before the first recorded frame.
    pub start_ts: Option<Instant>,
    pub images: Vec<RgbaImage>,
    pub dx: Vec<i32>,
    pub ts: Vec<Instant>,
}

impl Sequence {
    pub fn new() -> Self {
        Self {
            start_wall: None,
            start_ts: None,
            images: Vec::new(),
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

impl std::fmt::Debug for Sequence {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sequence")
            .field("start_ts", &self.start_wall)
            .field("frames", &format_args!("[{} elements]", self.images.len()))
            .field("dx", &format_args!("[{} elements]", self.dx.len()))
            .field("ts", &format_args!("[{} elements]", self.ts.len()))
            .finish()
    }
}
