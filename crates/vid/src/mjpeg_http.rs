use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read};
use std::time::{Duration, SystemTime};

use image::ImageFormat;

use crate::{Frame, FrameSource};

#[derive(Debug, thiserror::Error)]
pub enum HttpError {
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),
    #[error("HTTP {0}")]
    Status(reqwest::StatusCode),
    #[error("no multipart boundary in Content-Type: {0:?}")]
    NoBoundary(String),
    #[error("stream ended unexpectedly")]
    StreamEnded,
    #[error("no Content-Length in MJPEG part")]
    NoContentLength,
}

type Result<T> = std::result::Result<T, crate::Error>;

const FPS_WINDOW: usize = 60;
const DEFAULT_FPS: f64 = 30.0;

pub struct MjpegHttpSrc {
    reader: BufReader<reqwest::blocking::Response>,
    /// "--myboundary" (with leading "--" already prepended)
    boundary: String,
    frame_times: VecDeque<SystemTime>,
}

impl MjpegHttpSrc {
    pub fn open(url: &str) -> Result<Self> {
        let client = reqwest::blocking::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .build()
            .map_err(HttpError::from)?;

        let response = client.get(url).send().map_err(HttpError::from)?;

        if !response.status().is_success() {
            return Err(HttpError::Status(response.status()).into());
        }

        let ct = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_owned();

        let boundary = parse_boundary(&ct).ok_or_else(|| HttpError::NoBoundary(ct))?;

        Ok(Self {
            reader: BufReader::new(response),
            boundary,
            frame_times: VecDeque::with_capacity(FPS_WINDOW + 1),
        })
    }
}

fn parse_boundary(content_type: &str) -> Option<String> {
    for part in content_type.split(';') {
        if let Some(val) = part.trim().strip_prefix("boundary=") {
            let val = val.trim().trim_matches('"');
            if !val.is_empty() {
                return Some(format!("--{val}"));
            }
        }
    }
    None
}

/// Advance reader past lines until one matches `boundary`.
/// Returns `true` if a normal part boundary was found, `false` if the
/// end-of-stream marker (`--boundary--`) was found.
fn read_until_boundary(reader: &mut impl BufRead, boundary: &str) -> Result<bool> {
    let end_marker = format!("{boundary}--");
    let mut line = String::new();
    loop {
        line.clear();
        let n = reader.read_line(&mut line).map_err(crate::Error::Io)?;
        if n == 0 {
            return Err(HttpError::StreamEnded.into());
        }
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed == boundary {
            return Ok(true);
        }
        if trimmed == end_marker {
            return Ok(false);
        }
    }
}

/// Read part headers until blank line. Returns Content-Length if present.
fn read_part_headers(reader: &mut impl BufRead) -> Result<Option<usize>> {
    let mut content_length: Option<usize> = None;
    let mut line = String::new();
    loop {
        line.clear();
        let n = reader.read_line(&mut line).map_err(crate::Error::Io)?;
        if n == 0 {
            return Err(HttpError::StreamEnded.into());
        }
        if line.trim().is_empty() {
            break;
        }
        let lower = line.to_ascii_lowercase();
        if let Some(rest) = lower.strip_prefix("content-length:")
            && let Ok(v) = rest.trim().parse::<usize>()
        {
            content_length = Some(v);
        }
    }
    Ok(content_length)
}

fn read_exact(reader: &mut impl Read, len: usize) -> Result<Vec<u8>> {
    let mut buf = vec![0u8; len];
    let mut pos = 0;
    while pos < len {
        match reader.read(&mut buf[pos..]) {
            Ok(0) => return Err(HttpError::StreamEnded.into()),
            Ok(n) => pos += n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(crate::Error::Io(e)),
        }
    }
    Ok(buf)
}

impl FrameSource for MjpegHttpSrc {
    fn next_frame(&mut self) -> Result<Option<Frame>> {
        if !read_until_boundary(&mut self.reader, &self.boundary)? {
            return Ok(None);
        }

        let content_length =
            read_part_headers(&mut self.reader)?.ok_or(HttpError::NoContentLength)?;

        let jpeg_bytes = read_exact(&mut self.reader, content_length)?;

        let image =
            image::load_from_memory_with_format(&jpeg_bytes, ImageFormat::Jpeg)?.into_rgba8();

        let ts = SystemTime::now();
        self.frame_times.push_back(ts);
        if self.frame_times.len() > FPS_WINDOW {
            self.frame_times.pop_front();
        }

        Ok(Some(Frame { image, ts }))
    }

    fn fps(&self) -> f64 {
        if self.frame_times.len() < 2 {
            return DEFAULT_FPS;
        }
        let first = self.frame_times.front().unwrap();
        let last = self.frame_times.back().unwrap();
        match last.duration_since(*first) {
            Ok(d) if d.as_secs_f64() > 0.0 => (self.frame_times.len() - 1) as f64 / d.as_secs_f64(),
            _ => DEFAULT_FPS,
        }
    }

    fn is_live(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_boundary_simple() {
        assert_eq!(
            parse_boundary("multipart/x-mixed-replace; boundary=myboundary"),
            Some("--myboundary".into())
        );
    }

    #[test]
    fn parse_boundary_quoted() {
        assert_eq!(
            parse_boundary("multipart/x-mixed-replace; boundary=\"frame\""),
            Some("--frame".into())
        );
    }

    #[test]
    fn parse_boundary_missing() {
        assert_eq!(parse_boundary("text/plain"), None);
        assert_eq!(parse_boundary("multipart/x-mixed-replace"), None);
    }
}
