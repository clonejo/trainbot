use image::RgbaImage;
use std::time::{Instant, SystemTime};
use tracing::{instrument, trace};
use vid::Frame;

use crate::fit::FitMethod;
use crate::metrics::{record_brightness, record_sequence_length, FrameDispositionGuard};
use crate::stitch::{fit_and_stitch, FitAndStitchError};
use crate::{Config, Sequence, Train};

const GOOD_COS_SCORE_NO_MOVE: f64 = 0.99;
const GOOD_COS_SCORE_MOVE: f64 = 0.925;
const MIN_FRAME_PERIOD_S: f64 = 0.01;
const MAX_FRAME_PERIOD_S: f64 = 10.0;
const DX_LOW_PASS_FACTOR: f64 = 0.99;
const MIN_CONTRAST_AVG_DEV: f64 = 0.01;

/// Stateful automatic train detector and stitcher.
///
/// Mirrors Go's `AutoStitcher`: accumulates frames, detects motion onset/end via
/// cosine-similarity patch matching, and calls `fit_and_stitch` when a sequence ends.
pub struct AutoStitcher {
    config: Config,
    fit_method: FitMethod,
    prev_ts: Option<Instant>,
    prev_frame: Option<RgbaImage>,
    seq: Sequence,
    dx_abs_low_pass: f64,
}

impl AutoStitcher {
    pub fn new(config: Config, fit_method: FitMethod) -> Self {
        Self {
            config,
            fit_method,
            prev_ts: None,
            prev_frame: None,
            seq: Sequence::new(),
            dx_abs_low_pass: 0.0,
        }
    }

    /// Search for the best horizontal offset of `curr`'s central strip inside a
    /// wider crop of `prev`.
    ///
    /// Returns `(dx, cos)` where `dx` is positive for leftward motion and negative
    /// for rightward (matching Go's convention).
    fn find_offset(prev: &RgbaImage, curr: &RgbaImage, max_dx: i32) -> (i32, f64) {
        let max_dx = max_dx as u32;
        let fw = prev.width();
        let fh = prev.height();

        // Centered crop from prev: width=3*maxDx, height=fh/2+1.
        let w_sub = max_dx * 3;
        let h_sub = (fh as f64 * 0.5 + 1.0) as u32;
        let x_sub = (fw - w_sub) / 2;
        let y_sub = (fh - h_sub) / 2;
        let sub = image::imageops::crop_imm(prev, x_sub, y_sub, w_sub, h_sub).to_image();

        // Centered strip from curr: width=maxDx, same height.
        let w_slice = max_dx;
        let x_slice = (fw - w_slice) / 2;
        let slice = image::imageops::crop_imm(curr, x_slice, y_sub, w_slice, h_sub).to_image();

        // The expected search result when there is no motion.
        let x_zero = x_slice as i32 - x_sub as i32;

        let (x, _y, cos) = pmatch::search_rgba(&sub, &slice);
        (x as i32 - x_zero, cos)
    }

    fn record(&mut self, prev_ts: Instant, img: RgbaImage, dx: i32, ts: Instant, wall: SystemTime) {
        if self.seq.start_ts.is_none() {
            self.seq.start_ts = Some(prev_ts);
        }
        if self.seq.start_wall.is_none() {
            self.seq.start_wall = Some(wall);
        }
        self.seq.images.push(img);
        self.seq.dx.push(dx);
        self.seq.ts.push(ts);
        record_sequence_length(self.seq.images.len());
    }

    /// Attempt to stitch any buffered sequence and reset state.
    pub fn try_stitch_and_reset(&mut self) -> Result<Option<Train>, AutoStitcherError> {
        let _guard = scopeguard::guard((), |_| {
            record_sequence_length(0);
        });

        // mem::replace to avoid 'cannot move out of self' error:
        let seq = std::mem::replace(&mut self.seq, Sequence::new());
        self.dx_abs_low_pass = 0.0;

        if seq.is_empty() {
            return Ok(None);
        }

        let result = fit_and_stitch(seq, &self.config, self.fit_method)?;
        Ok(Some(result))
    }

