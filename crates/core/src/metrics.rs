use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;

use metrics_exporter_prometheus::{Matcher, PrometheusBuilder};

/// Install the Prometheus recorder and serve `/metrics` on `addr`.
///
/// Silently returns if a global recorder is already installed.
pub fn init_metrics(addr: &str) {
    let buckets = brightness_buckets();

    let recorder = match PrometheusBuilder::new()
        .set_buckets_for_metric(
            Matcher::Full("trainbot_brightness_avg".to_owned()),
            &buckets,
        )
        .and_then(|b| {
            b.set_buckets_for_metric(
                Matcher::Full("trainbot_brightness_avgdev".to_owned()),
                &buckets,
            )
        })
        .map(|b| b.build_recorder())
    {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("metrics: failed to build recorder: {e}");
            return;
        }
    };

    let handle = recorder.handle();
    if metrics::set_global_recorder(recorder).is_err() {
        return; // already installed
    }

    let addr = addr.to_owned();
    thread::spawn(move || {
        let listener = match TcpListener::bind(&addr) {
            Ok(l) => l,
            Err(e) => {
                tracing::error!("metrics: cannot bind to {addr}: {e}");
                return;
            }
        };
        tracing::info!("metrics listening on {addr}");

        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut buf = [0u8; 4096];
            let _ = stream.read(&mut buf);

            let body = handle.render();
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/plain; version=0.0.4; charset=utf-8\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(response.as_bytes());
        }
    });
}

pub struct FrameDispositionGuard {
    pub disposition: &'static str,
}
impl FrameDispositionGuard {
    pub(crate) fn new() -> Self {
        Self {
            disposition: "unknown",
        }
    }
}
impl Drop for FrameDispositionGuard {
    fn drop(&mut self) {
        record_frame_disposition(self.disposition);
    }
}
fn record_frame_disposition(disposition: &'static str) {
    metrics::counter!("trainbot_frame_dispositions_total", "disposition" => disposition)
        .increment(1);
}

pub fn record_sequence_length(n: usize) {
    metrics::gauge!("trainbot_sequence_length").set(n as f64);
}

pub fn record_source_queue_length(n: usize) {
    metrics::gauge!("trainbot_source_queue_length").set(n as f64);
}

pub fn record_fit_and_stitch_result(result: &'static str) {
    metrics::counter!("trainbot_fit_and_stitch_results_total", "result" => result).increment(1);
}

pub fn record_brightness(avg: f64, avg_dev: f64) {
    metrics::histogram!("trainbot_brightness_avg").record(avg);
    metrics::histogram!("trainbot_brightness_avgdev").record(avg_dev);
}

/// Compute 20 exponentially-spaced bucket boundaries matching Go's
/// `prometheus.ExponentialBucketsRange(0.0005, 1.0, 20)`.
fn brightness_buckets() -> Vec<f64> {
    let (start, end, n) = (0.0005_f64, 1.0_f64, 20usize);
    let factor = (end / start).powf(1.0 / (n - 1) as f64);
    (0..n).map(|i| start * factor.powi(i as i32)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brightness_buckets_range() {
        let b = brightness_buckets();
        assert_eq!(b.len(), 20);
        assert!((b[0] - 0.0005).abs() < 1e-10);
        assert!((b[19] - 1.0).abs() < 1e-6);
        // monotonically increasing
        for w in b.windows(2) {
            assert!(w[1] > w[0]);
        }
    }
}
