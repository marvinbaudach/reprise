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

/// A complete display format: how this system writes a date, and on which
/// dial it writes the time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DateTimeFormat {
    pub date: DatePattern,
    pub clock: ClockConvention,
}

impl DateTimeFormat {
    /// The locale-independent fallback, used before a frontend has supplied
    /// the platform's own and by tests that assert an exact string.
    pub fn iso() -> Self {
        Self {
            date: DatePattern::from_platform(DatePattern::ISO),
            clock: ClockConvention::Hours24,
        }
    }
}

/// Initialises the process locale from the environment and returns the
/// selected message locale as opaque bytes.
///
/// Reprise calls this exactly once during single-threaded startup, before
/// constructing the application or querying the locale's date patterns.
#[cfg(unix)]
pub fn initialize_system_locale() -> Option<Vec<u8>> {
    let selected_locale = setlocale_and_copy(libc::LC_ALL);
    let message_locale = setlocale_and_copy(libc::LC_MESSAGES);
    message_locale.or(selected_locale)
}

#[cfg(not(unix))]
pub fn initialize_system_locale() -> Option<Vec<u8>> {
    None
}

#[cfg(unix)]
fn setlocale_and_copy(category: libc::c_int) -> Option<Vec<u8>> {
    // SAFETY: `initialize_system_locale` has one call site: `i18n::init`
    // invokes it before constructing the application and before any thread
    // exists. The locale argument is a static NUL-terminated empty string.
    // `setlocale` returns C-library-owned storage, so the bytes are copied
    // before the next call can invalidate the pointer.
    unsafe {
        let raw = libc::setlocale(category, c"".as_ptr());
        (!raw.is_null()).then(|| std::ffi::CStr::from_ptr(raw).to_bytes().to_owned())
    }
}

/// Returns the date pattern selected by the system's current time locale.
///
/// Frontends must initialise the process locale before calling this. On
/// platforms without `nl_langinfo`, Reprise's ISO date pattern is returned.
#[cfg(unix)]
pub fn system_date_pattern() -> String {
    langinfo(libc::D_FMT)
}

#[cfg(not(unix))]
pub fn system_date_pattern() -> String {
    DatePattern::ISO.to_owned()
}

/// Returns the time pattern selected by the system's current time locale.
///
/// Frontends use this only to derive the twelve- or twenty-four-hour clock
/// convention. On platforms without `nl_langinfo`, a 24-hour pattern is
/// returned.
#[cfg(unix)]
pub fn system_time_pattern() -> String {
    langinfo(libc::T_FMT)
}

#[cfg(not(unix))]
pub fn system_time_pattern() -> String {
    "%H:%M".to_owned()
}

#[cfg(unix)]
fn langinfo(item: libc::nl_item) -> String {
    // SAFETY: `nl_langinfo` returns a pointer to a NUL-terminated string owned
    // by the C library, valid until the next `setlocale` on this thread. The
    // bytes are copied out immediately and the pointer is never retained, and
    // the only `setlocale` in this process runs once in `i18n::init` before
    // this is ever called.
    unsafe {
        let raw = libc::nl_langinfo(item);
        if raw.is_null() {
            return String::new();
        }
        std::ffi::CStr::from_ptr(raw).to_string_lossy().into_owned()
    }
}

pub fn format_unix_timestamp(secs: i64, format: &DateTimeFormat) -> String {
    let secs = secs.max(0);
    let days = secs / 86_400;
    let time_of_day = secs % 86_400;
    let (year, month, day) = civil_from_days(days);
    let hour = time_of_day / 3600;
    let minute = (time_of_day % 3600) / 60;
    let year = i32::try_from(year).unwrap_or(i32::MAX);
    let date = format.date.render(Some(year), Some(month), Some(day));
    format!("{date} {}", format.clock.render(hour, minute))
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

/// The three numeric fields a locale date pattern may carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DateField {
    Day,
    Month,
    Year,
}

/// A locale date pattern reduced to what Reprise is willing to render: the
/// three numeric fields and the literals between them.
///
/// Reprise takes the *order and punctuation* from the system and nothing
/// else. A locale that spells the month (`%b`, `%B`) or names the weekday
/// (`%a`, `%A`) is not rendered in its own shape — the whole pattern falls
/// back to ISO — because a month name is exactly what this change exists to
/// remove. A two-digit year (`%y`, which glibc still hands out for `en_US`)
/// is upgraded rather than rejected: the field is right, only its width is
/// wrong.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatePattern {
    /// Literal text before the first field. Usually empty.
    prefix: String,
    /// Each field with the literal run that follows it.
    fields: Vec<(DateField, String)>,
}

impl DatePattern {
    /// The fallback whenever the platform pattern cannot be rendered
    /// numerically. Unambiguous in every locale and already the shape the
    /// library's "Added" column has always used.
    pub const ISO: &'static str = "%Y-%m-%d";

    /// Reduces a platform strftime pattern to a [`DatePattern`], falling back
    /// to [`Self::ISO`] when it carries anything this renderer will not
    /// print.
    pub fn from_platform(raw: &str) -> Self {
        Self::parse(raw).unwrap_or_else(|| {
            Self::parse(Self::ISO).expect("the ISO pattern is renderable by construction")
        })
    }

