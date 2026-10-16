//! Byte-size formatting.
//!
//! Poof uses decimal units (1 GB = 1000³ bytes), matching what Finder and
//! System Settings display, so numbers line up with what users already see.

use std::fmt;

const UNITS: [&str; 6] = ["bytes", "KB", "MB", "GB", "TB", "PB"];

/// A number of bytes that formats itself for humans: `ByteSize(1_234_567)` → `1.2 MB`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct ByteSize(pub u64);

impl fmt::Display for ByteSize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let bytes = self.0;
        if bytes < 1000 {
            return if bytes == 1 { write!(f, "1 byte") } else { write!(f, "{bytes} bytes") };
        }
        let mut value = bytes as f64;
        let mut unit = 0;
        while value >= 999.95 && unit < UNITS.len() - 1 {
            value /= 1000.0;
            unit += 1;
        }
        // Three significant digits reads well at every magnitude: 4.07 GB, 40.7 GB, 407 GB.
        let precision = if value < 10.0 {
            2
        } else if value < 100.0 {
            1
        } else {
            0
        };
        write!(f, "{value:.precision$} {}", UNITS[unit])
    }
}

/// Formats a signed difference, e.g. `+3.20 GB` or `−512 MB`.
pub fn signed(delta: i64) -> String {
    let sign = if delta < 0 { "−" } else { "+" };
    format!("{sign}{}", ByteSize(delta.unsigned_abs()))
}

/// Converts whole gigabytes (decimal) to bytes.
pub const fn gb(value: u64) -> u64 {
    value * 1_000_000_000
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_small_values_as_bytes() {
        assert_eq!(ByteSize(0).to_string(), "0 bytes");
        assert_eq!(ByteSize(1).to_string(), "1 byte");
        assert_eq!(ByteSize(999).to_string(), "999 bytes");
    }

    #[test]
    fn formats_with_three_significant_digits() {
        assert_eq!(ByteSize(1_000).to_string(), "1.00 KB");
        assert_eq!(ByteSize(4_070_000_000).to_string(), "4.07 GB");
        assert_eq!(ByteSize(40_700_000_000).to_string(), "40.7 GB");
        assert_eq!(ByteSize(407_000_000_000).to_string(), "407 GB");
    }

    #[test]
    fn rolls_over_to_the_next_unit_instead_of_printing_1000() {
        assert_eq!(ByteSize(999_960).to_string(), "1.00 MB");
        assert_eq!(ByteSize(999_999_999_999).to_string(), "1.00 TB");
    }

    #[test]
    fn formats_signed_deltas() {
        assert_eq!(signed(3_200_000_000), "+3.20 GB");
        assert_eq!(signed(-512_000_000), "−512 MB");
    }
}
