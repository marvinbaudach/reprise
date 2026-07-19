use chrono::{TimeZone, Utc};

use super::{Granularity, StatsPeriod};

const NOW_2026_07_19: i64 = 1_784_424_000;

#[test]
fn stats_1_ribbon_axis_matches_period() {
    let year_to_date = StatsPeriod::YearToDate(2026).resolve(
        NOW_2026_07_19,
        &Utc,
        Some(timestamp(2026, 1, 2, 12, 0)),
    );
    assert_eq!(year_to_date.granularity, Granularity::Month);
    assert_eq!(
        year_to_date
            .buckets
            .iter()
            .map(|bucket| bucket.label.as_str())
            .collect::<Vec<_>>(),
        ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul"]
    );
    assert!(year_to_date.buckets.last().unwrap().open);
    assert!(year_to_date.buckets[..6].iter().all(|bucket| !bucket.open));

    let previous_year =
        StatsPeriod::Year(2025).resolve(NOW_2026_07_19, &Utc, Some(timestamp(2025, 1, 1, 0, 0)));
    assert_eq!(previous_year.buckets.len(), 12);
    assert!(previous_year.buckets.iter().all(|bucket| !bucket.open));

    let last_30_days = StatsPeriod::Last30Days.resolve(NOW_2026_07_19, &Utc, Some(NOW_2026_07_19));
    assert_eq!(last_30_days.granularity, Granularity::Day);
    assert_eq!(last_30_days.buckets.len(), 30);
    assert!(last_30_days.buckets.last().unwrap().open);
    assert_eq!(last_30_days.buckets.last().unwrap().label, "Jul 19");
}

/// `resolve` is public and the module promises never to panic, so a year
/// outside chrono's calendar must resolve to an empty range, not unwind.
#[test]
fn resolve_never_panics_on_an_out_of_calendar_year() {
    for period in [
        StatsPeriod::Year(i32::MAX),
        StatsPeriod::Year(i32::MIN),
        StatsPeriod::YearToDate(i32::MAX),
        StatsPeriod::YearToDate(i32::MIN),
    ] {
        let range = period.resolve(NOW_2026_07_19, &Utc, Some(NOW_2026_07_19));
        assert!(range.buckets.is_empty(), "period: {period:?}");
    }
}

/// STATS-1: the compared span is seasonally congruent, not merely the equally
/// long stretch immediately before. "2026 so far" is measured against Jan–Jul
/// 2025 — comparing it against Jun–Dec 2025 would pit summer against winter,
/// and listening time is seasonal.
#[test]
fn stats_1_year_to_date_compares_the_same_span_of_the_previous_year() {
    let (start, end) = StatsPeriod::YearToDate(2026)
        .previous_range(NOW_2026_07_19, &Utc)
        .expect("a year to date has a comparison span");

    assert_eq!(start, timestamp(2025, 1, 1, 0, 0));
    assert_eq!(end, timestamp(2025, 7, 19, 1, 20) + 1);

    let selected = StatsPeriod::YearToDate(2026).resolve(NOW_2026_07_19, &Utc, None);
    assert!(
        end <= selected.start_unix,
        "the compared span must lie strictly before the selected one"
    );
    assert_ne!(
        start,
        selected.start_unix - (selected.end_unix - selected.start_unix),
        "the compared span must not be the stretch immediately before (Jun-Dec 2025)"
    );
}

/// A selected full calendar year is compared against the whole year before it.
#[test]
fn stats_1_full_year_compares_against_the_whole_previous_year() {
    let (start, end) = StatsPeriod::Year(2025)
        .previous_range(NOW_2026_07_19, &Utc)
        .expect("a full year has a comparison span");

    assert_eq!(start, timestamp(2024, 1, 1, 0, 0));
    assert_eq!(end, timestamp(2025, 1, 1, 0, 0));
}

/// A rolling window has no seasonal counterpart a year back that a reader
/// would recognise, so it keeps the stretch immediately before it.
#[test]
fn stats_1_last_30_days_compares_against_the_30_days_before() {
    let (start, end) = StatsPeriod::Last30Days
        .previous_range(NOW_2026_07_19, &Utc)
        .expect("a rolling window has a comparison span");

    assert_eq!(start, timestamp(2026, 5, 21, 0, 0));
    assert_eq!(end, timestamp(2026, 6, 20, 0, 0));

    let selected = StatsPeriod::Last30Days.resolve(NOW_2026_07_19, &Utc, None);
    assert_eq!(end, selected.start_unix);
}

/// All time has nothing before it to compare against, so the hero pill stays
/// hidden.
#[test]
fn stats_1_all_time_has_no_compared_span() {
    assert_eq!(
        StatsPeriod::AllTime.previous_range(NOW_2026_07_19, &Utc),
        None
    );
}

/// February 29th has no counterpart in a common year. The compared span clamps
/// to the last day the previous February actually has rather than panicking or
/// silently sliding into March.
#[test]
fn stats_1_leap_day_clamps_the_compared_span_to_february() {
    let leap_day = timestamp(2028, 2, 29, 10, 0);
    let (start, end) = StatsPeriod::YearToDate(2028)
        .previous_range(leap_day, &Utc)
        .expect("a leap day still has a comparison span");

    assert_eq!(start, timestamp(2027, 1, 1, 0, 0));
    assert_eq!(end, timestamp(2027, 2, 28, 10, 0) + 1);
}

/// `previous_range` is public alongside `resolve` and inherits its promise
/// never to panic on an untrusted year.
#[test]
fn previous_range_never_panics_on_an_out_of_calendar_year() {
    for period in [
        StatsPeriod::Year(i32::MAX),
        StatsPeriod::Year(i32::MIN),
        StatsPeriod::YearToDate(i32::MAX),
        StatsPeriod::YearToDate(i32::MIN),
    ] {
        assert_eq!(
            period.previous_range(NOW_2026_07_19, &Utc),
            None,
            "period: {period:?}"
        );
    }
}

/// A year to date whose year is already over behaves like that full calendar
/// year, so it is compared against the whole year before it.
#[test]
fn year_to_date_of_a_finished_year_compares_against_that_whole_year() {
    let (start, end) = StatsPeriod::YearToDate(2024)
        .previous_range(NOW_2026_07_19, &Utc)
        .expect("a finished year to date has a comparison span");

    assert_eq!(start, timestamp(2023, 1, 1, 0, 0));
    assert_eq!(end, timestamp(2024, 1, 1, 0, 0));
}

#[test]
fn available_periods_lists_the_fixed_editorial_order() {
    assert_eq!(
        StatsPeriod::available_periods(2026),
        [
            StatsPeriod::YearToDate(2026),
            StatsPeriod::Year(2025),
            StatsPeriod::AllTime,
            StatsPeriod::Last30Days,
        ]
    );
}

fn timestamp(year: i32, month: u32, day: u32, hour: u32, minute: u32) -> i64 {
    Utc.with_ymd_and_hms(year, month, day, hour, minute, 0)
        .single()
        .unwrap()
        .timestamp()
}
