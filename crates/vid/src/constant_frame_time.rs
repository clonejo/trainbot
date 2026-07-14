use std::time::{Duration, Instant};

use crate::{Frame, FrameSource, Result};

/// Pretend the frame source is providing frames at a constant rate.
pub struct ConstantFrameTime {
    src: Box<dyn FrameSource + Send>,
    constant_frame_time: Duration,
    next_frame_ts: Instant,
}

impl ConstantFrameTime {
    pub fn new(src: Box<dyn FrameSource + Send>, constant_frame_time: Duration) -> Self {
        Self {
            src,
            next_frame_ts: Instant::now(),
            constant_frame_time,
        }
    }
}

impl FrameSource for ConstantFrameTime {
    fn next_frame(&mut self) -> Result<Option<Frame>> {
        let frame = self.src.next_frame()?.map(|mut frame| {
            frame.ts = self.next_frame_ts;
            self.next_frame_ts += self.constant_frame_time;
            frame
        });
        Ok(frame)
    }

    fn fps(&self) -> f64 {
        self.src.fps()
    }

    fn is_live(&self) -> bool {
        self.src.is_live()
    }
}
