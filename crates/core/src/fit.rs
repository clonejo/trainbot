use clap::ValueEnum;
use thiserror::Error;

use crate::sequence::Sequence;

#[derive(Debug, Error)]
pub enum FitLinearRobustError {
    #[error("fit failed: too few inliers {found} < {min}")]
    TooFewInliers { found: usize, min: usize },
    #[error("OLS on inliers failed")]
    OlsDegenerate,
}

#[derive(Debug, Error)]
pub enum FitDxError {
    #[error("sequence too short for fitting: {n} < 9")]
    TooShort { n: usize },
    #[error(transparent)]
    RobustFit(#[from] FitLinearRobustError),
    #[error("RANSAC fit failed: {0}")]
    Ransac(#[from] ransac::Error),
}

/// Which robust fitting algorithm `fit_dx` uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
#[value(rename_all = "lower")]
pub enum FitMethod {
    /// Deterministic iterative OLS with median-seeded outlier rejection.
    /// Matches Go within the standard test tolerances for all set0 videos.
    #[default]
    Ols,
    /// RANSAC using `rand::SmallRng` (seed=0).
    ///
    /// **Deviation from Go**: Go uses `math/rand` (lagged-Fibonacci, seed=0); Rust's
    /// `SmallRng` (Xorshift128+) produces a different random sequence, leading to
    /// slightly different inlier sets.  For the snow video this causes a ~0.12 m/s
    /// speed difference vs Go's output, which is outside the ±0.1 m/s test tolerance.
    Ransac,
}

pub struct FitDx {
    /// fitted per-frame pixel displacements (same length as `seq.dx`)
    pub dx_fit: Vec<i32>,
    /// estimated total displacement [px], always positive
    pub ds: f64,
    /// velocity at t=0 [px/s] (t measured from `seq.start_ts`)
    pub v0: f64,
    /// acceleration [px/s²]
    pub a: f64,
}

/// Fit a constant-acceleration model to the sequence and return smoothed integer dx values.
///
/// Both methods use the same hyper-parameters as Go's RANSAC:
/// threshold = 5% of max speed, min_inliers = n/2.
/// See `FitMethod` for per-method deviation notes.
pub(crate) fn fit_dx(
    seq: &Sequence,
    max_speed_px_s: f64,
    method: FitMethod,
) -> Result<FitDx, FitDxError> {
    let mut debug_plot = debug_plot::DebugPlot::new(seq);

    let n = seq.dx.len();
    if n < 9 {
        return Err(FitDxError::TooShort { n });
    }

    let start_ts = seq.start_ts.expect("start_ts must be set before fit_dx");

    // FIXME: rewrite this with iterators and zip/unzip
    // dt_complete[i] = seconds since previous frame (or startTS for i=0).
    // t_complete[i]  = seconds since startTS, only set for non-zero dx frames.
    let mut dt_complete = Vec::with_capacity(n);
    let mut t_complete = Vec::with_capacity(n);

    let mut t_fit: Vec<f64> = Vec::with_capacity(n);
    let mut v_fit: Vec<f64> = Vec::with_capacity(n);

    for i in 0..n {
        let dt_c = if i == 0 {
            seq.ts[i].duration_since(start_ts).as_secs_f64()
        } else {
            seq.ts[i].duration_since(seq.ts[i - 1]).as_secs_f64()
        };
        dt_complete.push(dt_c);

        let t_i = seq.ts[i].duration_since(start_ts).as_secs_f64();
        t_complete.push(t_i);

        if seq.dx[i] == 0 {
            continue;
        }

        t_fit.push(t_i);
        v_fit.push(seq.dx[i] as f64 / dt_c);
    }

    let threshold = max_speed_px_s * 0.05;
    let min_inliers = v_fit.len() / 2;

    // FIXME: switch from Vec to some more useful type, maybe Fit<const N> {[f64; N]}
    let fit: Vec<f64> = match method {
        FitMethod::Ols => fit_linear_robust(&t_fit, &v_fit, threshold, min_inliers)?.to_vec(),
        FitMethod::Ransac => ransac::ransac(
            &t_fit,
            &v_fit,
            |t, p| p[0] + p[1] * t,
            2,
            ransac::MetaParams {
                min_model_points: 3,
                max_iter: 25,
                min_inliers,
                inlier_threshold: threshold,
                seed: 0,
            },
        )?,
    };

    let v0 = fit[0];
    let a = fit[1];

    // Regenerate integer dx from fitted model:
    let mut dx_fit: Vec<i32> = Vec::with_capacity(n);
    let mut prev_x = 0;
    for t in t_complete.iter() {
        let x_fit = (v0 * t + a * t * t / 2.0).round() as i32;
        dx_fit.push(x_fit - prev_x);
        prev_x = x_fit;
    }
    debug_plot.add_fit(seq, &dt_complete, &t_complete, &fit, &dx_fit);

    let t_last = *t_fit.last().expect("at least one non-zero dx");
    let ds = (v0 * t_last + 0.5 * a * t_last * t_last).abs();

    Ok(FitDx { dx_fit, ds, v0, a })
}

#[cfg(not(feature = "debug-fit"))]
/// just a stub without feature "debug-fit"
mod debug_plot {
    use crate::sequence::Sequence;

    pub(crate) struct DebugPlot {}
    impl DebugPlot {
        pub(crate) fn new(_: &Sequence) -> DebugPlot {
            DebugPlot {}
        }
        pub(crate) fn add_fit(&mut self, _: &Sequence, _: &[f64], _: &[f64], _: &[f64], _: &[i32]) {
        }
    }
}
#[cfg(feature = "debug-fit")]
mod debug_plot {
    use plotters::{coord::types::RangedCoordf64, prelude::*};

    use crate::sequence::Sequence;

    pub(crate) struct DebugPlot<'a> {
        first_ts: std::time::Instant,
        ctx: ChartContext<'a, BitMapBackend<'a>, Cartesian2d<RangedCoordf64, RangedCoordf64>>,
    }
    impl DebugPlot<'_> {
        pub(crate) fn new(seq: &Sequence) -> DebugPlot<'_> {
            let root_area = BitMapBackend::new("fit_dx.png", (800, 600)).into_drawing_area();
            root_area.fill(&WHITE).unwrap();
            let first_ts = *seq.ts.first().unwrap();
            let mut ctx = ChartBuilder::on(&root_area)
                .set_label_area_size(LabelAreaPosition::Left, 40)
                .set_label_area_size(LabelAreaPosition::Bottom, 40)
                .caption("fit_dx()", ("sans-serif", 40))
                .build_cartesian_2d(
                    0.0..seq
                        .ts
                        .last()
                        .unwrap()
                        .duration_since(first_ts)
                        .as_secs_f64()
                        .ceil(),
                    f64::from(*seq.dx.iter().min().unwrap()) - 2.0
                        ..f64::from(*seq.dx.iter().max().unwrap()) + 2.0,
                )
                .unwrap();

            ctx.configure_mesh().draw().unwrap();

            ctx.draw_series(seq.ts.iter().zip(&seq.dx).map(|(ts, dx)| {
                Cross::new(
                    (ts.duration_since(first_ts).as_secs_f64(), f64::from(*dx)),
                    1,
                    BLACK,
                )
            }))
            .unwrap()
            .label("recorded (dx)")
            .legend(|(x, y)| Cross::new((x + 10, y), 1, BLACK));

            DebugPlot { first_ts, ctx }
        }
        pub(crate) fn add_fit(
            &mut self,
            seq: &Sequence,
            dt_complete: &[f64],
            t_complete: &[f64],
            fit: &[f64],
            dx_fit: &[i32],
        ) {
            // fit function
            self.ctx
                .draw_series(LineSeries::new(
                    vec![
                        (
                            t_complete[0],
                            (fit[0] + fit[1] * t_complete[0]) * dt_complete[0],
                        ),
                        (
                            *t_complete.last().unwrap(),
                            (fit[0] + fit[1] * t_complete.last().unwrap())
                                * dt_complete.last().unwrap(),
                        ),
                    ],
                    &GREEN,
                ))
                .unwrap()
                .label("fitted (fit)")
                .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], GREEN));

            // fit function integer quantized
            self.ctx
                .draw_series(seq.ts.iter().zip(dx_fit).map(|(ts, dx)| {
                    Circle::new(
                        (
                            ts.duration_since(self.first_ts).as_secs_f64(),
                            f64::from(*dx),
                        ),
                        3,
                        RED.mix(0.4),
                    )
                }))
                .unwrap()
                .label("fitted as int (dx_fit)")
                .legend(|(x, y)| Circle::new((x + 10, y), 3, RED));

            self.ctx
                .configure_series_labels()
                .border_style(BLACK)
                .background_style(WHITE.mix(0.8))
                .draw()
                .unwrap();
        }
    }
}

