//! JPEG frame scanner for continuous MJPEG byte streams (e.g. rpicam-vid stdout).
//!
//! Ported from Go's `pkg/vid/jpegscan.go`.  Supports an internal 2-byte
//! "unget" buffer so `scan_image_data` can peek at marker bytes and hand back
//! control to the outer segment loop without consuming them.

use crate::{Error, Result};
use std::io::{BufReader, Read};

const RST0: u8 = 0xD0;
const RST7: u8 = 0xD7;
const SOI: u8 = 0xD8;
const EOI: u8 = 0xD9;
const SOS: u8 = 0xDA;

struct Inner<R: Read> {
    reader: BufReader<R>,
    // up to 2 bytes pushed back by scan_image_data
    unget: [u8; 2],
    unget_len: usize,
}

impl<R: Read> Inner<R> {
    fn new(r: R) -> Self {
        Self {
            reader: BufReader::new(r),
            unget: [0; 2],
            unget_len: 0,
        }
    }

    fn read_byte(&mut self) -> std::io::Result<Option<u8>> {
        if self.unget_len > 0 {
            self.unget_len -= 1;
            return Ok(Some(self.unget[self.unget_len]));
        }
        let mut b = [0u8; 1];
        loop {
            match self.reader.read(&mut b) {
                Ok(0) => return Ok(None),
                Ok(_) => return Ok(Some(b[0])),
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e),
            }
        }
    }

    // Push two bytes back; b1 will be read next, b2 after that.
    fn unread2(&mut self, b1: u8, b2: u8) {
        self.unget[0] = b2;
        self.unget[1] = b1;
        self.unget_len = 2;
    }
}

pub struct JpegScanner<R: Read> {
    inner: Inner<R>,
}

impl<R: Read> JpegScanner<R> {
    pub fn new(r: R) -> Self {
        Self {
            inner: Inner::new(r),
        }
    }

    /// Read the next JPEG image from the stream.
    /// Returns `Ok(None)` on clean EOF before the SOI marker.
    pub fn scan(&mut self) -> Result<Option<Vec<u8>>> {
        let mut buf = Vec::new();

        // SOI: 0xFF 0xD8
        match self.read_n(&mut buf, 2)? {
            0 => return Ok(None),
            2 => {}
            _ => return Err(Error::Format("partial SOI at EOF".into())),
        }
        if buf[0] != 0xFF || buf[1] != SOI {
            return Err(Error::Format(format!(
                "invalid SOI: {:02x} {:02x}",
                buf[0], buf[1]
            )));
        }

        loop {
            // Segment marker: 0xFF <type>
            if self.read_n(&mut buf, 2)? != 2 {
                return Err(Error::Format("EOF reading segment marker".into()));
            }
            let pos = buf.len();
            if buf[pos - 2] != 0xFF {
                return Err(Error::Format(format!(
                    "expected 0xFF marker, got {:02x}",
                    buf[pos - 2]
                )));
            }
            let marker_type = buf[pos - 1];

            if marker_type == EOI {
                break;
            }
            if (RST0..=RST7).contains(&marker_type) {
                return Err(Error::Format("RST marker at invalid position".into()));
            }

            // Segment length (big-endian u16, includes the 2-byte length field itself)
            if self.read_n(&mut buf, 2)? != 2 {
                return Err(Error::Format("EOF reading segment length".into()));
            }
            let pos = buf.len();
            let seg_len = (buf[pos - 2] as usize) << 8 | buf[pos - 1] as usize;
            if seg_len < 2 {
                return Err(Error::Format(format!("invalid segment length {seg_len}")));
            }
            // Data bytes after the length field
            let data_len = seg_len - 2;
            if data_len > 0 && self.read_n(&mut buf, data_len)? != data_len {
                return Err(Error::Format("EOF reading segment data".into()));
            }

            if marker_type == SOS {
                self.scan_image_data(&mut buf)?;
            }
        }

        Ok(Some(buf))
    }

    /// Read exactly `n` bytes into `buf`. Returns how many bytes were actually
    /// read (may be < n only if EOF was hit on the first byte).
    fn read_n(&mut self, buf: &mut Vec<u8>, n: usize) -> Result<usize> {
        let start = buf.len();
        for i in 0..n {
            match self.inner.read_byte().map_err(Error::Io)? {
                None if i == 0 => {
                    return Ok(0); // clean EOF before anything was read
                }
                None => {
                    return Err(Error::Format(format!("EOF after {i} of {n} bytes")));
                }
                Some(b) => buf.push(b),
            }
        }
        Ok(buf.len() - start)
    }

    /// Consume image-data bytes (after SOS) until a new segment marker is
    /// found.  The marker bytes are pushed back so the outer loop can read them.
    fn scan_image_data(&mut self, buf: &mut Vec<u8>) -> Result<()> {
        loop {
            let b1 = match self.inner.read_byte().map_err(Error::Io)? {
                None => return Ok(()),
                Some(b) => b,
            };

            if b1 != 0xFF {
                buf.push(b1);
                continue;
            }

            // b1 == 0xFF — need to look at the next byte
            let b2 = match self.inner.read_byte().map_err(Error::Io)? {
                None => {
                    buf.push(b1);
                    return Ok(());
                }
                Some(b) => b,
            };

            match b2 {
                0x00 => {
                    // stuffed zero — literal 0xFF in image data
                    buf.push(0xFF);
                    buf.push(0x00);
                }
                RST0..=RST7 => {
                    buf.push(0xFF);
                    buf.push(b2);
                }
                _ => {
                    // A new segment marker — put both bytes back for outer loop
                    self.inner.unread2(b1, b2);
                    return Ok(());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn load_frame_jpg() -> Vec<u8> {
        // Use the same test image the Go tests use
        std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../pkg/vid/testdata/frame.jpg"
        ))
        .expect("frame.jpg test fixture missing")
    }

    #[test]
    fn scan_single_jpeg() {
        let data = load_frame_jpg();
        let mut scanner = JpegScanner::new(Cursor::new(data.clone()));
        let out = scanner.scan().unwrap().expect("should return Some");
        assert_eq!(out.len(), data.len());

        // Verify it decodes as a valid JPEG
        image::load_from_memory_with_format(&out, image::ImageFormat::Jpeg).unwrap();
    }

    #[test]
    fn scan_multiple_jpegs() {
        let frame = load_frame_jpg();
        let stream: Vec<u8> = frame
            .iter()
            .cloned()
            .cycle()
            .take(frame.len() * 5)
            .collect();
        let mut scanner = JpegScanner::new(Cursor::new(stream));
        for _ in 0..5 {
            let out = scanner.scan().unwrap().expect("expected a frame");
            assert_eq!(out.len(), frame.len());
        }
        // Clean EOF
        assert!(scanner.scan().unwrap().is_none());
    }
}
