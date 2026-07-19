//! My Stats copy: hero figures, the tag-spelling hint, the mix CTA and the
//! failure state. The rest of the page's text is English-only chrome built in
//! `ui::stats`; everything a user reads as a sentence lives here.

use super::{formatted, plural, text};

const HERO_HOURS_ONE: &str = N_!("1 hour");
const HERO_HOURS: &str = N_!("{count} hours");
const HERO_MINUTES_ONE: &str = N_!("1 minute");
const HERO_MINUTES: &str = N_!("{count} minutes");
const COMPARISON_UP: &str = N_!("\u{25b2} {percent}% vs {period}");
const COMPARISON_DOWN: &str = N_!("\u{25bc} {percent}% vs {period}");
const PREVIOUS_DAYS: &str = N_!("previous {count} days");
const SAME_PERIOD_YEAR: &str = N_!("same period {year}");
const SPELLINGS_MERGED_ONE: &str = N_!("1 spelling merged \u{2014} unify it in the tag editor?");
const SPELLINGS_MERGED: &str =
    N_!("{count} spellings merged \u{2014} unify them in the tag editor?");
const MIX_FROM_GENRE: &str = N_!("Mix from {genre} \u{00b7} Create");

pub const MIX_FROM_TOP_GENRE: &str = N_!("Mix from your top genre \u{00b7} Create");
pub const STATS_EMPTY: &str = N_!("Start listening to see your stats");
pub const STATS_UNAVAILABLE: &str = N_!("Your stats could not be read");
pub const STATS_UNAVAILABLE_DESCRIPTION: &str =
    N_!("Reading the listening history failed. Nothing is missing from it — this view just could not load it.");

/// The hero figure, rounded to whole hours as the editorial layout calls for.
/// Below an hour it names minutes rather than claiming "0 hours".
pub fn hero_listening_time(milliseconds: i64) -> String {
    let minutes = milliseconds.max(0) / 60_000;
    if minutes < 60 {
        return plural(
            HERO_MINUTES_ONE,
            HERO_MINUTES,
            minutes as usize,
            &[("count", &minutes.to_string())],
        );
    }
    let hours = minutes / 60;
    plural(
        HERO_HOURS_ONE,
        HERO_HOURS,
        hours as usize,
        &[("count", &hours.to_string())],
    )
}

/// The comparison pill. `period` names the span that was compared against —
/// never a bare "previous period".
pub fn comparison_pill(percent: i64, period: &str) -> String {
    let message = if percent >= 0 {
        COMPARISON_UP
    } else {
        COMPARISON_DOWN
    };
    formatted(
        message,
        &[("percent", &percent.abs().to_string()), ("period", period)],
    )
}

/// Name for the compared span of a rolling window: the equally long stretch
/// immediately before the selected one.
pub fn previous_days(days: i64) -> String {
    formatted(PREVIOUS_DAYS, &[("count", &days.to_string())])
}

/// Name for the compared span of a year to date: the same calendar stretch of
/// the previous year, which is what makes it seasonally comparable. "2026 so
/// far" reads "vs same period 2025" — Jan–Jul against Jan–Jul.
pub fn same_period_year(year: i32) -> String {
    formatted(SAME_PERIOD_YEAR, &[("year", &year.to_string())])
}

pub fn spellings_merged_hint(count: usize) -> String {
    plural(
        SPELLINGS_MERGED_ONE,
        SPELLINGS_MERGED,
        count,
        &[("count", &count.to_string())],
    )
}

pub fn mix_from_genre(genre: &str) -> String {
    formatted(MIX_FROM_GENRE, &[("genre", genre)])
}

pub fn mix_from_top_genre() -> String {
    text(MIX_FROM_TOP_GENRE)
}

pub fn stats_empty_title() -> String {
    text(STATS_EMPTY)
}

pub fn stats_unavailable_title() -> String {
    text(STATS_UNAVAILABLE)
}

pub fn stats_unavailable_description() -> String {
    text(STATS_UNAVAILABLE_DESCRIPTION)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hero_time_names_hours_and_falls_back_to_minutes() {
        assert_eq!(hero_listening_time(245_000_000), "68 hours");
        assert_eq!(hero_listening_time(3_600_000), "1 hour");
        assert_eq!(hero_listening_time(2_400_000), "40 minutes");
    }

    #[test]
    fn comparison_pill_names_the_compared_span() {
        assert_eq!(comparison_pill(12, "2025"), "\u{25b2} 12% vs 2025");
        assert_eq!(
            comparison_pill(12, &same_period_year(2025)),
            "\u{25b2} 12% vs same period 2025"
        );
        assert_eq!(
            comparison_pill(-8, &previous_days(30)),
            "\u{25bc} 8% vs previous 30 days"
        );
    }

    #[test]
    fn spelling_hint_counts_the_merged_variants() {
        assert_eq!(
            spellings_merged_hint(3),
            "3 spellings merged \u{2014} unify them in the tag editor?"
        );
    }
}
