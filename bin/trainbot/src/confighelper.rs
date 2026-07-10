use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::Duration;

use anyhow::Context;
use clap::Parser;
use image::DynamicImage;
use trainbot_core::{LogConfig, init_logging};
use vid::{FourCC, FrameSource};

use crate::args::ConfighelperArgs;

const MJPEG_BOUNDARY: &str = "frame";
const FRAME_QUALITY: u8 = 80;

static INDEX_HTML: &str = include_str!("../../../internal/pkg/server/wwwdata/index.html");

struct FrameState {
    jpeg: Option<Vec<u8>>,
    seq: u64,
}

type Shared = Arc<(Mutex<FrameState>, Condvar)>;

pub fn run(argv: Vec<String>) {
    let args = ConfighelperArgs::parse_from(argv);

    init_logging(&LogConfig {
        log_pretty: args.log_pretty,
        log_level: args.log_level.clone(),
    });

    if args.probe_only {
        probe_cameras();
        return;
    }

    let mut src = open_source(&args).unwrap_or_else(|e| {
        eprintln!("error: failed to open video source: {e:#}");
        std::process::exit(1);
    });

    // Throttle: ~5 fps in the shared stream regardless of source fps
    let source_fps = src.fps().max(1.0);
    let every_nth = (source_fps / 5.0).max(1.0) as usize;

    let shared: Shared = Arc::new((
        Mutex::new(FrameState { jpeg: None, seq: 0 }),
        Condvar::new(),
    ));

    // Start HTTP server thread
    {
        let shared = Arc::clone(&shared);
        let listen_addr = args.listen_addr.clone();
        thread::spawn(move || {
            let listener = TcpListener::bind(&listen_addr)
                .unwrap_or_else(|e| panic!("cannot bind {listen_addr}: {e}"));
            tracing::info!(url = %format!("http://{listen_addr}"), "confighelper listening");
            for stream in listener.incoming().flatten() {
                let shared = Arc::clone(&shared);
                thread::spawn(move || handle_connection(stream, shared));
            }
        });
    }

    tracing::info!(input = %args.input, "capturing frames");

    let mut frame_idx: usize = 0;
    loop {
        match src.next_frame() {
            Ok(Some(frame)) => {
                if frame_idx.is_multiple_of(every_nth)
                    && let Ok(jpeg) = encode_jpeg(&frame.image)
                {
                    let (lock, cvar) = &*shared;
                    let mut state = lock.lock().unwrap();
                    state.jpeg = Some(jpeg);
                    state.seq += 1;
                    cvar.notify_all();
                }
                frame_idx += 1;
            }
            Ok(None) => {
                tracing::info!("no more frames");
                break;
            }
            Err(e) => {
                tracing::warn!(err = %e, "frame error");
                thread::sleep(Duration::from_millis(100));
            }
        }
    }
}

fn open_source(args: &ConfighelperArgs) -> anyhow::Result<Box<dyn FrameSource>> {
    if args.input == "picam3" {
        return Ok(Box::new(
            vid::PiCam3Src::open(vid::PiCam3Config {
                roi_x: 0,
                roi_y: 0,
                width: 0,
                height: 0,
                focus: 0.0,
                rotate_180: args.rotate_180,
                format: FourCC::MJPG,
                fps: 5,
            })
            .context("open PiCam3Src")?,
        ));
    }

    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::FileTypeExt;
        if let Ok(meta) = std::fs::metadata(&args.input)
            && meta.file_type().is_char_device()
        {
            return Ok(Box::new(
                vid::CamSrc::open(vid::CamConfig {
                    device: args.input.clone(),
                    fourcc: FourCC::MJPG,
                    width: args.camera_w,
                    height: args.camera_h,
                })
                .context("open CamSrc")?,
            ));
        }
    }

    let path = Utf8PathBuf::from_str(&args.input).expect("--input path must be UTF-8");
    Ok(Box::new(
        vid::FileSrc::open(path.as_path()).context("open FileSrc")?,
    ))
}

