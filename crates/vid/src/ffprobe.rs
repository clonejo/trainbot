use crate::{Error, Result};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use std::time::SystemTime;

#[derive(Debug, Deserialize)]
struct ProbeOutput {
    streams: Vec<StreamJson>,
}

#[derive(Debug, Deserialize)]
struct StreamJson {
    codec_type: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    avg_frame_rate: Option<String>,
    tags: Option<TagsJson>,
}

#[derive(Debug, Deserialize)]
struct TagsJson {
    creation_time: Option<String>,
}

pub struct VideoInfo {
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub start_time: Option<SystemTime>,
}

pub fn probe(path: &str) -> Result<VideoInfo> {
    #[rustfmt::skip]
    let out = duct::cmd!(
        "ffprobe",
        "-v", "quiet",
        "-print_format", "json",
        "-show_streams",
        path
    )
    .stdout_capture()
    .run()
    .map_err(Error::ProbeRun)?;

    let parsed: ProbeOutput = serde_json::from_slice(&out.stdout).map_err(Error::ProbeJson)?;

    let video: Vec<_> = parsed
        .streams
        .iter()
        .filter(|s| s.codec_type.as_deref() == Some("video"))
        .collect();

    let stream = match video.len() {
        0 => return Err(Error::NoVideoStream),
        1 => video[0],
        _ => return Err(Error::MultipleVideoStreams),
    };

    let width = stream.width.ok_or(Error::ProbeMissing("width"))?;
    let height = stream.height.ok_or(Error::ProbeMissing("height"))?;
    let fps = parse_fps(
        stream
            .avg_frame_rate
            .as_deref()
            .ok_or(Error::ProbeMissing("avg_frame_rate"))?,
    )?;

    let start_time = stream
        .tags
        .as_ref()
        .and_then(|t| t.creation_time.as_deref())
        .and_then(|s| s.parse::<DateTime<Utc>>().ok())
        .map(SystemTime::from);

    Ok(VideoInfo {
        width,
        height,
        fps,
        start_time,
    })
}

fn parse_fps(s: &str) -> Result<f64> {
    let (num_s, den_s) = s
        .split_once('/')
        .ok_or_else(|| Error::InvalidFps(s.to_string()))?;
    let num: f64 = num_s
        .parse()
        .map_err(|_| Error::InvalidFps(s.to_string()))?;
    let den: f64 = den_s
        .parse()
        .map_err(|_| Error::InvalidFps(s.to_string()))?;
    if den == 0.0 {
        return Err(Error::InvalidFps(format!("{s}: zero denominator")));
    }
    Ok(num / den)
}
