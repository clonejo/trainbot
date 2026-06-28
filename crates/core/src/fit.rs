use crate::sequence::Sequence;

/// Which robust fitting algorithm `fit_dx` uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
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

fn sign(x: f64) -> f64 {
    if x > 0.0 {
        1.0
    } else if x < 0.0 {
        -1.0
    } else {
        0.0
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
) -> Result<[f64; 2], String> {
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
            return Err(format!(
                "fit failed: too few inliers {} < {min_inliers}",
                inliers.len()
            ));
        }

        let (it, iv): (Vec<f64>, Vec<f64>) = inliers.into_iter().unzip();
        let new_params = ols_linear(&it, &iv).ok_or_else(|| "OLS on inliers failed".to_string())?;

        if (new_params[0] - params[0]).abs() < 1e-10 && (new_params[1] - params[1]).abs() < 1e-10 {
            return Ok(new_params);
        }
        params = new_params;
    }

    Ok(params)
}

/// Fit a constant-acceleration model to the sequence and return smoothed integer dx values.
///
/// Returns `(dx_fit, ds, v0, a)`:
/// - `dx_fit` – fitted per-frame pixel displacements (same length as `seq.dx`)
/// - `ds`     – estimated total displacement [px], always positive
/// - `v0`     – velocity at t=0 [px/s] (t measured from `seq.start_ts`)
/// - `a`      – acceleration [px/s²]
///
/// Both methods use the same hyper-parameters as Go's RANSAC:
/// threshold = 5% of max speed, min_inliers = n/2.
/// See `FitMethod` for per-method deviation notes.
pub(crate) fn fit_dx(
    seq: &Sequence,
    max_speed_px_s: f64,
    method: FitMethod,
) -> Result<(Vec<i32>, f64, f64, f64), String> {
    let n = seq.dx.len();
    if n < 9 {
        return Err(format!("sequence too short for fitting: {n} < 9"));
    }

    let start_ts = seq.start_ts.expect("start_ts must be set before fit_dx");

    // dt_complete[i] = seconds since previous frame (or startTS for i=0).
    // t_complete[i]  = seconds since startTS, only set for non-zero dx frames.
    let mut dt_complete = vec![0.0f64; n];
    let mut t_complete = vec![0.0f64; n]; // 0-initialized; zero-dx frames stay 0

    let mut t_fit: Vec<f64> = Vec::with_capacity(n);
    let mut v_fit: Vec<f64> = Vec::with_capacity(n);

    for i in 0..n {
        let dt_c = if i == 0 {
            seq.ts[i]
                .duration_since(start_ts)
                .unwrap_or_default()
                .as_secs_f64()
        } else {
            seq.ts[i]
                .duration_since(seq.ts[i - 1])
                .unwrap_or_default()
                .as_secs_f64()
        };
        dt_complete[i] = dt_c;

        if seq.dx[i] == 0 {
            continue;
        }

        let t_i = seq.ts[i]
            .duration_since(start_ts)
            .unwrap_or_default()
            .as_secs_f64();
        t_complete[i] = t_i;
        t_fit.push(t_i);
        v_fit.push(seq.dx[i] as f64 / dt_c);
    }

    let threshold = max_speed_px_s * 0.05;
    let min_inliers = v_fit.len() / 2;

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
        )
        .map_err(|e| e.to_string())?,
    };

    // Regenerate integer dx from the fitted model, accumulating and
    // redistributing rounding error to keep the sum consistent.
    let mut dx_fit = vec![0i32; n];
    let mut round_err = 0.0f64;
    for i in 0..n {
        let dx_f = (fit[0] + fit[1] * t_complete[i]) * dt_complete[i];
        let mut dx_round = dx_f.round();
        round_err += dx_f - dx_round;
        if round_err.abs() >= 0.5 {
            dx_round += round_err;
            round_err -= sign(round_err);
        }
        dx_fit[i] = dx_round as i32;
    }

    let v0 = fit[0];
    let a = fit[1];
    let t_last = *t_fit.last().expect("at least one non-zero dx");
    let ds = (v0 * t_last + 0.5 * a * t_last * t_last).abs();

    Ok((dx_fit, ds, v0, a))
}