fn probe_cameras() {
    #[cfg(target_os = "linux")]
    {
        match vid::detect_cams() {
            Ok(cams) if cams.is_empty() => eprintln!("no cameras detected"),
            Ok(cams) => {
                for cam in cams {
                    println!(
                        "--input {} --camera-format-fourcc {} --camera-w {} --camera-h {}",
                        cam.device, cam.fourcc, cam.width, cam.height
                    );
                }
            }
            Err(e) => {
                eprintln!("probe failed: {e}");
                std::process::exit(1);
            }
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        eprintln!("camera detection is only supported on Linux");
    }
}

fn encode_jpeg(img: &image::RgbaImage) -> Result<Vec<u8>, image::ImageError> {
    use image::codecs::jpeg::JpegEncoder;
    let rgb = DynamicImage::ImageRgba8(img.clone()).into_rgb8();
    let mut buf = Vec::new();
    JpegEncoder::new_with_quality(&mut buf, FRAME_QUALITY)
        .encode_image(&DynamicImage::ImageRgb8(rgb))?;
    Ok(buf)
}

fn read_request_path(stream: &mut TcpStream) -> Option<String> {
    let mut buf = [0u8; 4096];
    let n = stream.read(&mut buf).ok()?;
    let text = std::str::from_utf8(&buf[..n]).ok()?;
    let first_line = text.lines().next()?;
    let path = first_line.split_whitespace().nth(1)?;
    // Strip query string
    Some(path.split('?').next().unwrap_or(path).to_owned())
}

fn handle_connection(mut stream: TcpStream, shared: Shared) {
    let path = match read_request_path(&mut stream) {
        Some(p) => p,
        None => return,
    };

    match path.as_str() {
        "/stream.mjpeg" => serve_mjpeg(stream, shared),
        "/stream.jpeg" => serve_snapshot(stream, shared),
        "/cameras" => serve_cameras(stream),
        _ => serve_html(stream),
    }
}

fn serve_mjpeg(mut stream: TcpStream, shared: Shared) {
    let header = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: multipart/x-mixed-replace; boundary={MJPEG_BOUNDARY}\r\nCache-Control: no-cache\r\n\r\n"
    );
    if stream.write_all(header.as_bytes()).is_err() {
        return;
    }

    let mut last_seq: u64 = 0;
    loop {
        let jpeg = {
            let (lock, cvar) = &*shared;
            let guard = lock.lock().unwrap();
            let guard = cvar.wait_while(guard, |s| s.seq == last_seq).unwrap();
            last_seq = guard.seq;
            guard.jpeg.clone()
        };

        let Some(jpeg) = jpeg else { continue };

        let part = format!(
            "--{MJPEG_BOUNDARY}\r\nContent-Type: image/jpeg\r\nContent-Length: {}\r\n\r\n",
            jpeg.len()
        );
        if stream.write_all(part.as_bytes()).is_err() {
            break;
        }
        if stream.write_all(&jpeg).is_err() {
            break;
        }
        if stream.write_all(b"\r\n").is_err() {
            break;
        }
    }
}

fn serve_snapshot(mut stream: TcpStream, shared: Shared) {
    let jpeg = {
        let (lock, cvar) = &*shared;
        let guard = lock.lock().unwrap();
        // Wait for at least one frame
        let guard = cvar.wait_while(guard, |s| s.jpeg.is_none()).unwrap();
        guard.jpeg.clone()
    };

    let Some(jpeg) = jpeg else {
        let _ = stream.write_all(b"HTTP/1.1 503 Service Unavailable\r\n\r\n");
        return;
    };

    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: image/jpeg\r\nContent-Length: {}\r\nCache-Control: no-cache\r\n\r\n",
        jpeg.len()
    );
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.write_all(&jpeg);
}

fn serve_cameras(mut stream: TcpStream) {
    let body = cameras_json();
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
        body.len(),
        body
    );
    let _ = stream.write_all(response.as_bytes());
}

fn cameras_json() -> String {
    #[cfg(target_os = "linux")]
    {
        let cams = match vid::detect_cams() {
            Ok(c) => c,
            Err(_) => return "[]".to_owned(),
        };
        let entries: Vec<String> = cams
            .iter()
            .map(|c| {
                format!(
                    r#"{{"DeviceFile":{},"Format":{},"FrameSize":{{"X":{},"Y":{}}}}}"#,
                    serde_json::to_string(&c.device).unwrap(),
                    serde_json::to_string(&c.fourcc.to_string()).unwrap(),
                    c.width,
                    c.height,
                )
            })
            .collect();
        format!("[{}]", entries.join(","))
    }
    #[cfg(not(target_os = "linux"))]
    {
        "[]".to_owned()
    }
}

fn serve_html(mut stream: TcpStream) {
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\n\r\n{}",
        INDEX_HTML.len(),
        INDEX_HTML
    );
    let _ = stream.write_all(response.as_bytes());
}
