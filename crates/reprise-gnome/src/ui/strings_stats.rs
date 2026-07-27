//! My Stats copy: hero figures, the tag-spelling hint, the mix CTA and the
//! empty/failure states. The rest of the page's text is English-only chrome
//! built in `ui::stats`; everything a user reads as a sentence lives here.

use chrono::{Datelike, NaiveDate};
use reprise_core::format::format_thousands;

use super::{formatted, plural, text};
use reprise_core::library::stats_period::{StatsPeriod, ROLLING_WINDOW_DAYS};
use reprise_core::library::stats_snapshot::{
    ComparisonDirection, ComparisonFactor, ComparisonPresentation,
};

const STATS_DURATION_HOURS_MINUTES: &str = N_!("{hours} h {minutes}");
const STATS_DURATION_HOURS: &str = N_!("{hours} h");
const STATS_DURATION_MINUTES: &str = N_!("{minutes} min");
const STATS_HERO_SUBLINE: &str = N_!("{plays} plays · {artists} artists");
const STATS_PACE: &str = N_!("PACE FOR {year}");
const STATS_BEST_WEEK: &str = N_!("{date} · {time}");
const STATS_TREND_DELTA: &str = N_!("{sign}{time}");
const STATS_VS_YEAR: &str = N_!("vs {year}");
const STATS_VS_PREVIOUS_DAYS: &str = N_!("vs previous {count} days");
const COMPARISON_UP: &str = N_!("\u{25b2} {percent}% vs {period}");
const COMPARISON_DOWN: &str = N_!("\u{25bc} {percent}% vs {period}");
const COMPARISON_FACTOR_UP: &str = N_!("\u{25b2} \u{00d7}{factor} vs {period}");
const COMPARISON_FACTOR_DOWN: &str = N_!("\u{25bc} \u{00d7}{factor} vs {period}");
const COMPARISON_FACTOR_DECIMAL: &str = N_!("{whole}.{fraction}");
const COMPARISON_FACTOR_LESS_THAN: &str = N_!("<{factor}");
const PREVIOUS_DAYS: &str = N_!("previous {count} days");
const SAME_PERIOD_YEAR: &str = N_!("same period {year}");
const NEW_THIS_YEAR: &str = N_!("New this year");
const NEW_IN_YEAR: &str = N_!("New in {year}");
const NEW_IN_LAST_DAYS: &str = N_!("New in the last {count} days");
const NEAR_ZERO_SAME_PERIOD: &str = N_!("Less than one minute in the same period {year}");
const NEAR_ZERO_YEAR: &str = N_!("Less than one minute in {year}");
const NEAR_ZERO_PREVIOUS_DAYS: &str = N_!("Less than one minute in the previous {count} days");
const SPELLINGS_MERGED_ONE: &str = N_!("1 spelling merged \u{2014} unify it in the tag editor?");
const SPELLINGS_MERGED: &str =
    N_!("{count} spellings merged \u{2014} unify them in the tag editor?");
pub const STATS_EMPTY: &str = N_!("Start listening to see your stats");
pub const STATS_UNAVAILABLE: &str = N_!("Your stats could not be read");
pub const STATS_UNAVAILABLE_DESCRIPTION: &str =
    N_!("Reading the listening history failed. Nothing is missing from it — this view just could not load it.");
pub const STATS_THIN_HISTORY: &str = N_!("Keep listening — stats grow with you");

/// The single compact duration presentation used throughout My Stats.
pub fn stats_duration(milliseconds: i64) -> String {
    let minutes = milliseconds.max(0) / 60_000;
    if minutes < 60 {
        return formatted(STATS_DURATION_MINUTES, &[("minutes", &minutes.to_string())]);
    }
    let hours = minutes / 60;
    let remaining_minutes = minutes % 60;
    if remaining_minutes == 0 {
        return formatted(STATS_DURATION_HOURS, &[("hours", &hours.to_string())]);
    }
    formatted(
        STATS_DURATION_HOURS_MINUTES,
        &[
            ("hours", &hours.to_string()),
            ("minutes", &remaining_minutes.to_string()),
        ],
    )
}

