use tracing_subscriber::{
    EnvFilter,
    fmt::{self, format::FmtSpan},
};

#[derive(Clone, Debug)]
pub struct LogConfig {
    /// Human-readable console output.  Default: false (JSON output).
    pub log_pretty: bool,
    /// tracing-compatible filter string, e.g. "info", "debug", "trainbot=trace".
    pub log_level: String,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            log_pretty: false,
            log_level: "info".to_owned(),
        }
    }
}

/// Initialize the global tracing subscriber.  Call once at startup.
///
/// Silently no-ops if a subscriber is already set (safe to call in tests).
pub fn init_logging(config: &LogConfig) {
    let level = if config.log_level.is_empty() {
        "info"
    } else {
        &config.log_level
    };
    let filter = EnvFilter::try_new(level).unwrap_or_else(|_| EnvFilter::new("info"));

    if config.log_pretty {
        let _ = fmt::fmt()
            .with_env_filter(filter)
            .with_span_events(FmtSpan::CLOSE)
            .try_init();
    } else {
        let _ = fmt::fmt()
            .json()
            .with_env_filter(filter)
            .with_span_events(FmtSpan::CLOSE)
            .try_init();
    }
}
