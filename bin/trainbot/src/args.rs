use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "trainbot", about = "Automatic train sighting detector")]
pub struct DetectArgs {
    // LogConfig
    #[arg(long, env = "LOG_PRETTY", default_value_t = false, help = "log pretty")]
    pub log_pretty: bool,
    #[arg(long, env = "LOG_LEVEL", default_value = "info", value_name = "LEVEL", help = "log level")]
    pub log_level: String,

    // DataStore
    #[arg(long, env = "DATA_DIR", default_value = "data", value_name = "DIR", help = "Directory to store output data")]
    pub data_dir: String,

    // Input
    #[arg(long, env = "INPUT", value_name = "FILE", help = "Video4linux device file or regular video file, e.g. /dev/video0, video.mp4, or 'picam3'")]
    pub input: String,
    #[arg(long, env = "CAMERA_FORMAT_FOURCC", default_value = "MJPG", value_name = "CODE", help = "Camera pixel format FourCC string, ignored if using video file")]
    pub camera_format_fourcc: String,
    #[arg(long, env = "CAMERA_W", default_value_t = 1920, value_name = "X", help = "Camera frame size width, ignored if using video file or picam3")]
    pub camera_w: u32,
    #[arg(long, env = "CAMERA_H", default_value_t = 1080, value_name = "Y", help = "Camera frame size height, ignored if using video file or picam3")]
    pub camera_h: u32,

    // Rect
    #[arg(short = 'X', long, env = "RECT_X", default_value_t = 0, value_name = "N", help = "Rect to look at, x (left)")]
    pub rect_x: u32,
    #[arg(short = 'Y', long, env = "RECT_Y", default_value_t = 0, value_name = "N", help = "Rect to look at, y (top)")]
    pub rect_y: u32,
    #[arg(short = 'W', long, env = "RECT_W", default_value_t = 0, value_name = "N", help = "Rect to look at, width")]
    pub rect_w: u32,
    #[arg(short = 'H', long, env = "RECT_H", default_value_t = 0, value_name = "N", help = "Rect to look at, height")]
    pub rect_h: u32,
    #[arg(long, env = "RECT_MASK", value_name = "FILE", help = "When stitching, only take pixels from the white areas in the mask.")]
    pub mask: Option<String>,

    // Camera opts
    #[arg(long, env = "ROTATE_180", help = "Rotate camera picture 180 degrees (only picam3)")]
    pub rotate_180: bool,

    // Physics
    #[arg(long, env = "PX_PER_M", default_value_t = 45.0, value_name = "K", help = "Pixels per meter")]
    pub px_per_m: f64,
    #[arg(long, env = "MIN_SPEED_KPH", default_value_t = 25.0, value_name = "K", help = "Assumed train min speed, km/h")]
    pub min_speed_kph: f64,
    #[arg(long, env = "MAX_SPEED_KPH", default_value_t = 160.0, value_name = "K", help = "Assumed train max speed, km/h")]
    pub max_speed_kph: f64,
    #[arg(long, env = "MIN_LEN_M", default_value_t = 5.0, value_name = "K", help = "Minimum length of trains")]
    pub min_len_m: f64,
    #[arg(long, env = "MAX_FRAME_COUNT_PER_SEQ", default_value_t = 1500, value_name = "N", help = "Max frames before force-ending a sequence")]
    pub max_frame_count_per_seq: usize,

    // Profiling (no-op, accepted for compat)
    #[arg(long, env = "CPU_PROFILE", help = "Write CPU profile (accepted, no-op in Rust build)")]
    pub cpu_profile: bool,
    #[arg(long, env = "HEAP_PROFILE", help = "Write memory heap profiles (accepted, no-op in Rust build)")]
    pub heap_profile: bool,

    // Upload (accepted-and-ignored; upload is Phase 8)
    #[arg(long, env = "ENABLE_UPLOAD", help = "Enable uploading of data (not yet implemented; accepted for compat)")]
    pub enable_upload: bool,
    #[arg(long, env = "UPLOAD_FTP_HOST", value_name = "HOST", help = "FTP hostname")]
    pub upload_ftp_host: Option<String>,
    #[arg(long, env = "UPLOAD_FTP_PORT", default_value_t = 21, value_name = "PORT", help = "FTP port")]
    pub upload_ftp_port: u16,
    #[arg(long, env = "UPLOAD_FTP_USER", value_name = "USER", help = "FTP username")]
    pub upload_ftp_user: Option<String>,
    #[arg(long, env = "UPLOAD_FTP_PASSWORD", value_name = "PASS", help = "FTP password")]
    pub upload_ftp_password: Option<String>,
    #[arg(long, env = "UPLOAD_FTP_PWD", default_value = ".", value_name = "DIR", help = "FTP working directory")]
    pub upload_ftp_pwd: String,

    // Prometheus
    #[arg(long, env = "PROMETHEUS", default_value_t = false, help = "Expose Prometheus-compatible metrics endpoint")]
    pub prometheus: bool,
    #[arg(long, env = "PROMETHEUS_LISTEN", default_value = ":18963", help = "Prometheus endpoint bind address")]
    pub prometheus_listen: String,

    // Rust-only
    #[arg(long, default_value = "ols", value_name = "METHOD", help = "RANSAC fit method: ols|ransac")]
    pub fit_method: String,
}

#[derive(Parser, Debug)]
#[command(name = "confighelper", about = "Web UI helper to find crop-rectangle arguments")]
pub struct ConfighelperArgs {
    // LogConfig
    #[arg(long, env = "LOG_PRETTY", default_value_t = true, help = "log pretty")]
    pub log_pretty: bool,
    #[arg(long, env = "LOG_LEVEL", default_value = "info", value_name = "LEVEL", help = "log level")]
    pub log_level: String,

    #[arg(long, default_value_t = false, help = "Do not bake in WWW static files (no-op in Rust build)")]
    pub live_reload: bool,
    #[arg(long, default_value = "localhost:8080", help = "Address and port to listen on")]
    pub listen_addr: String,

    #[arg(long, required = true, help = "Video4linux device file, e.g. /dev/video0, or 'picam3'")]
    pub input: String,
    #[arg(long, default_value_t = 1920, value_name = "X", help = "Camera frame size width, ignored for picam3")]
    pub camera_w: u32,
    #[arg(long, default_value_t = 1080, value_name = "Y", help = "Camera frame size height, ignored for picam3")]
    pub camera_h: u32,

    #[arg(long, env = "ROTATE_180", help = "Rotate camera picture 180 degrees (only picam3)")]
    pub rotate_180: bool,

    #[arg(long, help = "Only print v4l camera probe output and exit")]
    pub probe_only: bool,
}

#[derive(Parser, Debug)]
#[command(name = "cleanup", about = "Find and print removal commands for orphaned blobs not referenced in the database")]
pub struct CleanupArgs {
    // LogConfig
    #[arg(long, env = "LOG_PRETTY", default_value_t = false, help = "log pretty")]
    pub log_pretty: bool,
    #[arg(long, env = "LOG_LEVEL", default_value = "info", value_name = "LEVEL", help = "log level")]
    pub log_level: String,

    // DataStore
    #[arg(long, env = "DATA_DIR", default_value = "data", value_name = "DIR", help = "Directory to store output data")]
    pub data_dir: String,
}
