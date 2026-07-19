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