pub fn stats_hero_subline(plays: i64, artists: i64) -> String {
    formatted(
        STATS_HERO_SUBLINE,
        &[
            ("plays", &format_thousands(plays)),
            ("artists", &format_thousands(artists)),
        ],
    )
}

pub fn stats_per_day(milliseconds: i64) -> String {
    stats_duration(milliseconds)
}

pub fn stats_pace_label(year: i32) -> String {
    formatted(STATS_PACE, &[("year", &year.to_string())])
}

pub fn stats_best_week(start: NaiveDate, milliseconds: i64) -> String {
    formatted(
        STATS_BEST_WEEK,
        &[
            ("date", &format!("{} {}", start.format("%b"), start.day())),
            ("time", &stats_duration(milliseconds)),
        ],
    )
}

pub fn stats_trend_delta(milliseconds: i64) -> String {
    formatted(
        STATS_TREND_DELTA,
        &[
            ("sign", if milliseconds >= 0 { "+" } else { "−" }),
            ("time", &stats_duration(milliseconds.saturating_abs())),
        ],
    )
}

pub fn stats_trend_reference(period: StatsPeriod) -> Option<String> {
    match period {
        StatsPeriod::YearToDate(year) | StatsPeriod::Year(year) => Some(formatted(
            STATS_VS_YEAR,
            &[("year", &year.saturating_sub(1).to_string())],
        )),
        StatsPeriod::Last30Days => Some(formatted(
            STATS_VS_PREVIOUS_DAYS,
            &[("count", &ROLLING_WINDOW_DAYS.to_string())],
        )),
        StatsPeriod::AllTime => None,
    }
}

