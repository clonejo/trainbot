use image::RgbaImage;
use thiserror::Error;

use crate::fit::{fit_dx, FitDxError, FitMethod};
use crate::gif::create_gif;
use crate::metrics::{record_fit_and_stitch_result, record_sequence_length};
use crate::{Config, Sequence, Train};

#[derive(Debug, Error)]
pub(crate) enum StitchError {
    #[error("sequence too short to stitch")]
    TooShort,
    #[error("dx elements do not have consistent sign")]
    InconsistentSign,
    #[error("would allocate too much memory: {width}x{height}")]
    TooLarge { width: u32, height: u32 },
}

#[derive(Debug, Error)]
pub(crate) enum FitAndStitchError {
    #[error("unable to fit: {0}")]
    UnableToFit(#[from] FitDxError),
    #[error("too short: {actual:.1} < {min:.1}")]
    TooShort { actual: f64, min: f64 },
    #[error("too slow: {actual:.1} < {min:.1}")]
    TooSlow { actual: f64, min: f64 },
    #[error("unable to assemble image: {0}")]
    UnableToAssembleImage(#[from] StitchError),
}

/// Composite `frames` into a panoramic RGBA image using the integer offsets `dx`.
///
/// All dx values must have the same sign (direction of motion).
///
/// **Deviation from Go**: Go's `stitch()` accepts an optional mask and switches
/// between `draw.Src` (no mask — replaces destination pixels wholesale) and
/// `draw.Over` (mask — alpha compositing).  This implementation always uses
/// `image::imageops::overlay` (Porter-Duff Over).  For opaque video frames
/// (alpha=255 throughout) the two are pixel-identical.  Masked compositing is
/// not implemented; `Config::mask` is currently ignored.
pub(crate) fn stitch(frames: &[RgbaImage], dx: &[i32]) -> Result<RgbaImage, StitchError> {
    if dx.len() < 2 {
        return Err(StitchError::TooShort);
    }

    let fw = frames[0].width() as i32;
    let fh = frames[0].height() as i32;

    let sign = i_sign(dx[0]);
    let mut w = fw * sign;
    for &x in &dx[1..] {
        if i_sign(x) != sign {
            return Err(StitchError::InconsistentSign);
        }
        w += x;
    }

    let img_w = w.unsigned_abs();
    let img_h = fh as u32;

    const MAX_MEMORY_BYTES: usize = 1024 * 1024 * 50;
    if (img_w as usize) * (img_h as usize) * 4 > MAX_MEMORY_BYTES {
        return Err(StitchError::TooLarge {
            width: img_w,
            height: img_h,
        });
    }

    let mut img = RgbaImage::new(img_w, img_h);

    if w > 0 {
        // Forward (leftward train): earlier frames on the left.
        let mut pos: i64 = 0;
        for (i, frame) in frames.iter().enumerate() {
            image::imageops::overlay(&mut img, frame, pos, 0);
            pos += dx[i] as i64;
        }
    } else {
        // Backward (rightward train): earlier frames on the right.
        let mut pos: i64 = (-w - fw) as i64;
        for (i, frame) in frames.iter().enumerate() {
            image::imageops::overlay(&mut img, frame, pos, 0);
            pos += dx[i] as i64;
        }
    }

    Ok(img)
}

/// Fit the constant-acceleration model, validate, stitch frames, and create GIF.
///
/// Modifies `seq` in-place (strips trailing zero-dx entries).
/// Returns `Err` with a descriptive message if the sequence is rejected.
pub(crate) fn fit_and_stitch(
    mut seq: Sequence,
    config: &Config,
    method: FitMethod,
) -> Result<Train, FitAndStitchError> {
    // Strip trailing zero-dx frames (mirrors Go's `fitAndStitch`).
    while !seq.dx.is_empty() && *seq.dx.last().unwrap() == 0 {
        seq.dx.pop();
        seq.ts.pop();
        seq.frames.pop();
    }
    record_sequence_length(seq.frames.len());

    // max_px_per_frame(1) = max pixels/frame at 1 fps = max speed in px/s.
    let max_speed_px_s = config.max_px_per_frame(1.0) as f64;
    let (dx_fit, ds, v0, a) = fit_dx(&seq, max_speed_px_s, method).map_err(|e| {
        record_fit_and_stitch_result("unable_to_fit");
        FitAndStitchError::from(e)
    })?;

    if ds < config.min_length_px() {
        record_fit_and_stitch_result("too_short");
        return Err(FitAndStitchError::TooShort {
            actual: ds,
            min: config.min_length_px(),
        });
    }

    // Evaluate speed at the midpoint.
    // Go uses ts[0] (first recorded frame) as the time base, not startTS.
    // Because v0/a are fitted with t=0 at startTS, this introduces a ~1 frame
    // bias in the speed estimate.  Preserved intentionally for parity with Go.
    let t_mid = seq.ts[seq.ts.len() / 2]
        .duration_since(seq.ts[0])
        .unwrap_or_default()
        .as_secs_f64();
    let speed = v0 + a * t_mid;

    if speed.abs() < config.min_speed_px_ps() {
        record_fit_and_stitch_result("too_slow");
        return Err(FitAndStitchError::TooSlow {
            actual: speed.abs(),
            min: config.min_speed_px_ps(),
        });
    }

    let img = stitch(&seq.frames, &dx_fit).map_err(|e| {
        record_fit_and_stitch_result("unable_to_assemble_image");
        FitAndStitchError::from(e)
    })?;
    let gif_data = create_gif(&seq, &img);
    record_fit_and_stitch_result("success");

    Ok(Train {
        start_ts: seq.ts[0],
        n_frames: seq.frames.len(),
        length_px: ds,
        // Negate: leftward motion produces positive dx, but SpeedPxS > 0 means right.
        speed_px_s: -speed,
        accel_px_s2: -a,
        conf: config.clone(),
        image: img,
        gif_data,
    })
}

fn i_sign(x: i32) -> i32 {
    if x > 0 {
        1
    } else if x < 0 {
        -1
    } else {
        0
    }
}
