mod auto;
mod config;
mod fit;
mod gif;
pub mod log;
pub mod metrics;
mod sequence;
mod stitch;
mod train;

pub use auto::AutoStitcher;
pub use config::Config;
pub use fit::FitMethod;
pub use log::{init_logging, LogConfig};
pub use metrics::init_metrics;
pub use train::Train;

use sequence::Sequence;

/// Run the full detection pipeline on a `FrameSource`.
///
/// Optionally crops each frame to `(x, y, w, h)` before processing.
/// Returns all detected trains in order of detection.
pub fn run_pipeline(
    src: &mut dyn vid::FrameSource,
    config: Config,
    crop: Option<(u32, u32, u32, u32)>,
    fit_method: FitMethod,
) -> Vec<Train> {
    let mut stitcher = AutoStitcher::new(config, fit_method);
    let mut trains = Vec::new();

    loop {
        match src.next_frame() {
            Ok(Some(frame)) => {
                let img = if let Some((x, y, w, h)) = crop {
                    image::imageops::crop_imm(&frame.image, x, y, w, h).to_image()
                } else {
                    frame.image
                };
                if let Some(train) = stitcher.frame(img, frame.ts) {
                    trains.push(train);
                }
            }
            Ok(None) => break,
            Err(e) => {
                tracing::warn!("pipeline frame source error: {e}");
                break;
            }
        }
    }

    if let Some(t) = stitcher.try_stitch_and_reset() {
        trains.push(t);
    }

    trains
}
