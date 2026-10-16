//! Volume capacity, as macOS itself reports it.
//!
//! "Available" in Finder is *not* what `df` prints: Finder includes
//! **purgeable** space (local Time Machine snapshots, iCloud-evictable files,
//! caches macOS may drop). We read both numbers through Foundation's
//! `NSURL` resource keys so the gap between them can be explained.

use std::path::Path;

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_foundation::{
    NSArray, NSDictionary, NSNumber, NSString, NSURL, NSURLResourceKey,
    NSURLVolumeAvailableCapacityForImportantUsageKey, NSURLVolumeAvailableCapacityKey,
    NSURLVolumeTotalCapacityKey,
};
use serde::{Deserialize, Serialize};

use crate::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct VolumeInfo {
    /// Capacity of the volume (for APFS, of its container).
    pub total: u64,
    /// Space that is free right now (what `df` reports).
    pub free: u64,
    /// Free plus purgeable — what Finder shows as "Available".
    pub available: u64,
}

impl VolumeInfo {
    /// Space macOS can reclaim on demand (snapshots, evictable files, caches).
    pub fn purgeable(&self) -> u64 {
        self.available.saturating_sub(self.free)
    }

    /// Space that is in use and *not* purgeable.
    pub fn used(&self) -> u64 {
        self.total.saturating_sub(self.available)
    }

    /// Fraction of the disk that is in use, `0.0 ..= 1.0`.
    pub fn used_fraction(&self) -> f32 {
        if self.total == 0 { 0.0 } else { self.used() as f32 / self.total as f32 }
    }
}

/// Queries capacity for the volume containing `path` (usually `/`).
pub fn query(path: &Path) -> Result<VolumeInfo> {
    let url = NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));

    // SAFETY: these are immutable `NSString` constants exported by Foundation.
    let (total_key, free_key, important_key) = unsafe {
        (
            NSURLVolumeTotalCapacityKey,
            NSURLVolumeAvailableCapacityKey,
            NSURLVolumeAvailableCapacityForImportantUsageKey,
        )
    };
    let keys = NSArray::from_slice(&[total_key, free_key, important_key]);
    let values = url
        .resourceValuesForKeys_error(&keys)
        .map_err(|e| Error::Other(format!("could not read volume capacity: {}", e.localizedDescription())))?;

    let total = number(&values, total_key);
    let free = number(&values, free_key);
    // `important usage` already includes `free`; never let it drop below it.
    let available = number(&values, important_key).max(free);
    Ok(VolumeInfo { total, free, available })
}

fn number(values: &NSDictionary<NSURLResourceKey, AnyObject>, key: &NSURLResourceKey) -> u64 {
    values
        .objectForKey(key)
        .and_then(|value: Retained<AnyObject>| value.downcast::<NSNumber>().ok())
        .map_or(0, |n| u64::try_from(n.longLongValue()).unwrap_or(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derived_values() {
        let v = VolumeInfo { total: 100, free: 20, available: 35 };
        assert_eq!(v.purgeable(), 15);
        assert_eq!(v.used(), 65);
        assert!((v.used_fraction() - 0.65).abs() < f32::EPSILON);
        assert_eq!(VolumeInfo::default().used_fraction(), 0.0);
    }

    #[test]
    fn queries_the_boot_volume() {
        let v = query(Path::new("/")).unwrap();
        assert!(v.total > 0);
        assert!(v.available >= v.free);
        assert!(v.total >= v.available);
    }
}
