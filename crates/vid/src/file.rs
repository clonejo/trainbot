use crate::{Error, Frame, FrameSource, Result, ffprobe};
use image::RgbaImage;
use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub struct FileSrc {
    child: Child,
    width: u32,
    height: u32,
    fps: f64,
    start_time: SystemTime,
    frame_count: u64,
    buf: Vec<u8>,
}

impl FileSrc {
    pub fn open(path: &str) -> Result<Self> {
        let info = ffprobe::probe(path)?;
        let frame_bytes = (info.width * info.height * 4) as usize;

        let child = Command::new("ffmpeg")
            .args(["-i", path, "-f", "rawvideo", "-pix_fmt", "rgba", "pipe:1"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| Error::Process(format!("failed to spawn ffmpeg: {e}")))?;

        Ok(Self {
            child,
            width: info.width,
            height: info.height,
            fps: info.fps,
            start_time: info.start_time.unwrap_or(UNIX_EPOCH),
            frame_count: 0,
            buf: vec![0u8; frame_bytes],
        })
    }

    pub fn width(&self) -> u32 {
        self.width
    }
    pub fn height(&self) -> u32 {
        self.height
    }
}

impl FrameSource for FileSrc {
    fn next_frame(&mut self) -> Result<Option<Frame>> {
        let stdout = self
            .child
            .stdout
            .as_mut()
            .ok_or_else(|| Error::Process("no stdout pipe".into()))?;

        let n = read_exact_or_eof(stdout, &mut self.buf).map_err(Error::Io)?;
        if n == 0 {
            return Ok(None); // clean EOF
        }
        if n != self.buf.len() {
            return Err(Error::Process(format!(
                "partial frame: {n} of {} bytes",
                self.buf.len()
            )));
        }

        let ts = self.start_time + Duration::from_secs_f64(self.frame_count as f64 / self.fps);
        self.frame_count += 1;

        let img = RgbaImage::from_raw(self.width, self.height, self.buf.clone())
            .ok_or_else(|| Error::Process("frame buffer size mismatch".into()))?;

        Ok(Some(Frame { image: img, ts }))
    }

    fn fps(&self) -> f64 {
        self.fps
    }
    fn is_live(&self) -> bool {
        false
    }
}

impl Drop for FileSrc {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Read until `buf` is full, returning the number of bytes read.
/// Returns 0 only if the very first `read()` returns 0 (clean EOF).
fn read_exact_or_eof(r: &mut impl Read, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut total = 0;
    while total < buf.len() {
        match r.read(&mut buf[total..]) {
            Ok(0) => break,
            Ok(n) => total += n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set0_path(name: &str) -> String {
        format!(
            "{}/../../internal/pkg/stitch/testdata/set0/{name}",
            env!("CARGO_MANIFEST_DIR")
        )
    }

    fn ffprobe_available() -> bool {
        std::process::Command::new("ffprobe")
            .arg("-version")
            .output()
            .is_ok()
    }

    fn count_frames(name: &str) -> u32 {
        let path = set0_path(name);
        let mut src = FileSrc::open(&path).unwrap_or_else(|e| panic!("open {name}: {e}"));
        assert!(src.width() > 0 && src.height() > 0);
        let mut n = 0u32;
        while src.next_frame().unwrap().is_some() {
            n += 1;
        }
        n
    }

    #[test]
    fn file_src_day_frame_count() {
        if !ffprobe_available() { eprintln!("skip: ffprobe not found"); return; }
        let n = count_frames("day.mp4");
        assert!((85..=87).contains(&n), "expected ~86 frames, got {n}");
    }

    #[test]
    fn file_src_night_frame_count() {
        if !ffprobe_available() { eprintln!("skip: ffprobe not found"); return; }
        let n = count_frames("night.mp4");
        assert!((82..=84).contains(&n), "expected ~83 frames, got {n}");
    }

    #[test]
    fn file_src_rain_frame_count() {
        if !ffprobe_available() { eprintln!("skip: ffprobe not found"); return; }
        let n = count_frames("rain.mp4");
        assert!((81..=83).contains(&n), "expected ~82 frames, got {n}");
    }

    #[test]
    fn file_src_snow_frame_count() {
        if !ffprobe_available() { eprintln!("skip: ffprobe not found"); return; }
        let n = count_frames("snow.mp4");
        assert!((55..=57).contains(&n), "expected ~56 frames, got {n}");
    }
}
