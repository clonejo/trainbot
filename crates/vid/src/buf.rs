use crossbeam_channel::{Receiver, TrySendError, bounded};

use crate::{Frame, FrameSource, Result};

/// Allow taking in frames at a constant rate, while processing is variable.
/// An added benefit is moving JPEG decode into a separate thread.
pub struct BufSrc {
    fps: f64,
    rx: Receiver<Result<Option<Frame>>>,
    _thread: std::thread::JoinHandle<()>,
}

impl BufSrc {
    pub fn new(mut src: Box<dyn FrameSource + Send>, cap: usize) -> Self {
        assert!(src.is_live(), "BufSrc only wraps live sources");
        let fps = src.fps();
        let (tx, rx) = bounded(cap);
        let thread = std::thread::spawn(move || {
            let mut dropped_in_a_row: u32 = 0;
            loop {
                let item = src.next_frame();
                let done = matches!(item, Ok(None) | Err(_));
                match tx.try_send(item) {
                    Ok(()) => {
                        dropped_in_a_row = 0;
                        record_frame_buffered("buffered");
                        record_source_queue_length(tx.len());
                    }
                    Err(TrySendError::Full(_)) => {
                        dropped_in_a_row += 1;
                        if dropped_in_a_row % 60 == 1 {
                            tracing::warn!(dropped_in_a_row, "frame buffer full, dropping frame");
                        }
                        record_frame_buffered("dropped");
                    }
                    Err(TrySendError::Disconnected(_)) => break,
                }
                if done {
                    break;
                }
            }
        });
        Self {
            fps,
            rx,
            _thread: thread,
        }
    }
}

impl FrameSource for BufSrc {
    fn next_frame(&mut self) -> Result<Option<Frame>> {
        let frame = self.rx.recv().unwrap_or(Ok(None));
        record_source_queue_length(self.rx.len());
        frame
    }

    fn fps(&self) -> f64 {
        self.fps
    }

    fn is_live(&self) -> bool {
        true
    }
}

// prometheus metrics
fn record_frame_buffered(buffered: &'static str) {
    metrics::counter!("trainbot_frame_buffered_total", "buffered" => buffered).increment(1);
}
fn record_source_queue_length(len: usize) {
    metrics::gauge!("trainbot_source_queue_length").set(len as f64);
}
