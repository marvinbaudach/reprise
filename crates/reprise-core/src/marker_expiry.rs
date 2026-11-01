//! One expiry rule for the on-disk "asked, nothing to show" markers (NET-7e), shared by the
//! artist-portrait cache and the album-cover cache so both expire on the same boundary.

use std::time::SystemTime;

/// How far ahead of `now` a marker's modification time may be and still read as a recent
/// write. Further ahead it is a bad clock, which must not pin the marker for weeks.
const FUTURE_MTIME_TOLERANCE_SECONDS: i64 = 24 * 60 * 60;

/// A marker is fresh while its age is below `ttl_seconds`; exactly the TTL is expired.
/// `age_seconds` is negative when the marker is dated after `now`.
pub(crate) fn marker_is_fresh(age_seconds: i64, ttl_seconds: i64) -> bool {
    age_seconds >= -FUTURE_MTIME_TOLERANCE_SECONDS && age_seconds < ttl_seconds
}

/// Whole seconds from `modified` to `now`, negative when `modified` is later.
pub(crate) fn age_seconds(now: SystemTime, modified: SystemTime) -> i64 {
    match now.duration_since(modified) {
        Ok(age) => i64::try_from(age.as_secs()).unwrap_or(i64::MAX),
        Err(ahead) => i64::try_from(ahead.duration().as_secs()).map_or(i64::MIN, |secs| -secs),
    }
}