    /// Core per-frame logic (called with the current frame and previous state).
    ///
    /// Returns a `Train` if a sequence ended with this frame.
    #[instrument(level = "trace", skip(frame))]
    fn process_frame(&mut self, frame: &Frame) -> Result<Option<Train>, AutoStitcherError> {
        let ts = frame.ts;
        let wall = frame.wall;
        let img = &frame.image;

        let Some(prev_ts) = self.prev_ts else {
            return Ok(None);
        };

        let mut frame_disposition_guard = FrameDispositionGuard::new();

        let frame_period_s = ts.duration_since(prev_ts).as_secs_f64();
        if frame_period_s < MIN_FRAME_PERIOD_S {
            frame_disposition_guard.disposition = "fast_frame";
            return Ok(None);
        }

        let min_dx = self.config.min_px_per_frame(frame_period_s);
        let max_dx = self.config.max_px_per_frame(frame_period_s);

        if img.width() < max_dx as u32 * 3 {
            tracing::warn!(
                "frame too narrow for max speed: {}px < {}px",
                img.width(),
                max_dx * 3
            );
            frame_disposition_guard.disposition = "slow_frame";
            return Ok(None);
        }

        let is_active = !self.seq.is_empty();

        let (avg_ch, avg_dev) = avg::rgba(img);
        let avg_mean = (avg_ch[0] + avg_ch[1] + avg_ch[2]) / 3.0;
        let avg_dev_mean = (avg_dev[0] + avg_dev[1] + avg_dev[2]) / 3.0;
        record_brightness(avg_mean, avg_dev_mean);

        if avg_dev_mean < MIN_CONTRAST_AVG_DEV {
            frame_disposition_guard.disposition = "low_contrast";
            if is_active {
                let last_ts = *self.seq.ts.last().unwrap();
                let elapsed = ts.duration_since(last_ts).as_secs_f64();
                if elapsed > MAX_FRAME_PERIOD_S {
                    return self.try_stitch_and_reset();
                }
            }
            return Ok(None);
        }

        // Compute offset before any mutation of self (borrow-checker scope).
        let (dx, cos) = {
            let prev = self.prev_frame.as_ref().unwrap();
            Self::find_offset(prev, img, max_dx)
        };

        trace!(dx, cos, is_active, "frame offset");

        if is_active {
            self.dx_abs_low_pass = self.dx_abs_low_pass * DX_LOW_PASS_FACTOR
                + dx.unsigned_abs() as f64 * (1.0 - DX_LOW_PASS_FACTOR);

            if self.seq.len() > self.config.max_frame_count_per_seq {
                return self.try_stitch_and_reset();
            }

            if self.dx_abs_low_pass < min_dx as f64 {
                return self.try_stitch_and_reset();
            }

            self.record(prev_ts, img.clone(), dx, ts, wall);
            frame_disposition_guard.disposition = "recorded";
            return Ok(None);
        }

        // Not yet in a sequence.
        if cos >= GOOD_COS_SCORE_NO_MOVE && dx.unsigned_abs() < min_dx as u32 {
            frame_disposition_guard.disposition = "not_moving";
            return Ok(None);
        }

        if cos >= GOOD_COS_SCORE_MOVE
            && dx.unsigned_abs() >= min_dx as u32
            && dx.unsigned_abs() <= max_dx as u32
        {
            tracing::info!("start of new sequence");
            self.record(prev_ts, img.clone(), dx, ts, wall);
            self.dx_abs_low_pass = dx.unsigned_abs() as f64;
            frame_disposition_guard.disposition = "recorded_new_sequence";
            return Ok(None);
        }

        trace!(cos, dx, min_dx, max_dx, "inconclusive frame");
        frame_disposition_guard.disposition = "inconclusive";
        Ok(None)
    }

    /// Submit a frame.  Returns a `Train` if a complete sequence just finished.
    ///
    /// Always updates the previous-frame state (even on early return), matching
    /// Go's deferred assignment.
    pub fn frame(&mut self, frame: Frame) -> Result<Option<Train>, AutoStitcherError> {
        let result = self.process_frame(&frame);
        self.prev_ts = Some(frame.ts);
        self.prev_frame = Some(frame.image);
        result
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AutoStitcherError {
    #[error("rejected sequence: {0}")]
    FitAndStitchError(#[from] FitAndStitchError),
}
impl AutoStitcherError {
    pub fn video_data(&self) -> Option<&Vec<u8>> {
        if let AutoStitcherError::FitAndStitchError(FitAndStitchError::UnableToFit {
            video_data: Some(video_data),
            ..
        }) = self
        {
            return Some(video_data);
        }
        None
    }
}

impl std::fmt::Debug for AutoStitcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let prev_frame = self
            .prev_frame
            .as_ref()
            .map(|mask| format!("RgbaImage ({}x{})", mask.width(), mask.height()))
            .unwrap_or_default();
        f.debug_struct("AutoStitcher")
            .field("config", &self.config)
            .field("fit_method", &self.fit_method)
            .field("prev_ts", &self.prev_ts)
            .field("prev_frame", &prev_frame)
            .field("seq", &self.seq)
            .field("dx_abs_low_pass", &self.dx_abs_low_pass)
            .finish()
    }
}
