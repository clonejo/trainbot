use crate::{Error, FourCC, Frame, FrameSource, Result, convert, jpeg_scan::JpegScanner};
use duct::ReaderHandle;
use image::RgbaImage;
use std::io::{self, BufReader, Read};
use std::time::SystemTime;

//  pi@raspberrypi:~ $ rpicam-hello --list
//  Available cameras
//  -----------------
//  0 : imx708 [4608x2592] (/base/soc/i2c0mux/i2c@1/imx708@1a)
//      Modes: 'SRGGB10_CSI2P' : 1536x864 [120.13 fps - (768, 432)/3072x1728 crop]
//                               2304x1296 [56.03 fps - (0, 0)/4608x2592 crop]
//                               4608x2592 [14.35 fps - (0, 0)/4608x2592 crop]

//  Raspberry Pi HQ camera:
//
//  0 : imx477 [4056x3040 12-bit RGGB] (/base/soc/i2c0mux/i2c@1/imx477@1a)
//      Modes: 'SRGGB10_CSI2P' : 1332x990 [120.05 fps - (696, 528)/2664x1980 crop]
//             'SRGGB12_CSI2P' : 2028x1080 [50.03 fps - (0, 440)/4056x2160 crop]
//                               2028x1520 [40.01 fps - (0, 0)/4056x3040 crop]
//                               4056x3040 [10.00 fps - (0, 0)/4056x3040 crop]

/// Hardcoded sensor dimensions for the Raspberry Pi Camera Module v3.
//const SENSOR_W: u32 = 2304;
//const SENSOR_H: u32 = 1296;
// TODO: make configurable / support picam3 + HQ camera
const SENSOR_W: u32 = 1014;
const SENSOR_H: u32 = 740;

pub struct PiCam3Config {
    /// ROI within the sensor (defaults to full sensor if all-zero). (ROI = region of interest)
    pub roi_x: u32,
    pub roi_y: u32,
    pub width: u32,
    pub height: u32,
    /// Constant lens focus: 0 = infinity, 2 ≈ 0.5 m.
    pub focus: f64,
    pub rotate_180: bool,
    pub format: FourCC,
    pub fps: u32,
}

enum Inner {
    Mjpeg(JpegScanner<ReaderHandle>),
    Yuv420 {
        stdout: BufReader<ReaderHandle>,
        buf: Vec<u8>,
    },
}

pub struct PiCam3Src {
    w: u32,
    h: u32,
    fps: u32,
    inner: Inner,
}

impl PiCam3Src {
    pub fn open(cfg: PiCam3Config) -> Result<Self> {
        let (roi_x, roi_y, w, h) = if cfg.width == 0 && cfg.height == 0 {
            (0, 0, SENSOR_W, SENSOR_H)
        } else {
            (cfg.roi_x, cfg.roi_y, cfg.width, cfg.height)
        };

        let sx = SENSOR_W as f64;
        let sy = SENSOR_H as f64;
        let roi = format!(
            "{:.6},{:.6},{:.6},{:.6}",
            roi_x as f64 / sx,
            roi_y as f64 / sy,
            w as f64 / sx,
            h as f64 / sy,
        );

        let mut args: Vec<String> = vec![
            "--verbose=1".into(),
            "--timeout=0".into(),
            "--inline".into(),
            "--nopreview".into(),
            format!("--width={w}"),
            format!("--height={h}"),
            format!("--roi={roi}"),
            //format!("--mode={}:{}:12:P", SENSOR_W, SENSOR_H),
            "--mode=2028:1520:12:P".into(), // FIXME: make mode configurable
            format!("--framerate={}", cfg.fps),
            "--autofocus-mode=manual".into(),
            format!("--lens-position={:.6}", cfg.focus),
            "--output".into(),
            "-".into(),
        ];

        if cfg.rotate_180 {
            args.push("--rotation=180".into());
        }

        match cfg.format {
            FourCC::MJPG => {
                args.push("--codec=mjpeg".into());
                args.push("--quality=90".into());
            }
            FourCC::YU12 => {
                args.push("--codec=yuv420".into());
            }
            other => {
                return Err(Error::Format(format!(
                    "rpicam-vid: unsupported format {other}"
                )));
            }
        }

        let reader = duct::cmd("rpicam-vid", &args)
            .reader()
            .map_err(Error::ProcessSpawn)?;

        let inner = if cfg.format == FourCC::MJPG {
            Inner::Mjpeg(JpegScanner::new(reader))
        } else {
            let buf_size = (w * h * 12 / 8) as usize; // YU12: w*h*1.5 bytes
            Inner::Yuv420 {
                stdout: BufReader::new(reader),
                buf: vec![0u8; buf_size],
            }
        };

        Ok(Self {
            w,
            h,
            fps: cfg.fps,
            inner,
        })
    }
}

impl FrameSource for PiCam3Src {
    fn next_frame(&mut self) -> Result<Option<Frame>> {
        let ts = SystemTime::now();
        let w = self.w;
        let h = self.h;

        let image: RgbaImage = match &mut self.inner {
            Inner::Mjpeg(scanner) => {
                let data = scanner
                    .scan()?
                    .ok_or_else(|| Error::Process("MJPEG stream ended".into()))?;
                image::load_from_memory_with_format(&data, image::ImageFormat::Jpeg)
                    .map_err(Error::Image)?
                    .into_rgba8()
            }
            Inner::Yuv420 { stdout, buf } => {
                read_exact(stdout, buf).map_err(Error::Io)?;
                convert::yuv420_to_rgba(buf, w, h)
            }
        };

        Ok(Some(Frame { image, ts }))
    }

    fn fps(&self) -> f64 {
        self.fps as f64
    }
    fn is_live(&self) -> bool {
        true
    }
}

fn read_exact(r: &mut impl Read, buf: &mut [u8]) -> io::Result<()> {
    let mut pos = 0;
    while pos < buf.len() {
        match r.read(&mut buf[pos..]) {
            Ok(0) => {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "EOF mid-frame",
                ));
            }
            Ok(n) => pos += n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }
    Ok(())
}
