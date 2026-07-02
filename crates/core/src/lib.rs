mod auto;
mod config;
mod fit;
mod gif;
pub mod log;
pub mod metrics;
mod sequence;
mod stitch;
mod train;
mod video;

pub use auto::{AutoStitcher, AutoStitcherError};
pub use config::Config;
pub use fit::FitMethod;
pub use log::{init_logging, LogConfig};
pub use metrics::init_metrics;
pub use train::Train;
pub use video::{Encoder as VideoEncoder, EXTENSION as VIDEO_EXTENSION};

use sequence::Sequence;