/// Ordinary least squares for the linear model v(t) = v0 + a*t.
/// Returns `[v0, a]` or `None` if degenerate.
fn ols_linear(t: &[f64], v: &[f64]) -> Option<[f64; 2]> {
    let n = t.len() as f64;
    if n < 2.0 {
        return None;
    }
    let sum_t: f64 = t.iter().sum();
    let sum_v: f64 = v.iter().sum();
    let sum_t2: f64 = t.iter().map(|ti| ti * ti).sum();
    let sum_tv: f64 = t.iter().zip(v.iter()).map(|(ti, vi)| ti * vi).sum();
    let denom = n * sum_t2 - sum_t * sum_t;
    if denom.abs() < 1e-15 {
        return None;
    }
    let a = (n * sum_tv - sum_t * sum_v) / denom;
    let v0 = (sum_v - a * sum_t) / n;
    Some([v0, a])
}

/// Robust linear fit using iterative outlier rejection.
///
/// Seeds the iteration with (median_v, 0) — robust to outliers — then
/// iteratively removes points with |residual| ≥ threshold and refits via OLS.
/// Deterministic — no random sampling.
fn fit_linear_robust(
    t: &[f64],
    v: &[f64],
    threshold: f64,
    min_inliers: usize,
) -> Result<[f64; 2], FitLinearRobustError> {
    // Robust initial estimate: median velocity, zero acceleration.
    let mut v_sorted = v.to_vec();
    v_sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median_v = v_sorted[v_sorted.len() / 2];
    let mut params = [median_v, 0.0f64];

    for _ in 0..30 {
        let inliers: Vec<(f64, f64)> = t
            .iter()
            .zip(v.iter())
            .filter(|(ti, vi)| (params[0] + params[1] * *ti - *vi).abs() < threshold)
            .map(|(ti, vi)| (*ti, *vi))
            .collect();

        if inliers.len() < min_inliers {
            return Err(FitLinearRobustError::TooFewInliers {
                found: inliers.len(),
                min: min_inliers,
            });
        }

        let (it, iv): (Vec<f64>, Vec<f64>) = inliers.into_iter().unzip();
        let new_params = ols_linear(&it, &iv).ok_or(FitLinearRobustError::OlsDegenerate)?;

        if (new_params[0] - params[0]).abs() < 1e-10 && (new_params[1] - params[1]).abs() < 1e-10 {
            return Ok(new_params);
        }
        params = new_params;
    }

    Ok(params)
}
