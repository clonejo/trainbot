//! USB camera frame source via Video4Linux 2 (raw ioctls only; no libv4l linkage).

use crate::{Error, FourCC, Frame, FrameSource, Result, convert};
use image::RgbaImage;
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::thread;
use std::time::SystemTime;
use v4l::io::traits::CaptureStream;
use v4l::video::Capture;

const SKIP_INITIAL_FRAMES: usize = 5;
const STREAM_BUFFERS: u32 = 4;

#[derive(Debug, Clone)]
pub struct CamConfig {
    pub device: String,
    pub fourcc: FourCC,
    pub width: u32,
    pub height: u32,
}

pub struct CamSrc {
    fps: f64,
    rx: Receiver<Result<RgbaImage>>,
    _thread: thread::JoinHandle<()>,
}

impl CamSrc {
    pub fn open(cfg: CamConfig) -> Result<Self> {
        let dev = v4l::Device::with_path(&cfg.device).map_err(Error::Io)?;

        let fmt = v4l::Format::new(
            cfg.width,
            cfg.height,
            v4l::FourCC::new(&cfg.fourcc.to_le_bytes()),
        );
        dev.set_format(&fmt).map_err(Error::Io)?;

        let actual = dev.format().map_err(Error::Io)?;
        if actual.width != cfg.width || actual.height != cfg.height {
            return Err(Error::Format(format!(
                "camera returned {}×{}, requested {}×{}",
                actual.width, actual.height, cfg.width, cfg.height
            )));
        }

        let fps = {
            let params = dev.params().map_err(Error::Io)?;
            let iv = params.interval;
            if iv.numerator == 0 {
                25.0 // fallback
            } else {
                iv.denominator as f64 / iv.numerator as f64
            }
        };

        let (tx, rx): (SyncSender<Result<RgbaImage>>, _) = mpsc::sync_channel(4);
        let thread = thread::spawn(move || capture_thread(dev, cfg, tx));

        Ok(Self {
            fps,
            rx,
            _thread: thread,
        })
    }
}

fn capture_thread(
    dev: v4l::Device,
    cfg: CamConfig,
    tx: SyncSender<Result<RgbaImage>>,
) {
    let mut stream =
        match v4l::io::mmap::Stream::with_buffers(&dev, v4l::buffer::Type::VideoCapture, STREAM_BUFFERS) {
            Ok(s) => s,
            Err(e) => {
                let _ = tx.send(Err(Error::Io(e)));
                return;
            }
        };

    // Discard initial frames (some cameras return garbage frames at startup)
    for _ in 0..SKIP_INITIAL_FRAMES {
        if stream.next().is_err() {
            return;
        }
    }

    loop {
        let (buf, _meta) = match stream.next() {
            Ok(r) => r,
            Err(e) => {
                let _ = tx.send(Err(Error::Io(e)));
                return;
            }
        };

        let image = match cfg.fourcc {
            FourCC::MJPG => {
                match image::load_from_memory_with_format(buf, image::ImageFormat::Jpeg) {
                    Ok(img) => img.into_rgba8(),
                    Err(e) => {
                        let _ = tx.send(Err(Error::Image(e)));
                        return;
                    }
                }
            }
            FourCC::YUYV => convert::yuyv_to_rgba(buf, cfg.width, cfg.height),
            other => {
                let _ = tx.send(Err(Error::Format(format!("unsupported camera format {other}"))));
                return;
            }
        };

        if tx.send(Ok(image)).is_err() {
            return; // receiver dropped
        }
    }
}

impl FrameSource for CamSrc {
    fn next_frame(&mut self) -> Result<Option<Frame>> {
        let image = self.rx.recv().map_err(|_| Error::Process("camera thread exited".into()))??;
        Ok(Some(Frame {
            image,
            ts: SystemTime::now(),
        }))
    }

    fn fps(&self) -> f64 {
        self.fps
    }
    fn is_live(&self) -> bool {
        true
    }
}

/// Probe all `/dev/video*` devices and return their supported configurations.
/// Devices that are inaccessible or report no formats are silently skipped.
pub fn detect_cams() -> Result<Vec<CamConfig>> {
    let mut results = Vec::new();

    let mut entries: Vec<_> = std::fs::read_dir("/dev")
        .map_err(Error::Io)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map_or(false, |n| n.starts_with("video"))
        })
        .collect();
    entries.sort();

    for path in entries {
        let path_str = match path.to_str() {
            Some(s) => s.to_string(),
            None => continue,
        };
        let Ok(dev) = v4l::Device::with_path(&path_str) else {
            continue;
        };
        let Ok(formats) = dev.enum_formats() else {
            continue;
        };
        for fmt_desc in formats {
            let fcc_bytes = fmt_desc.fourcc.repr;
            let fcc = FourCC::from_le_bytes(&fcc_bytes);
            let Ok(sizes) = dev.enum_framesizes(fmt_desc.fourcc) else {
                continue;
            };
            for sz in sizes {
                use v4l::framesize::FrameSizeEnum;
                let (w, h) = match sz.size {
                    FrameSizeEnum::Discrete(d) => (d.width, d.height),
                    FrameSizeEnum::Stepwise(s) => (s.max_width, s.max_height),
                };
                results.push(CamConfig {
                    device: path_str.clone(),
                    fourcc: fcc,
                    width: w,
                    height: h,
                });
            }
        }
    }

    Ok(results)
}
