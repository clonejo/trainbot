mod auto;
mod config;
mod fit;
pub mod log;
pub mod metrics;
mod sequence;
mod stitch;
mod train;
mod video;

pub use auto::{AutoStitcher, AutoStitcherError};
pub use config::Config;
pub use fit::FitMethod;
pub use log::{LogConfig, init_logging};
pub use metrics::init_metrics;
pub use train::Train;
pub use video::{EXTENSION as VIDEO_EXTENSION, Encoder as VideoEncoder};

use sequence::Sequence;
