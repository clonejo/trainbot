use crate::{Error, Frame, FrameSource, Result, ffprobe};
use camino::Utf8Path;
use duct::ReaderHandle;
use image::RgbaImage;
use std::io::Read;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub struct FileSrc {
    reader: ReaderHandle,
    width: u32,
    height: u32,
    fps: f64,
    start_ts: Instant,
    start_time_wall: SystemTime,
    frame_pts: Vec<f64>,
    frames_processed: usize,
    buf: Vec<u8>,
}

impl FileSrc {
    pub fn open(path: &Utf8Path) -> Result<Self> {
        let info = ffprobe::probe(path)?;
        let frame_bytes = (info.width * info.height * 4) as usize;

        #[rustfmt::skip]
        let reader = duct::cmd!(
            "ffmpeg",
            "-loglevel", "error",
            "-i", path,
            "-f", "rawvideo",
            "-pix_fmt", "rgba",
            "pipe:1"
        )
        .reader()
        .map_err(Error::ProcessSpawn)?;

        Ok(Self {
            reader,
            width: info.width,
            height: info.height,
            fps: info.fps,
            start_ts: Instant::now(), /* the absolute value does not matter */
            start_time_wall: info.start_time.unwrap_or(UNIX_EPOCH),
            frame_pts: info.frame_pts,
            frames_processed: 0,
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
        let n = read_exact_or_eof(&mut self.reader, &mut self.buf).map_err(Error::Io)?;
        if n == 0 {
            assert_eq!(self.frame_pts.len(), self.frames_processed);
            return Ok(None); // clean EOF
        }
        if n != self.buf.len() {
            return Err(Error::Process(format!(
                "partial frame: {n} of {} bytes",
                self.buf.len()
            )));
        }

        let ts = self.start_ts + Duration::from_secs_f64(self.frame_pts[self.frames_processed]);
        let wall =
            self.start_time_wall + Duration::from_secs_f64(self.frame_pts[self.frames_processed]);
        self.frames_processed += 1;

        let image = RgbaImage::from_raw(self.width, self.height, self.buf.clone())
            .ok_or_else(|| Error::Process("frame buffer size mismatch".into()))?;

        Ok(Some(Frame { image, ts, wall }))
    }

    fn fps(&self) -> f64 {
        self.fps
    }
    fn is_live(&self) -> bool {
        false
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
    use testdata::{assert_ffmpeg_available, testdata_path};

    use super::*;

    fn count_frames(name: &str) -> u32 {
        let path = testdata_path(&format!("set0/{name}"));
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
        assert_ffmpeg_available();
        let n = count_frames("day.mp4");
        assert!((182..=184).contains(&n), "expected 183 frames, got {n}");
    }

    #[test]
    fn file_src_night_frame_count() {
        assert_ffmpeg_available();
        let n = count_frames("night.mp4");
        assert!((206..=208).contains(&n), "expected 207 frames, got {n}");
    }

    #[test]
    fn file_src_rain_frame_count() {
        assert_ffmpeg_available();
        let n = count_frames("rain.mp4");
        assert!((196..=198).contains(&n), "expected 197 frames, got {n}");
    }

    #[test]
    fn file_src_snow_frame_count() {
        assert_ffmpeg_available();
        let n = count_frames("snow.mp4");
        assert!((88..=90).contains(&n), "expected 89 frames, got {n}");
    }
}
