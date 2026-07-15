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

/// Formats the remaining time as `−M:SS` (or `−H:MM:SS`), using U+2212
/// MINUS SIGN for visual consistency with tabular-numeral fonts.
pub fn format_remaining(position_ms: i64, duration_ms: i64) -> String {
    let remaining = (duration_ms - position_ms).max(0);
    format!("\u{2212}{}", format_duration(remaining))
}

/// Formats a duration in milliseconds as a human-readable summary of days,
/// hours, and minutes — Rhythmbox-style copy for the library status line
/// (`status_bar.rs`), e.g. `"4 days, 6 hours and 28 minutes"`. Distinct from
/// `format_duration`: that one renders a single *track's* length as `m:ss`;
/// this renders a *library-wide total* at minute granularity (seconds are
/// dropped, not rounded).
///
/// Rules: each of days/hours is included only when nonzero (a leading part
/// that happens to be zero is omitted entirely — e.g. "2 days and 5 minutes"
/// has no "0 hours"); minutes is always included, even when zero, so a
/// zero/negative input still reads as `"0 minutes"` rather than an empty
/// string. Singular/plural wording follows the count (`1 day` vs `2 days`).
/// Multiple parts are joined with ", " except the last two, which are joined
/// with " and ".
pub fn format_total_duration(ms: i64) -> String {
    let total_minutes = ms.max(0) / 1000 / 60;
    let days = total_minutes / (24 * 60);
    let hours = (total_minutes % (24 * 60)) / 60;
    let minutes = total_minutes % 60;

    let mut parts = Vec::with_capacity(3);
    if days > 0 {
        parts.push(pluralize(days, "day", "days"));
    }
    if hours > 0 {
        parts.push(pluralize(hours, "hour", "hours"));
    }
    // Always present, even when zero — see the doc comment above.
    parts.push(pluralize(minutes, "minute", "minutes"));

    join_with_and(&parts)
}

fn pluralize(n: i64, singular: &str, plural: &str) -> String {
    format!("{n} {}", if n == 1 { singular } else { plural })
}

/// Joins 1+ pre-formatted parts with ", " between all but the final pair,
/// and " and " between the final pair — e.g. `["a", "b", "c"] → "a, b and
/// c"`. `parts` is never empty in practice (`format_total_duration` always
/// pushes at least the minutes part), but an empty slice degrades to an
/// empty string rather than panicking.
fn join_with_and(parts: &[String]) -> String {
    match parts.split_last() {
        None => String::new(),
        Some((last, [])) => last.clone(),
        Some((last, rest)) => format!("{} and {last}", rest.join(", ")),
    }
}

/// Formats an integer with `,` thousands separators, en-US style (e.g.
/// `1704` → `"1,704"`) — a `toLocaleString()`-equivalent for the status
/// line's track count. Handles negative values (`-1704` → `"-1,704"`) even
/// though `LibraryStats::track_count` is never expected to be negative, for
/// the same defensive reason `format_duration` clamps rather than assumes.
pub fn format_thousands(n: i64) -> String {
    let digits = n.unsigned_abs().to_string();
    let mut grouped = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, ch) in digits.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(ch);
    }
    let grouped: String = grouped.chars().rev().collect();
    if n < 0 {
        format!("-{grouped}")
    } else {
        grouped
    }
}

/// Formats a Unix timestamp (seconds since the epoch, UTC) as `YYYY-MM-DD
/// HH:MM` — used by the import-errors panel's "Time" column (`ui::import_
/// errors_view`, Stage 3 Task 8). Always UTC (this app has no timezone/
/// locale dependency yet) — consistent and unambiguous is more important
/// here than local-time convenience for a low-traffic diagnostic column.
/// Negative input (clock skew, a malformed row) is clamped to the epoch
/// itself rather than panicking, matching this module's other clamp-not-
/// panic conventions.
pub fn format_unix_timestamp(secs: i64) -> String {
    let secs = secs.max(0);
    let days = secs / 86_400;
    let time_of_day = secs % 86_400;
    let (year, month, day) = civil_from_days(days);
    let hour = time_of_day / 3600;
    let minute = (time_of_day % 3600) / 60;
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}")
}

/// Days-since-epoch (1970-01-01) to a proleptic-Gregorian `(year, month,
/// day)` triple. Howard Hinnant's well-known `civil_from_days` algorithm
/// (<http://howardhinnant.github.io/date_algorithms.html>), reproduced by
/// hand rather than pulling in a date/time crate dependency for the one
/// column that needs it.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = (if z >= 0 { z } else { z - 146_096 }) / 146_097;
    let doe = (z - era * 146_097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let month = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32; // [1, 12]
    let year = if month <= 2 { y + 1 } else { y };
    (year, month, day)
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

    #[test]
    fn total_duration_formats_days_hours_and_minutes() {
        let ms = ((4 * 24 + 6) * 60 + 28) * 60 * 1000;
        assert_eq!(format_total_duration(ms), "4 days, 6 hours and 28 minutes");
    }

    #[test]
    fn total_duration_formats_hours_and_minutes() {
        assert_eq!(
            format_total_duration(90 * 60 * 1000),
            "1 hour and 30 minutes"
        );
    }

    #[test]
    fn total_duration_formats_minutes_only() {
        assert_eq!(format_total_duration(5 * 60 * 1000), "5 minutes");
    }

    #[test]
    fn total_duration_omits_zero_hours_part() {
        let ms = (2 * 24 * 60 + 5) * 60 * 1000;
        assert_eq!(format_total_duration(ms), "2 days and 5 minutes");
    }

    #[test]
    fn total_duration_zero_is_zero_minutes() {
        assert_eq!(format_total_duration(0), "0 minutes");
    }

    #[test]
    fn total_duration_negative_is_zero_minutes() {
        assert_eq!(format_total_duration(-1), "0 minutes");
    }

    #[test]
    fn thousands_groups_large_numbers() {
        assert_eq!(format_thousands(1_704), "1,704");
        assert_eq!(format_thousands(1_000_000), "1,000,000");
    }

    #[test]
    fn thousands_leaves_small_numbers_unchanged() {
        assert_eq!(format_thousands(0), "0");
        assert_eq!(format_thousands(999), "999");
    }

    #[test]
    fn thousands_handles_negative_numbers() {
        assert_eq!(format_thousands(-1_704), "-1,704");
    }

    #[test]
    fn unix_timestamp_formats_the_epoch() {
        assert_eq!(format_unix_timestamp(0), "1970-01-01 00:00");
    }

    #[test]
    fn unix_timestamp_formats_a_well_known_value() {
        // 1_000_000_000 seconds after the epoch is the widely-cited
        // "one billion seconds" instant, 2001-09-09T01:46:40Z.
        assert_eq!(format_unix_timestamp(1_000_000_000), "2001-09-09 01:46");
    }

    #[test]
    fn unix_timestamp_clamps_negative_input_to_the_epoch() {
        assert_eq!(format_unix_timestamp(-5), "1970-01-01 00:00");
    }

    #[test]
    fn format_remaining_shows_negative_remaining_time() {
        assert_eq!(format_remaining(8_000, 68_000), "\u{2212}1:00");
    }

    #[test]
    fn format_remaining_at_start_shows_full_duration() {
        assert_eq!(format_remaining(0, 181_000), "\u{2212}3:01");
    }

    #[test]
    fn format_remaining_at_end_shows_zero() {
        assert_eq!(format_remaining(181_000, 181_000), "\u{2212}0:00");
    }

    #[test]
    fn format_remaining_with_hours() {
        assert_eq!(format_remaining(0, 3_753_000), "\u{2212}1:02:33");
    }
}
