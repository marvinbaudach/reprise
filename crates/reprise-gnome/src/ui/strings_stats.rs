//! My Stats copy: hero figures, the tag-spelling hint, the mix CTA and the
//! empty/failure states. The rest of the page's text is English-only chrome
//! built in `ui::stats`; everything a user reads as a sentence lives here.

use super::{formatted, plural, text};
use reprise_core::library::stats_period::{StatsPeriod, ROLLING_WINDOW_DAYS};
use reprise_core::library::stats_snapshot::{
    ComparisonDirection, ComparisonFactor, ComparisonPresentation,
};

const HERO_HOURS_ONE: &str = N_!("1 hour");
const HERO_HOURS: &str = N_!("{count} hours");
const HERO_MINUTES_ONE: &str = N_!("1 minute");
const HERO_MINUTES: &str = N_!("{count} minutes");
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

pub const STATS_AUDIO_INTENSITY: &str = N_!("Your listening leaned toward higher intensity");
pub const STATS_AUDIO_BRIGHTNESS: &str = N_!("Your listening leaned toward higher brightness");
pub const STATS_AUDIO_DYNAMICITY: &str = N_!("Your listening leaned toward higher dynamicity");
pub const STATS_AUDIO_RHYTHMICITY: &str = N_!("Your listening leaned toward higher rhythmicity");
const STATS_AUDIO_EVIDENCE: &str = N_!("Based on {count} analyzed plays · {coverage}% coverage");

pub fn stats_audio_evidence(count: i64, coverage: i64) -> String {
    formatted(
        STATS_AUDIO_EVIDENCE,
        &[
            ("count", &count.to_string()),
            ("coverage", &coverage.to_string()),
        ],
    )
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
    fn stats_1a_comparison_copy_renders_every_presentation_without_decimal_noise() {
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
