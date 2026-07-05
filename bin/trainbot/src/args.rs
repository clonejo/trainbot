use clap::Parser;

use trainbot_core::{FitMethod, VideoEncoder};

#[derive(Parser, Debug)]
#[command(name = "trainbot", about = "Automatic train sighting detector")]
pub struct DetectArgs {
    // LogConfig
    #[arg(long, env = "LOG_PRETTY", default_value_t = false, help = "log pretty")]
    pub log_pretty: bool,
    #[arg(
        long,
        env = "LOG_LEVEL",
        default_value = "info",
        value_name = "LEVEL",
        help = "log level"
    )]
    pub log_level: String,

    // DataStore
    #[arg(
        long,
        env = "DATA_DIR",
        default_value = "data",
        value_name = "DIR",
        help = "Directory to store output data"
    )]
    pub data_dir: String,

    // Input
    #[arg(
        long,
        env = "INPUT",
        value_name = "FILE",
        help = "Video4linux device file or regular video file, e.g. /dev/video0, video.mp4, or 'picam3'"
    )]
    pub input: String,
    #[arg(
        long,
        env = "CAMERA_FORMAT_FOURCC",
        default_value = "MJPG",
        value_name = "CODE",
        help = "Camera pixel format FourCC string, ignored if using video file"
    )]
    pub camera_format_fourcc: String,
    #[arg(
        long,
        env,
        help = "This option should not typically be needed. By default, we use the current wall clock time as the timestamp for each frame as it comes in. For video files the PTS is used. If your input framerate is constant, but the system time when trainbot consumes frames is too jittery, setting this may help against 'unable to fit' errors. In microseconds."
    )]
    pub constant_frame_time_micros: Option<u64>,
    #[arg(
        long,
        env,
        default_value_t = 200,
        value_name = "N",
        help = "Number of frames to buffer between the frame source and the AutoStitcher thread. (Only used for live sources.)"
    )]
    pub src_buf_cap: usize,
    #[arg(
        long,
        env = "CAMERA_W",
        default_value_t = 1920,
        value_name = "X",
        help = "Camera frame size width, ignored if using video file or picam3"
    )]
    pub camera_w: u32,
    #[arg(
        long,
        env = "CAMERA_H",
        default_value_t = 1080,
        value_name = "Y",
        help = "Camera frame size height, ignored if using video file or picam3"
    )]
    pub camera_h: u32,

    // Rect
    #[arg(
        short = 'X',
        long,
        env = "RECT_X",
        default_value_t = 0,
        value_name = "N",
        help = "Rect to look at, x (left)"
    )]
    pub rect_x: u32,
    #[arg(
        short = 'Y',
        long,
        env = "RECT_Y",
        default_value_t = 0,
        value_name = "N",
        help = "Rect to look at, y (top)"
    )]
    pub rect_y: u32,
    #[arg(
        short = 'W',
        long,
        env = "RECT_W",
        default_value_t = 0,
        value_name = "N",
        help = "Rect to look at, width"
    )]
    pub rect_w: u32,
    #[arg(
        short = 'H',
        long,
        env = "RECT_H",
        default_value_t = 0,
        value_name = "N",
        help = "Rect to look at, height"
    )]
    pub rect_h: u32,
    #[arg(
        long,
        env = "RECT_MASK",
        value_name = "FILE",
        help = "When stitching, only take pixels from the white areas in the mask."
    )]
    pub mask: Option<String>,

    #[arg(
        long,
        default_value_t,
        help = "Which ffmpeg encoder to use for videos. Use h264_v4l2m2m on the raspi 4, and libx264 when your CPU is fast enough."
    )]
    pub video_encoder: VideoEncoder,

    // Camera opts
    #[arg(
        long,
        env = "ROTATE_180",
        help = "Rotate camera picture 180 degrees (only picam3)"
    )]
    pub rotate_180: bool,

    // Physics
    #[arg(
        long,
        env = "PX_PER_M",
        default_value_t = 45.0,
        value_name = "K",
        help = "Pixels per meter"
    )]
    pub px_per_m: f64,
    #[arg(
        long,
        env = "MIN_SPEED_KPH",
        default_value_t = 25.0,
        value_name = "K",
        help = "Assumed train min speed, km/h"
    )]
    pub min_speed_kph: f64,
    #[arg(
        long,
        env = "MAX_SPEED_KPH",
        default_value_t = 160.0,
        value_name = "K",
        help = "Assumed train max speed, km/h"
    )]
    pub max_speed_kph: f64,
    #[arg(
        long,
        env = "MIN_LEN_M",
        default_value_t = 5.0,
        value_name = "K",
        help = "Minimum length of trains"
    )]
    pub min_len_m: f64,
    #[arg(
        long,
        env = "MAX_FRAME_COUNT_PER_SEQ",
        default_value_t = 1500,
        value_name = "N",
        help = "Max frames before force-ending a sequence"
    )]
    pub max_frame_count_per_seq: usize,

    // Profiling (no-op, accepted for compat)
    #[arg(
        long,
        env = "CPU_PROFILE",
        help = "Write CPU profile (accepted, no-op in Rust build)"
    )]
    pub cpu_profile: bool,
    #[arg(
        long,
        env = "HEAP_PROFILE",
        help = "Write memory heap profiles (accepted, no-op in Rust build)"
    )]
    pub heap_profile: bool,

    // Prometheus
    #[arg(
        long,
        env = "PROMETHEUS",
        default_value_t = false,
        help = "Expose Prometheus-compatible metrics endpoint"
    )]
    pub prometheus: bool,
    #[arg(
        long,
        env = "PROMETHEUS_LISTEN",
        default_value = "[::]:18963",
        help = "Prometheus endpoint bind address"
    )]
    pub prometheus_listen: String,

    // Rust-only
    #[arg(
        long,
        default_value = "ransac",
        value_name = "METHOD",
        help = "RANSAC fit method: ols|ransac"
    )]
    pub fit_method: FitMethod,
}

