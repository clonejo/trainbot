mod convert;
mod fcc;
mod ffprobe;
mod file;
mod jpeg_scan;
mod mjpeg_http;
mod picam3;

#[cfg(target_os = "linux")]
mod cam;

pub use fcc::FourCC;
pub use file::FileSrc;
pub use jpeg_scan::JpegScanner;
pub use mjpeg_http::MjpegHttpSrc;
pub use picam3::{PiCam3Config, PiCam3Src};

#[cfg(target_os = "linux")]
pub use cam::{CamConfig, CamSrc, detect_cams};

use image::RgbaImage;
use std::time::SystemTime;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("probe failed: {0}")]
    Probe(String),
    #[error("image error: {0}")]
    Image(#[from] image::ImageError),
    #[error("no video stream found")]
    NoVideoStream,
    #[error("multiple video streams found")]
    MultipleVideoStreams,
    #[error("invalid fps string: {0}")]
    InvalidFps(String),
    #[error("process error: {0}")]
    Process(String),
    #[error("format error: {0}")]
    Format(String),
    #[error("http: {0}")]
    Http(#[from] mjpeg_http::HttpError),
}

pub type Result<T> = std::result::Result<T, Error>;

pub struct Frame {
    pub image: RgbaImage,
    pub ts: SystemTime,
}

pub trait FrameSource {
    fn next_frame(&mut self) -> Result<Option<Frame>>;
    fn fps(&self) -> f64;
    fn is_live(&self) -> bool;
}
