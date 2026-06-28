use image::RgbaImage;
use std::time::SystemTime;
use tracing::trace;

use crate::fit::FitMethod;
use crate::metrics::{record_brightness, record_frame_disposition, record_sequence_length};
use crate::stitch::fit_and_stitch;
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
    prev_ts: Option<SystemTime>,
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

    fn record(&mut self, prev_ts: SystemTime, frame: RgbaImage, dx: i32, ts: SystemTime) {
        if self.seq.start_ts.is_none() {
            self.seq.start_ts = Some(prev_ts);
        }
        self.seq.frames.push(frame);
        self.seq.dx.push(dx);
        self.seq.ts.push(ts);
        record_sequence_length(self.seq.frames.len());
    }

    /// Attempt to stitch any buffered sequence and reset state.
    pub fn try_stitch_and_reset(&mut self) -> Option<Train> {
        let seq = std::mem::replace(&mut self.seq, Sequence::new());
        self.dx_abs_low_pass = 0.0;
        record_sequence_length(0);

        if seq.is_empty() {
            return None;
        }

        fit_and_stitch(seq, &self.config, self.fit_method)
            .map_err(|e| tracing::debug!("rejected sequence: {e}"))
            .ok()
    }

    /// Core per-frame logic (called with the current frame and previous state).
    fn process_frame(&mut self, frame: &RgbaImage, ts: SystemTime) -> Option<Train> {
        let prev_ts = self.prev_ts?;

        let frame_period_s = ts.duration_since(prev_ts).unwrap_or_default().as_secs_f64();
        if frame_period_s < MIN_FRAME_PERIOD_S {
            return None;
        }

        let min_dx = self.config.min_px_per_frame(frame_period_s);
        let max_dx = self.config.max_px_per_frame(frame_period_s);

        if frame.width() < max_dx as u32 * 3 {
            tracing::warn!(
                "frame too narrow for max speed: {}px < {}px",
                frame.width(),
                max_dx * 3
            );
            record_frame_disposition("slow_frame");
            return None;
        }

        let is_active = !self.seq.is_empty();

        let (avg_ch, avg_dev) = avg::rgba(frame);
        let avg_mean = (avg_ch[0] + avg_ch[1] + avg_ch[2]) / 3.0;
        let avg_dev_mean = (avg_dev[0] + avg_dev[1] + avg_dev[2]) / 3.0;
        record_brightness(avg_mean, avg_dev_mean);

        if avg_dev_mean < MIN_CONTRAST_AVG_DEV {
            record_frame_disposition("low_contrast");
            if is_active {
                let last_ts = *self.seq.ts.last().unwrap();
                let elapsed = ts.duration_since(last_ts).unwrap_or_default().as_secs_f64();
                if elapsed > MAX_FRAME_PERIOD_S {
                    return self.try_stitch_and_reset();
                }
            }
            return None;
        }

        // Compute offset before any mutation of self (borrow-checker scope).
        let (dx, cos) = {
            let prev = self.prev_frame.as_ref().unwrap();
            Self::find_offset(prev, frame, max_dx)
        };

        tracing::debug!(dx, cos, is_active, "frame offset");

        if is_active {
            self.dx_abs_low_pass = self.dx_abs_low_pass * DX_LOW_PASS_FACTOR
                + dx.unsigned_abs() as f64 * (1.0 - DX_LOW_PASS_FACTOR);

            if self.seq.len() > self.config.max_frame_count_per_seq {
                return self.try_stitch_and_reset();
            }

            if self.dx_abs_low_pass < min_dx as f64 {
                return self.try_stitch_and_reset();
            }

            self.record(prev_ts, frame.clone(), dx, ts);
            record_frame_disposition("recorded");
            return None;
        }

        // Not yet in a sequence.
        if cos >= GOOD_COS_SCORE_NO_MOVE && dx.unsigned_abs() < min_dx as u32 {
            record_frame_disposition("not_moving");
            return None;
        }

        if cos >= GOOD_COS_SCORE_MOVE
            && dx.unsigned_abs() >= min_dx as u32
            && dx.unsigned_abs() <= max_dx as u32
        {
            tracing::info!("start of new sequence");
            self.record(prev_ts, frame.clone(), dx, ts);
            self.dx_abs_low_pass = dx.unsigned_abs() as f64;
            record_frame_disposition("recorded_new_sequence");
            return None;
        }

        tracing::debug!(cos, dx, min_dx, max_dx, "inconclusive frame");
        record_frame_disposition("inconclusive");
        None
    }

    /// Submit a frame.  Returns a `Train` if a complete sequence just finished.
    ///
    /// Always updates the previous-frame state (even on early return), matching
    /// Go's deferred assignment.
    pub fn frame(&mut self, frame: RgbaImage, ts: SystemTime) -> Option<Train> {
        let result = self.process_frame(&frame, ts);
        self.prev_ts = Some(ts);
        self.prev_frame = Some(frame);
        result
    }
}
