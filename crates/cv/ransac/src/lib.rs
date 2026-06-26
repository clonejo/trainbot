use nalgebra::{DMatrix, DVector};
use rand::{Rng, SeedableRng as _};

#[derive(Debug)]
pub enum Error {
    Unsuccessful,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "RANSAC unsuccessful")
    }
}

impl std::error::Error for Error {}

/// Meta-parameters controlling the RANSAC search.
#[derive(Debug, Clone)]
pub struct MetaParams {
    pub min_model_points: usize,
    pub max_iter: usize,
    pub min_inliers: usize,
    pub inlier_threshold: f64,
    pub seed: u64,
}

/// Draw `n` unique samples from `(x, y)` without replacement.
fn sample(rng: &mut impl Rng, x: &[f64], y: &[f64], n: usize) -> (Vec<f64>, Vec<f64>) {
    let mut used = vec![false; x.len()];
    let mut xs = Vec::with_capacity(n);
    let mut ys = Vec::with_capacity(n);
    while xs.len() < n {
        let i = rng.gen_range(0..x.len());
        if !used[i] {
            used[i] = true;
            xs.push(x[i]);
            ys.push(y[i]);
        }
    }
    (xs, ys)
}

/// Levenberg-Marquardt nonlinear least squares with numerical Jacobian.
/// Returns `(params, cost)` or `None` if the linear solve fails every iteration.
fn fit_nls(
    x: &[f64],
    y: &[f64],
    model: &dyn Fn(f64, &[f64]) -> f64,
    n_params: usize,
) -> Option<(Vec<f64>, f64)> {
    let n = x.len();
    let step = 1e-7;
    let mut params = vec![0.0f64; n_params];
    let mut lambda = 1e-3f64;

    for _ in 0..300 {
        // Residuals and numerical Jacobian.
        let mut r = DVector::<f64>::zeros(n);
        let mut j = DMatrix::<f64>::zeros(n, n_params);
        for i in 0..n {
            r[i] = model(x[i], &params) - y[i];
            for k in 0..n_params {
                let mut p = params.clone();
                p[k] += step;
                j[(i, k)] = (model(x[i], &p) - model(x[i], &params)) / step;
            }
        }

        // Damped normal equations: (J^T J + λI) δ = -J^T r
        let jtj = j.transpose() * &j;
        let jtr = j.transpose() * &r;
        let a = &jtj + lambda * DMatrix::<f64>::identity(n_params, n_params);
        let rhs = -&jtr;

        let svd = a.svd(true, true);
        let Ok(delta) = svd.solve(&rhs, 1e-10) else {
            lambda *= 10.0;
            continue;
        };

        let new_params: Vec<f64> = params
            .iter()
            .zip(delta.iter())
            .map(|(p, d)| p + d)
            .collect();

        let old_cost: f64 = r.iter().map(|ri| ri * ri).sum();
        let new_cost: f64 = x
            .iter()
            .zip(y.iter())
            .map(|(xi, yi)| (model(*xi, &new_params) - yi).powi(2))
            .sum();

        if new_cost <= old_cost {
            params = new_params;
            lambda = (lambda * 0.1).max(1e-16);
            if delta.norm() < 1e-12 {
                break;
            }
        } else {
            lambda = (lambda * 10.0).min(1e16);
        }
    }

    let cost: f64 = x
        .iter()
        .zip(y.iter())
        .map(|(xi, yi)| (model(*xi, &params) - yi).powi(2))
        .sum();

    Some((params, cost))
}