    fn parse(raw: &str) -> Option<Self> {
        let mut prefix = String::new();
        let mut fields: Vec<(DateField, String)> = Vec::new();
        let mut chars = raw.chars().peekable();

        while let Some(character) = chars.next() {
            if character != '%' {
                push_literal(&mut prefix, &mut fields, character);
                continue;
            }
            // Skip the padding and locale modifiers glibc allows between the
            // percent and the conversion character (`%-d`, `%_d`, `%0e`,
            // `%Ey`), so a padded field is still recognised as its field.
            let mut conversion = chars.next()?;
            while matches!(conversion, '-' | '_' | '0' | '^' | '#' | 'E' | 'O') {
                conversion = chars.next()?;
            }
            let field = match conversion {
                'd' | 'e' => DateField::Day,
                'm' => DateField::Month,
                'Y' | 'y' => DateField::Year,
                '%' => {
                    push_literal(&mut prefix, &mut fields, '%');
                    continue;
                }
                // Month names, weekday names, day-of-year, compound
                // conversions — anything else means this locale's shape is
                // not one Reprise renders.
                _ => return None,
            };
            if fields.iter().any(|(seen, _)| *seen == field) {
                return None; // a repeated field is not a date pattern
            }
            fields.push((field, String::new()));
        }

        // All three fields must be present; a pattern missing one is not a
        // full date and would silently drop information.
        let complete = [DateField::Day, DateField::Month, DateField::Year]
            .iter()
            .all(|field| fields.iter().any(|(seen, _)| seen == field));
        complete.then_some(Self { prefix, fields })
    }

    /// Renders the date, omitting absent fields together with the literal run
    /// that follows them.
    ///
    /// A day without a month is not a date anyone can read, so the day is
    /// dropped in that case. When any field is omitted, a trailing run of
    /// ASCII punctuation or whitespace is trimmed — a dangling `/` or `.`
    /// reads as truncation. Non-ASCII trailing text (the CJK unit markers) is
    /// kept, because there it carries the meaning of the field. A complete
    /// date is reproduced verbatim, trailing punctuation included.
    pub fn render(&self, year: Option<i32>, month: Option<u32>, day: Option<u32>) -> String {
        let day = month.and(day);
        let omitted = year.is_none() || month.is_none() || day.is_none();

        let mut out = self.prefix.clone();
        for (field, suffix) in &self.fields {
            let value = match field {
                DateField::Day => day.map(|day| format!("{day:02}")),
                DateField::Month => month.map(|month| format!("{month:02}")),
                DateField::Year => year.map(|year| format!("{year:04}")),
            };
            if let Some(value) = value {
                out.push_str(&value);
                out.push_str(suffix);
            }
        }

        if omitted {
            let trimmed = out.trim_end_matches(|character: char| {
                character.is_ascii_punctuation() || character.is_whitespace()
            });
            return trimmed.trim_start().to_owned();
        }
        out
    }
}

/// Appends a literal character to whichever run is currently open: the
/// prefix while no field has been seen, otherwise the suffix of the last one.
fn push_literal(prefix: &mut String, fields: &mut [(DateField, String)], character: char) {
    match fields.last_mut() {
        Some((_, suffix)) => suffix.push(character),
        None => prefix.push(character),
    }
}

/// Whether the locale writes the time on a twelve- or twenty-four-hour dial.
///
/// Reprise takes only that choice from the system and never the locale's full
/// time pattern: `T_FMT` carries seconds in most locales, and a second-precise
/// timestamp in a table cell is noise.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClockConvention {
    Hours24,
    Hours12,
}

impl ClockConvention {
    /// Derives the dial from a platform time pattern. Any twelve-hour
    /// conversion — the hour itself (`%I`, `%l`), the meridiem (`%p`, `%P`)
    /// or the compound twelve-hour time (`%r`) — makes it twelve.
    pub fn from_platform(t_fmt: &str) -> Self {
        let twelve = ["%I", "%l", "%p", "%P", "%r"]
            .iter()
            .any(|marker| t_fmt.contains(marker));
        if twelve {
            Self::Hours12
        } else {
            Self::Hours24
        }
    }

    /// Renders hour and minute. Seconds are never shown.
    pub fn render(self, hour: i64, minute: i64) -> String {
        match self {
            Self::Hours24 => format!("{hour:02}:{minute:02}"),
            Self::Hours12 => {
                let meridiem = if hour < 12 { "AM" } else { "PM" };
                let hour = match hour % 12 {
                    0 => 12,
                    other => other,
                };
                format!("{hour}:{minute:02} {meridiem}")
            }
        }
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
        assert_eq!(
            format_unix_timestamp(0, &DateTimeFormat::iso()),
            "1970-01-01 00:00"
        );
    }