#[derive(Parser, Debug)]
#[command(
    name = "confighelper",
    about = "Web UI helper to find crop-rectangle arguments"
)]
pub struct ConfighelperArgs {
    // LogConfig
    #[arg(long, env = "LOG_PRETTY", default_value_t = true, help = "log pretty")]
    pub log_pretty: bool,
    #[arg(
        long,
        env = "LOG_LEVEL",
        default_value = "info",
        value_name = "LEVEL",
        help = "log level"
    )]
    pub log_level: String,

    #[arg(
        long,
        default_value_t = false,
        help = "Do not bake in WWW static files (no-op in Rust build)"
    )]
    pub live_reload: bool,
    #[arg(
        long,
        default_value = "localhost:8080",
        help = "Address and port to listen on"
    )]
    pub listen_addr: String,

    #[arg(
        long,
        required = true,
        help = "Video4linux device file, e.g. /dev/video0, or 'picam3'"
    )]
    pub input: String,
    #[arg(
        long,
        default_value_t = 1920,
        value_name = "X",
        help = "Camera frame size width, ignored for picam3"
    )]
    pub camera_w: u32,
    #[arg(
        long,
        default_value_t = 1080,
        value_name = "Y",
        help = "Camera frame size height, ignored for picam3"
    )]
    pub camera_h: u32,

    #[arg(
        long,
        env = "ROTATE_180",
        help = "Rotate camera picture 180 degrees (only picam3)"
    )]
    pub rotate_180: bool,

    #[arg(long, help = "Only print v4l camera probe output and exit")]
    pub probe_only: bool,
}

#[derive(Parser, Debug)]
#[command(
    name = "cleanup",
    about = "Find and print removal commands for orphaned blobs not referenced in the database"
)]
pub struct CleanupArgs {
    // LogConfig
    #[arg(long, env = "LOG_PRETTY", default_value_t = false, help = "log pretty")]
    pub log_pretty: bool,
    #[arg(
        long,
        env = "LOG_LEVEL",
        default_value = "info",
        value_name = "LEVEL",
        help = "log level"
    )]
    pub log_level: String,

    // DataStore
    #[arg(
        long,
        env = "DATA_DIR",
        default_value = "data",
        value_name = "DIR",
        help = "Directory to store output data"
    )]
    pub data_dir: String,
}
