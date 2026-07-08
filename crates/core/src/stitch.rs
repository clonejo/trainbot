use image::RgbaImage;
use thiserror::Error;

use crate::fit::{fit_dx, FitDxError, FitMethod};
use crate::gif::create_gif;
use crate::metrics::record_fit_and_stitch_result;
use crate::video::{self, create_video};
use crate::{Config, Sequence, Train};

#[derive(Debug, Error)]
pub enum StitchError {
    #[error("sequence too short to stitch")]
    TooShort,
    #[error("dx elements do not have consistent sign")]
    InconsistentSign,
    #[error("would allocate too much memory: {width}x{height}")]
    TooLarge { width: u32, height: u32 },
}

#[derive(Debug, Error)]
pub enum FitAndStitchError {
    #[error("unable to fit: {fit_dx_error}")]
    UnableToFit {
        fit_dx_error: FitDxError,
        video_data: Option<Vec<u8>>,
    },
    #[error("too short: {actual:.1} < {min:.1}")]
    TooShort { actual: f64, min: f64 },
    #[error("too slow: {actual:.1} < {min:.1}")]
    TooSlow { actual: f64, min: f64 },
    #[error("unable to assemble image: {0}")]
    UnableToAssembleImage(#[from] StitchError),
    #[error("Could not generate video: {0}")]
    Video(#[from] video::VideoError),
}

/// Composite `frames` into a panoramic RGBA image using the integer offsets `dx`.
///
/// All dx values must have the same sign (direction of motion).
/// When `mask` is provided, each frame pixel is composited with the mask's alpha
/// channel as the effective source alpha (Porter-Duff Over), matching Go's
/// `draw.DrawMask(..., draw.Over)`.  Without a mask, `imageops::overlay` is used
/// (equivalent to `draw.Src` for opaque frames).
pub(crate) fn stitch(
    frames: &[RgbaImage],
    dx: &[i32],
    mask: Option<&RgbaImage>,
) -> Result<RgbaImage, StitchError> {
    if dx.len() < 2 {
        return Err(StitchError::TooShort);
    }

    let fw = frames[0].width() as i32;
    let fh = frames[0].height() as i32;

    let sign = dx[0].signum();
    let mut w = fw * sign;
    for &x in &dx[1..] {
        if x.signum() != sign {
            return Err(StitchError::InconsistentSign);
        }
        w += x;
    }

    let img_w = w.unsigned_abs();
    let img_h = fh as u32;

    const MAX_MEMORY_BYTES: usize = 1024 * 1024 * 100;
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
            composite(&mut img, frame, mask, pos, 0);
            pos += dx[i] as i64;
        }
    } else {
        // Backward (rightward train): earlier frames on the right.
        let mut pos: i64 = (-w - fw) as i64;
        for (i, frame) in frames.iter().enumerate() {
            composite(&mut img, frame, mask, pos, 0);
            pos += dx[i] as i64;
        }
    }

    Ok(img)
}

/// Porter-Duff Over: draws `frame` onto `dst` at (`x`, `y`), optionally gated
/// by `mask`'s alpha channel.  Matches Go's `draw.DrawMask(..., draw.Over)` when
/// a mask is present, and `image::imageops::overlay` (same Op) when not.
///
/// **Assumes opaque frames** (src alpha = 255 for all pixels), which is always
/// true for video frames.  Under that assumption the effective source alpha is
/// simply `mask_a`, and the Porter-Duff Over formula reduces to:
///
/// ```text
/// out_a  = mask_a + dst_a * (255 - mask_a) / 255
/// out_ch = (src_ch * mask_a + dst_ch * dst_a * (255 - mask_a) / 255) / out_a
/// ```
fn composite(dst: &mut RgbaImage, frame: &RgbaImage, mask: Option<&RgbaImage>, x: i64, y: i64) {
    match mask {
        None => image::imageops::overlay(dst, frame, x, y),
        Some(mask) => {
            let dst_w = dst.width() as i64;
            let dst_h = dst.height() as i64;
            for (fx, fy, src) in frame.enumerate_pixels() {
                let mask_a = mask.get_pixel(fx, fy)[3] as u32;
                if mask_a == 0 {
                    continue;
                }
                let px = x + fx as i64;
                let py = y + fy as i64;
                if px < 0 || py < 0 || px >= dst_w || py >= dst_h {
                    continue;
                }
                // src alpha = 255 (opaque frame), so effective src_a = mask_a.
                let inv_mask_a = 255 - mask_a;
                let dst_px = dst.get_pixel_mut(px as u32, py as u32);
                let dst_a = dst_px[3] as u32;
                let out_a = mask_a + dst_a * inv_mask_a / 255;
                if out_a == 0 {
                    continue;
                }
                dst_px[0] = ((src[0] as u32 * mask_a + dst_px[0] as u32 * dst_a * inv_mask_a / 255)
                    / out_a) as u8;
                dst_px[1] = ((src[1] as u32 * mask_a + dst_px[1] as u32 * dst_a * inv_mask_a / 255)
                    / out_a) as u8;
                dst_px[2] = ((src[2] as u32 * mask_a + dst_px[2] as u32 * dst_a * inv_mask_a / 255)
                    / out_a) as u8;
                dst_px[3] = out_a as u8;
            }
        }
    }
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
    // max_px_per_frame(1) = max pixels/frame at 1 fps = max speed in px/s.
    let max_speed_px_s = config.max_px_per_frame(1.0) as f64;
    let (dx_fit, ds, v0, a) = fit_dx(&seq, max_speed_px_s, method).map_err(|fit_dx_error| {
        record_fit_and_stitch_result("unable_to_fit");
        let video_data = match fit_dx_error {
            FitDxError::TooShort { .. } => None,
            _ => create_video(&seq, config.video_encoder).ok(),
        };
        FitAndStitchError::UnableToFit {
            fit_dx_error,
            video_data,
        }
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

    let img = stitch(&seq.frames, &dx_fit, config.mask.as_ref()).map_err(|e| {
        record_fit_and_stitch_result("unable_to_assemble_image");
        FitAndStitchError::from(e)
    })?;
    let gif_data = create_gif(&seq, &img);
    let video_data = create_video(&seq, config.video_encoder)?;
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
        video_data,
    })
}