    #[test]
    fn unix_timestamp_formats_a_well_known_value() {
        // 1_000_000_000 seconds after the epoch is the widely-cited
        // "one billion seconds" instant, 2001-09-09T01:46:40Z.
        assert_eq!(
            format_unix_timestamp(1_000_000_000, &DateTimeFormat::iso()),
            "2001-09-09 01:46"
        );
    }

    #[test]
    fn unix_timestamp_clamps_negative_input_to_the_epoch() {
        assert_eq!(
            format_unix_timestamp(-5, &DateTimeFormat::iso()),
            "1970-01-01 00:00"
        );
    }

    #[test]
    fn unix_timestamp_follows_the_supplied_format() {
        let german = DateTimeFormat {
            date: DatePattern::from_platform("%d.%m.%Y"),
            clock: ClockConvention::Hours24,
        };
        assert_eq!(
            format_unix_timestamp(1_000_000_000, &german),
            "09.09.2001 01:46"
        );
        let american = DateTimeFormat {
            date: DatePattern::from_platform("%m/%d/%y"),
            clock: ClockConvention::Hours12,
        };
        assert_eq!(
            format_unix_timestamp(1_000_000_000, &american),
            "09/09/2001 1:46 AM"
        );
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

    #[test]
    fn date_pattern_renders_the_day_first_convention() {
        let pattern = DatePattern::from_platform("%d.%m.%Y");
        assert_eq!(pattern.render(Some(2026), Some(5), Some(29)), "29.05.2026");
        assert_eq!(pattern.render(Some(2026), Some(5), None), "05.2026");
        assert_eq!(pattern.render(Some(2026), None, None), "2026");
        assert_eq!(pattern.render(None, Some(8), Some(15)), "15.08");
    }

    #[test]
    fn date_pattern_renders_the_month_first_convention() {
        let pattern = DatePattern::from_platform("%m/%d/%Y");
        assert_eq!(pattern.render(Some(2026), Some(5), Some(29)), "05/29/2026");
        assert_eq!(pattern.render(Some(2026), Some(5), None), "05/2026");
        assert_eq!(pattern.render(None, Some(8), Some(15)), "08/15");
    }

    #[test]
    fn date_pattern_keeps_non_ascii_unit_markers_when_fields_drop() {
        let pattern = DatePattern::from_platform("%Y年%m月%d日");
        assert_eq!(
            pattern.render(Some(2026), Some(5), Some(29)),
            "2026年05月29日"
        );
        assert_eq!(pattern.render(Some(2026), Some(5), None), "2026年05月");
        assert_eq!(pattern.render(Some(2026), None, None), "2026年");
    }

    #[test]
    fn date_pattern_reproduces_a_complete_date_verbatim() {
        // Hungarian ends a full date with a period; a complete render must
        // not trim it. Only an omitted field licenses trailing trimming.
        let pattern = DatePattern::from_platform("%Y. %m. %d.");
        assert_eq!(
            pattern.render(Some(2026), Some(5), Some(29)),
            "2026. 05. 29."
        );
        assert_eq!(pattern.render(Some(2026), Some(5), None), "2026. 05");
    }

    #[test]
    fn date_pattern_upgrades_a_two_digit_year() {
        // glibc hands out %m/%d/%y for en_US. Four digits, always.
        let pattern = DatePattern::from_platform("%m/%d/%y");
        assert_eq!(pattern.render(Some(2026), Some(5), Some(29)), "05/29/2026");
    }

    #[test]
    fn date_pattern_falls_back_to_iso_for_a_non_numeric_pattern() {
        for raw in ["%a, %b %-d, %Y", "%A %d %B %Y", "", "%d.%m", "nonsense"] {
            assert_eq!(
                DatePattern::from_platform(raw).render(Some(2026), Some(5), Some(29)),
                "2026-05-29",
                "pattern {raw:?} should have fallen back to ISO"
            );
        }
    }

    #[test]
    fn date_pattern_ignores_a_day_without_a_month() {
        let pattern = DatePattern::from_platform("%d.%m.%Y");
        assert_eq!(pattern.render(Some(2026), None, Some(29)), "2026");
    }

    #[test]
    fn clock_convention_reads_twelve_hours_from_the_locale() {
        for raw in ["%I:%M:%S %p", "%r", "%l:%M %P"] {
            assert_eq!(
                ClockConvention::from_platform(raw),
                ClockConvention::Hours12,
                "pattern {raw:?} is a twelve-hour locale"
            );
        }
        for raw in ["%H:%M:%S", "%T", ""] {
            assert_eq!(
                ClockConvention::from_platform(raw),
                ClockConvention::Hours24,
                "pattern {raw:?} is a twenty-four-hour locale"
            );
        }
    }

    #[test]
    fn clock_convention_renders_minutes_and_never_seconds() {
        assert_eq!(ClockConvention::Hours24.render(14, 3), "14:03");
        assert_eq!(ClockConvention::Hours24.render(0, 0), "00:00");
        assert_eq!(ClockConvention::Hours12.render(14, 3), "2:03 PM");
        assert_eq!(ClockConvention::Hours12.render(0, 5), "12:05 AM");
        assert_eq!(ClockConvention::Hours12.render(12, 0), "12:00 PM");
    }
}
