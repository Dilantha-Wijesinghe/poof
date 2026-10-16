//! Timestamps are stored as Unix seconds (`i64`) everywhere: compact, sortable
//! and trivially serialisable. This module converts them for display.

use std::time::{SystemTime, UNIX_EPOCH};

use jiff::{Timestamp, tz::TimeZone};

pub const MINUTE: i64 = 60;
pub const HOUR: i64 = 60 * MINUTE;
pub const DAY: i64 = 24 * HOUR;

/// The current time in Unix seconds.
pub fn now() -> i64 {
    to_unix(SystemTime::now())
}

/// Converts a [`SystemTime`] to Unix seconds (negative before 1970).
pub fn to_unix(time: SystemTime) -> i64 {
    match time.duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_secs() as i64,
        Err(e) => -(e.duration().as_secs() as i64),
    }
}

/// "just now", "5 minutes ago", "3 days ago", "Mar 4, 2025".
pub fn relative(timestamp: i64) -> String {
    relative_to(timestamp, now())
}

fn relative_to(timestamp: i64, now: i64) -> String {
    let elapsed = now - timestamp;
    let plural = |n: i64, unit: &str| format!("{n} {unit}{} ago", if n == 1 { "" } else { "s" });
    match elapsed {
        e if e < MINUTE => "just now".to_owned(),
        e if e < HOUR => plural(e / MINUTE, "minute"),
        e if e < DAY => plural(e / HOUR, "hour"),
        e if e < 30 * DAY => plural(e / DAY, "day"),
        _ => date(timestamp),
    }
}

/// A short local date such as `Mar 4, 2025`.
pub fn date(timestamp: i64) -> String {
    format_local(timestamp, "%b %-d, %Y")
}

/// A short local date and time such as `Mar 4, 14:05`.
pub fn date_time(timestamp: i64) -> String {
    format_local(timestamp, "%b %-d, %H:%M")
}

fn format_local(timestamp: i64, pattern: &str) -> String {
    Timestamp::from_second(timestamp)
        .map(|t| t.to_zoned(TimeZone::system()).strftime(pattern).to_string())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_times_read_naturally() {
        let now = 1_700_000_000;
        assert_eq!(relative_to(now - 10, now), "just now");
        assert_eq!(relative_to(now - MINUTE, now), "1 minute ago");
        assert_eq!(relative_to(now - 5 * HOUR, now), "5 hours ago");
        assert_eq!(relative_to(now - 3 * DAY, now), "3 days ago");
        assert!(relative_to(now - 90 * DAY, now).contains(", 20"));
    }
}
