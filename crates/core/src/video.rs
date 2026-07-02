use std::ffi::OsString;
use std::io::{pipe, Write};
use std::time::SystemTimeError;

use bytes::{BufMut, Bytes, BytesMut};
use clap::ValueEnum;
use duct::cmd;
use mkv_element::io::blocking_impl::*;
use mkv_element::prelude::*;
use tracing::debug;

use crate::sequence::Sequence;

pub const EXTENSION: &str = "mp4";

/// Encode a video from the sequence frames by using ffmpeg.
///
/// To avoid dynamic library dependencies, we want to pipe to the ffmpeg binary. However, there is
/// no easy way to stream raw frames and PTS (per-frame timestamps for variable framerate) to
/// ffmpeg. That's why we generate an mkv stream to pass to ffmpeg.
pub(crate) fn create_video(seq: &Sequence, encoder: Encoder) -> Result<Vec<u8>, VideoError> {
    let first_ts = *seq.ts.first().ok_or(VideoError::FramesEmpty)?;
    let first_frame = seq.frames.first().ok_or(VideoError::FramesEmpty)?;

    // libsvtav1 is terribly slow on a raspi 4, no chance.
    // h264_v4l2m2m encoded my 400x700 video at 1.5x realtime speed
    //let encoder: String = encoder.into();

    let (reader, mut writer) = pipe()?;
    #[rustfmt::skip]
    let ffmpeg = cmd!(
        "ffmpeg",
        "-loglevel", "repeat+level+warning",
        "-i", "-",
        "-f", "ismv", // MP4 errors out with "muxer does not support non seekable output"
        "-fps_mode", "passthrough",
        "-c:v", encoder.to_osstring(),
        "-b:v", "2048k",
        // TODO: -b:v BITRATE ? constant quality supported by raspi hw encoder?
        "-"
    )
    .stdin_file(reader)
    .stdout_capture()
    .start()?;

    //let mut writer = std::fs::File::create("create_video_debug.mkv")?;

    // Create an EBML header element
    let ebml = Ebml {
        ebml_max_id_length: EbmlMaxIdLength(4),
        ebml_max_size_length: EbmlMaxSizeLength(8),
        doc_type: Some(DocType("matroska".to_string())),
        doc_type_version: Some(DocTypeVersion(4)),
        doc_type_read_version: Some(DocTypeReadVersion(2)),
        ..Default::default()
    };
    ebml.write_to(&mut writer)?;

    const TIMESTAMP_SCALE: TimestampScale = TimestampScale(1000); // microsecond resolution
    const TRACK_NUMBER: u8 = 1;

    // hand-roll unsized segment, so we can steram clusters:
    open_segment(&mut writer)?; //
    let info = Info {
        timestamp_scale: TIMESTAMP_SCALE,
        muxing_app: MuxingApp("mkv-element".to_string()),
        writing_app: WritingApp("trainbot".to_string()),
        duration: None,
        ..Default::default()
    };
    info.write_to(&mut writer)?; // 1_000_000 = ms; use 1_000 for µs PTS
    let tracks: Tracks = Tracks {
        track_entry: vec![TrackEntry {
            track_number: TrackNumber(1),
            track_uid: TrackUid(2342),
            track_type: TrackType(TRACK_NUMBER.into()), // Video
            codec_id: CodecId("V_UNCOMPRESSED".to_string()),
            video: Some(Video {
                pixel_width: PixelWidth(first_frame.width().into()),
                pixel_height: PixelHeight(first_frame.height().into()),
                uncompressed_fourcc: Some(UncompressedFourcc(Bytes::from("RGBA"))),
                ..Default::default()
            }),
            ..Default::default()
        }],
        ..Default::default()
    };
    tracks.write_to(&mut writer)?;

    // mkv-element does not support streamed writing, so we take the lazy way and create a
    // segment for each frame:
    for (frame, ts) in seq.frames.iter().zip(&seq.ts) {
        let raw_pixels = frame.as_raw();
        let mut frame_bytes = BytesMut::with_capacity(4 + raw_pixels.len());
        // mkv-element does not actually support serializing Frames, so we do that on our own:
        simple_block_body(&mut frame_bytes, TRACK_NUMBER, 0, true, raw_pixels)?;

        // FIXME: switch ts from SystemTime to Instant, (and have a separate SystemTime for
        // start of sequence) to avoid errors like this:
        let mkv_timestamp = u64::try_from(
            ts.duration_since(first_ts)?.as_millis(), /* mkv has milliseconds as default */
        )
        .unwrap()
            * TIMESTAMP_SCALE.0;
        let cluster = Cluster {
            timestamp: Timestamp(mkv_timestamp),
            blocks: vec![mkv_element::ClusterBlock::Simple(SimpleBlock(
                frame_bytes.into(),
            ))],
            ..Default::default()
        };
        cluster.write_to(&mut writer)?;
    }

    // close ffmpeg's stdin:
    drop(writer);

    let encoded = ffmpeg.into_output()?.stdout;
    debug!(bytes = encoded.len(), "got encoded bytes from ffmpeg");
    Ok(encoded)
}

#[derive(ValueEnum, Clone, Copy, Debug, Default)]
pub enum Encoder {
    #[value(name = "libx264")]
    #[default]
    Libx264,
    #[value(name = "h264_v4l2m2m")]
    H264V4l2m2m,
}
impl std::fmt::Display for Encoder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            self.to_possible_value()
                .expect("value(skip) not used")
                .get_name()
        )
    }
}
impl Encoder {
    pub fn to_osstring(self) -> OsString {
        OsString::from(
            self.to_possible_value()
                .expect("value(skip) not used")
                .get_name(),
        )
    }
}

#[derive(Debug, thiserror::Error)]
pub enum VideoError {
    #[error("Error when using mkv_element: {0}")]
    MkvElementError(#[from] mkv_element::Error),

    #[error("IO Error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Sequence has no frames")]
    FramesEmpty,

    #[error("wall clock jumped backwards while sequence was recorded: {0}")]
    SystemTimeError(#[from] SystemTimeError),
}

fn open_segment<W: Write>(out: &mut W) -> std::io::Result<()> {
    // 2. Open Segment with UNKNOWN size, by hand.
    //    Segment ID 0x18538067, then the 8-byte unknown-size VINT (0x01 + 7×0xFF).
    out.write_all(&[0x18, 0x53, 0x80, 0x67])?;
    out.write_all(&[0x01, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF])?;
    Ok(())
}

fn simple_block_body(
    out: &mut impl BufMut,
    track: u8,
    rel_ts: i16,
    keyframe: bool,
    payload: &[u8],
) -> std::io::Result<()> {
    out.put_u8(0x80 | track); // track # as 1-byte EBML VINT (1..=127)
    out.put_i16(rel_ts); // i16 big-endian, relative to cluster ts
    out.put_u8(if keyframe { 0x80 } else { 0x00 }); // flags: keyframe set, no lacing
    out.put(payload); // the actual frame
    Ok(())
}