pub fn stats_new_badge() -> String {
    text(NEW_THIS_YEAR)
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ComparisonCopy {
    pub pill: String,
    pub tooltip: String,
}

/// Localizes the presentation selected by core. The pill uses the shortest
/// truthful period reference; the tooltip restores the full seasonal wording.
pub fn comparison_copy(
    presentation: ComparisonPresentation,
    period: StatsPeriod,
) -> Option<ComparisonCopy> {
    let period_copy = compared_period_copy(period)?;
    if presentation == ComparisonPresentation::New {
        return Some(ComparisonCopy {
            pill: period_copy.new_pill,
            tooltip: period_copy.new_tooltip,
        });
    }
    Some(ComparisonCopy {
        pill: comparison_value(presentation, &period_copy.short),
        tooltip: comparison_value(presentation, &period_copy.full),
    })
}

fn comparison_value(presentation: ComparisonPresentation, period: &str) -> String {
    match presentation {
        ComparisonPresentation::Percentage(percent) => comparison_pill(percent, period),
        ComparisonPresentation::Factor { direction, value } => {
            let message = match direction {
                ComparisonDirection::Up => COMPARISON_FACTOR_UP,
                ComparisonDirection::Down => COMPARISON_FACTOR_DOWN,
            };
            formatted(
                message,
                &[("factor", &comparison_factor(value)), ("period", period)],
            )
        }
        ComparisonPresentation::New => unreachable!("new comparisons use period-specific copy"),
    }
}

fn comparison_factor(value: ComparisonFactor) -> String {
    match value {
        ComparisonFactor::Whole(whole) => whole.to_string(),
        ComparisonFactor::Decimal { whole, tenth } => formatted(
            COMPARISON_FACTOR_DECIMAL,
            &[
                ("whole", &whole.to_string()),
                ("fraction", &tenth.to_string()),
            ],
        ),
        ComparisonFactor::LessThanOneTenth => formatted(
            COMPARISON_FACTOR_LESS_THAN,
            &[(
                "factor",
                &formatted(
                    COMPARISON_FACTOR_DECIMAL,
                    &[("whole", "0"), ("fraction", "1")],
                ),
            )],
        ),
    }
}

struct ComparedPeriodCopy {
    short: String,
    full: String,
    new_pill: String,
    new_tooltip: String,
}

fn compared_period_copy(period: StatsPeriod) -> Option<ComparedPeriodCopy> {
    match period {
        StatsPeriod::YearToDate(year) => {
            let previous_year = year.saturating_sub(1);
            Some(ComparedPeriodCopy {
                short: previous_year.to_string(),
                full: same_period_year(previous_year),
                new_pill: text(NEW_THIS_YEAR),
                new_tooltip: formatted(
                    NEAR_ZERO_SAME_PERIOD,
                    &[("year", &previous_year.to_string())],
                ),
            })
        }
        StatsPeriod::Year(year) => {
            let previous_year = year.saturating_sub(1);
            Some(ComparedPeriodCopy {
                short: previous_year.to_string(),
                full: previous_year.to_string(),
                new_pill: formatted(NEW_IN_YEAR, &[("year", &year.to_string())]),
                new_tooltip: formatted(NEAR_ZERO_YEAR, &[("year", &previous_year.to_string())]),
            })
        }
        StatsPeriod::Last30Days => Some(ComparedPeriodCopy {
            short: previous_days(ROLLING_WINDOW_DAYS),
            full: previous_days(ROLLING_WINDOW_DAYS),
            new_pill: formatted(
                NEW_IN_LAST_DAYS,
                &[("count", &ROLLING_WINDOW_DAYS.to_string())],
            ),
            new_tooltip: formatted(
                NEAR_ZERO_PREVIOUS_DAYS,
                &[("count", &ROLLING_WINDOW_DAYS.to_string())],
            ),
        }),
        StatsPeriod::AllTime => None,
    }
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

pub fn stats_empty_title() -> String {
    text(STATS_EMPTY)
}

pub fn stats_unavailable_title() -> String {
    text(STATS_UNAVAILABLE)
}

pub fn stats_unavailable_description() -> String {
    text(STATS_UNAVAILABLE_DESCRIPTION)
}

pub fn stats_thin_history_hint() -> String {
    text(STATS_THIN_HISTORY)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stats_duration_uses_one_compact_page_wide_format() {
        assert_eq!(stats_duration(25_080_000), "6 h 58");
        assert_eq!(stats_duration(3_600_000), "1 h");
        assert_eq!(stats_duration(2_400_000), "40 min");
        assert_eq!(stats_per_day(180_000), "3 min");
        assert_eq!(stats_trend_delta(-25_080_000), "−6 h 58");
        assert_eq!(stats_trend_delta(-2_400_000), "−40 min");
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
    fn stats_11a_comparison_copy_renders_every_presentation_without_decimal_noise() {
        let copy =
            |presentation| comparison_copy(presentation, StatsPeriod::YearToDate(2026)).unwrap();

        let normal = copy(ComparisonPresentation::Percentage(12));
        assert_eq!(normal.pill, "\u{25b2} 12% vs 2025");
        assert_eq!(normal.tooltip, "\u{25b2} 12% vs same period 2025");

        let integer_factor = copy(ComparisonPresentation::Factor {
            direction: ComparisonDirection::Up,
            value: ComparisonFactor::Whole(11),
        });
        assert_eq!(integer_factor.pill, "\u{25b2} \u{00d7}11 vs 2025");
        assert!(!integer_factor.pill.contains("11.0"));

        let decimal_factor = copy(ComparisonPresentation::Factor {
            direction: ComparisonDirection::Up,
            value: ComparisonFactor::Decimal {
                whole: 11,
                tenth: 5,
            },
        });
        assert_eq!(decimal_factor.pill, "\u{25b2} \u{00d7}11.5 vs 2025");

        let decline = copy(ComparisonPresentation::Factor {
            direction: ComparisonDirection::Down,
            value: ComparisonFactor::Decimal { whole: 0, tenth: 3 },
        });
        assert_eq!(decline.pill, "\u{25bc} \u{00d7}0.3 vs 2025");

        let extreme_decline = copy(ComparisonPresentation::Factor {
            direction: ComparisonDirection::Down,
            value: ComparisonFactor::LessThanOneTenth,
        });
        assert_eq!(extreme_decline.pill, "\u{25bc} \u{00d7}<0.1 vs 2025");

        let new = copy(ComparisonPresentation::New);
        assert_eq!(new.pill, "New this year");
        assert_eq!(new.tooltip, "Less than one minute in the same period 2025");
    }

    #[test]
    fn spelling_hint_counts_the_merged_variants() {
        assert_eq!(
            spellings_merged_hint(3),
            "3 spellings merged \u{2014} unify them in the tag editor?"
        );
    }
}
