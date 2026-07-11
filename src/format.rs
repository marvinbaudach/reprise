//! Display formatting helpers shared by the UI layer.

/// Formats a duration in milliseconds as `m:ss` (or `h:mm:ss` once the hour
/// mark is reached), matching how track lengths are conventionally shown in
/// music players. Negative input (never expected from the database, but
/// `duration_ms` is a plain `i64` with no type-level guarantee) is clamped to
/// zero rather than panicking or producing a negative-looking string.
pub fn format_duration(ms: i64) -> String {
    let total_secs = ms.max(0) / 1000;
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let seconds = total_secs % 60;
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_minutes_and_seconds() {
        assert_eq!(format_duration(181_000), "3:01");
    }

    #[test]
    fn formats_sub_minute_with_zero_padded_seconds() {
        assert_eq!(format_duration(59_000), "0:59");
    }

    #[test]
    fn formats_hours_once_past_the_hour_mark() {
        assert_eq!(format_duration(3_753_000), "1:02:33");
    }

    #[test]
    fn clamps_negative_input_to_zero() {
        assert_eq!(format_duration(-5), "0:00");
    }

    #[test]
    fn formats_exact_zero() {
        assert_eq!(format_duration(0), "0:00");
    }
}
