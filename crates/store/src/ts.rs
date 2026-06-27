use chrono::{DateTime, FixedOffset};

/// Format a timestamp for blob filenames, matching Go's `"20060102_150405.999_Z07:00"`.
///
/// Rules:
/// - Milliseconds only (nanoseconds truncated, not rounded)
/// - Trailing zeros trimmed from the fractional part
/// - Offset +00:00 and UTC both render as `Z`
pub fn format_file_ts(dt: &DateTime<FixedOffset>) -> String {
    let millis = dt.timestamp_subsec_nanos() / 1_000_000;
    let frac = {
        let s = format!("{:03}", millis);
        let trimmed = s.trim_end_matches('0');
        if trimmed.is_empty() {
            String::new()
        } else {
            format!(".{}", trimmed)
        }
    };
    let tz = format_offset(dt.offset().local_minus_utc());
    format!("{}{frac}_{tz}", dt.format("%Y%m%d_%H%M%S"))
}

/// Format a timestamp for SQLite storage, matching Go's sqlite driver output (RFC 3339,
/// sub-second precision with trailing zeros trimmed, timezone preserved).
pub fn format_db_ts(dt: &DateTime<FixedOffset>) -> String {
    let nanos = dt.timestamp_subsec_nanos();
    let frac = format_nanos_trimmed(nanos);
    let tz = format_db_offset(dt.offset().local_minus_utc());
    format!("{}{frac}{tz}", dt.format("%Y-%m-%dT%H:%M:%S"))
}

/// Parse a timestamp from SQLite storage (RFC 3339 with optional fractional seconds).
pub fn parse_db_ts(s: &str) -> Result<DateTime<FixedOffset>, chrono::ParseError> {
    // Try with sub-seconds first, then without.
    DateTime::parse_from_rfc3339(s)
}

fn format_nanos_trimmed(nanos: u32) -> String {
    if nanos == 0 {
        return String::new();
    }
    let s = format!("{:09}", nanos);
    format!(".{}", s.trim_end_matches('0'))
}

/// Offset → `Z` for ±0, else `+HH:MM` / `-HH:MM`.
fn format_offset(offset_secs: i32) -> String {
    if offset_secs == 0 {
        return "Z".to_string();
    }
    let sign = if offset_secs > 0 { '+' } else { '-' };
    let abs = offset_secs.unsigned_abs();
    format!("{}{:02}:{:02}", sign, abs / 3600, (abs % 3600) / 60)
}

/// DB offset: never uses `Z`; always `+HH:MM` (matches Go's modernc.org/sqlite behaviour).
fn format_db_offset(offset_secs: i32) -> String {
    let sign = if offset_secs >= 0 { '+' } else { '-' };
    let abs = offset_secs.unsigned_abs();
    format!("{}{:02}:{:02}", sign, abs / 3600, (abs % 3600) / 60)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::DateTime;

    fn parse(s: &str) -> DateTime<FixedOffset> {
        DateTime::parse_from_rfc3339(s).unwrap()
    }

    #[test]
    fn file_ts_utc_trailing_zero() {
        // 660009478ns → 660ms → ".66" (trailing zero trimmed), UTC → Z
        let dt = parse("2023-12-24T09:58:52.660009478Z");
        assert_eq!(format_file_ts(&dt), "20231224_095852.66_Z");
    }

    #[test]
    fn file_ts_utc_no_trim() {
        let dt = parse("2023-12-24T11:19:12.839262415Z");
        assert_eq!(format_file_ts(&dt), "20231224_111912.839_Z");
    }

    #[test]
    fn file_ts_positive_offset() {
        let dt = parse("2023-10-28T17:31:50.709434526+01:00");
        assert_eq!(format_file_ts(&dt), "20231028_173150.709_+01:00");
    }

    #[test]
    fn file_ts_zero_offset_is_z() {
        // +00:00 must render as Z
        let dt = parse("2023-11-25T15:49:46.958831882+00:00");
        assert_eq!(format_file_ts(&dt), "20231125_154946.958_Z");
    }

    #[test]
    fn file_ts_positive_offset2() {
        let dt = parse("2023-03-28T06:32:16.516941205+01:00");
        assert_eq!(format_file_ts(&dt), "20230328_063216.516_+01:00");
    }

    #[test]
    fn db_ts_roundtrip() {
        let dt = parse("2023-06-10T16:20:58.805+02:00");
        let stored = format_db_ts(&dt);
        assert_eq!(stored, "2023-06-10T16:20:58.805+02:00");
        let back = parse_db_ts(&stored).unwrap();
        assert_eq!(back, dt);
    }

    #[test]
    fn db_ts_millis() {
        let dt = parse("2023-11-10T12:57:45.897+01:00");
        assert_eq!(format_db_ts(&dt), "2023-11-10T12:57:45.897+01:00");
    }
}