/// Run RANSAC to robustly fit `model` to noisy `(x, y)` data.
///
/// `model(x, params) -> y`; `n_params` is the number of parameters.
/// Returns the best-fit parameter vector, or `Error::Unsuccessful` if no
/// iteration produced enough inliers.
pub fn ransac(
    x: &[f64],
    y: &[f64],
    model: impl Fn(f64, &[f64]) -> f64,
    n_params: usize,
    p: MetaParams,
) -> Result<Vec<f64>, Error> {
    assert_eq!(x.len(), y.len(), "x and y must have the same length");
    assert!(n_params >= 1, "model must have at least one parameter");
    assert!(p.min_model_points >= 1);
    assert!(p.max_iter >= 1);
    assert!(p.min_inliers >= p.min_model_points);

    let mut rng = rand::rngs::SmallRng::seed_from_u64(p.seed);
    let mut best: Option<(Vec<f64>, f64)> = None;

    for _ in 0..p.max_iter {
        // Sample minimum set and fit hypothesis.
        let (xs, ys) = sample(&mut rng, x, y, p.min_model_points);
        let Some((hypo, _)) = fit_nls(&xs, &ys, &model, n_params) else {
            continue;
        };

        // Collect inliers.
        let (x_in, y_in): (Vec<f64>, Vec<f64>) = x
            .iter()
            .zip(y.iter())
            .filter(|(xi, yi)| (model(**xi, &hypo) - **yi).abs() < p.inlier_threshold)
            .map(|(xi, yi)| (*xi, *yi))
            .unzip();

        if x_in.len() < p.min_inliers {
            continue;
        }

        // Refit on inliers.
        let Some((params, cost)) = fit_nls(&x_in, &y_in, &model, n_params) else {
            continue;
        };

        if best.as_ref().is_none_or(|(_, bc)| cost < *bc) {
            best = Some((params, cost));
        }
    }

    best.map(|(params, _)| params).ok_or(Error::Unsuccessful)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_near(got: f64, want: f64, tol: f64, label: &str) {
        assert!(
            (got - want).abs() <= tol,
            "{label}: got {got}, want {want} ± {tol}"
        );
    }

    #[test]
    fn test_ransac_constant() {
        // Data is mostly constant at 34 with a few outliers.
        let test_data = [
            34, 34, 34, 34, 34, 26, 0, 34, 1, 1, 0, 0, 20, 0, 34, 34, 34, 34, 25, 34, 34, 34, 34,
            34, 34, 34, 34, 34, 22, 0, 34, 34, 28, 34, 34, 26, 27, 34, 34, 34, 34, 34, 34, 0, 0,
            34, 34, 34, 0, 34, 34, 34, 34, 1, 34, 34, 22, 34, 34, 34, 34, 0, 34, 0, 34, 34, 26, 34,
            34, 34, 3, 34, 34, 32, 34, 34, 34, 7, 0, 34, 0, 34, 1, 34, 34, 0, 34, 34, 5, 34, 5, 34,
            27, 0, 0, 34, 34, 34, 34, 34, 32, 31, 34, 34, 29, 25, 34, 10, 0, 6, 0, 34, 0, 34, 1,
            24, 34, 34, 35,
        ];
        let xf: Vec<f64> = (0..test_data.len()).map(|i| i as f64).collect();
        let yf: Vec<f64> = test_data.iter().map(|&v| v as f64).collect();

        // model: params[0] + params[1]*x²
        let model = |x: f64, ps: &[f64]| ps[0] + ps[1] * x * x;

        let fit = ransac(
            &xf,
            &yf,
            model,
            2,
            MetaParams {
                min_model_points: 3,
                max_iter: 10,
                min_inliers: xf.len() / 2,
                inlier_threshold: 2.0,
                seed: 123,
            },
        )
        .unwrap();

        assert_near(fit[0], 34.0, 0.1, "constant term");
        assert_near(fit[1], 0.0, 0.001, "quadratic term");
    }

    #[test]
    fn test_ransac_linear() {
        let x = vec![
            0.0, 1., 2., 3., 4., 5., 6., 7., 8., 9., 10., 11., 12., 13., 14., 15., 16., 17., 18.,
            19., 20., 21., 22., 23., 24., 25., 26., 27., 28., 29., 30., 31., 32., 33., 34., 35.,
            36., 37., 38., 39., 40., 41., 42., 43., 44., 45., 46., 47., 48., 49., 50.,
        ];
        let y = vec![
            -97.78, 28.13, -168.58, 50.80, 61.93, 56.98, 82.35, 301.81, 106.61, 41.08, 129.21,
            140.78, 155.03, 167.81, 180.02, 382.00, 204.80, 218.80, 230.70, 247.93, 262.02, 275.77,
            286.56, 301.00, 315.51, 333.54, 347.28, 361.53, 377.77, 393.67, 411.54, 424.81, 444.14,
            456.44, 476.06, 494.24, 511.96, 289.42, 584.95, 562.87, 465.45, 599.09, 617.92, 634.42,
            653.89, 674.67, 690.20, 709.56, 980.63, 749.58, 572.2,
        ];

        // model: params[0] + params[1]*x + 0.5*params[2]*x²
        let model = |x: f64, ps: &[f64]| ps[0] + ps[1] * x + 0.5 * ps[2] * x * x;

        let fit = ransac(
            &x,
            &y,
            model,
            3,
            MetaParams {
                min_model_points: 4,
                max_iter: 20,
                min_inliers: x.len() / 2,
                inlier_threshold: 5.0,
                seed: 123,
            },
        )
        .unwrap();

        assert_near(fit[0], 20.0, 5.0, "intercept");
        assert_near(fit[1], 10.0, 1.0, "linear term");
        assert_near(fit[2], 0.2, 0.015, "quadratic term");
    }
}
