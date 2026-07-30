use chrono::{TimeZone, Utc};
use rusqlite::params;

use super::{compute, ComparisonDirection, ComparisonFactor, ComparisonPresentation};
use crate::library::stats_period::StatsPeriod;

const NOW_2026_07_19: i64 = 1_784_424_000;

/// STATS-11a: ordinary changes retain the familiar percentage form. The
/// presentation is core-owned so every frontend receives the same decision.
#[test]
fn stats_11a_percentage_below_the_threshold_stays_a_percentage() {
    let snapshot = comparison_snapshot(1_099_000, Some(100_000));

    assert_eq!(
        snapshot.hero.comparison_presentation,
        Some(ComparisonPresentation::Percentage(999))
    );
}

/// STATS-11a: +1000% is the first factor-form value. Core decides whether the
/// rounded factor is whole or decimal; only its punctuation remains localized.
#[test]
fn stats_11a_at_and_above_the_threshold_becomes_a_correctly_rounded_factor() {
    for (current_ms, expected) in [
        (1_100_000, ComparisonFactor::Whole(11)),
        (
            1_154_000,
            ComparisonFactor::Decimal {
                whole: 11,
                tenth: 5,
            },
        ),
    ] {
        let snapshot = comparison_snapshot(current_ms, Some(100_000));

        assert_eq!(
            snapshot.hero.comparison_presentation,
            Some(ComparisonPresentation::Factor {
                direction: ComparisonDirection::Up,
                value: expected,
            })
        );
    }
}

/// STATS-11a: a strong decline uses the same multiplicative vocabulary as a
/// strong rise while retaining the established downward direction marker.
#[test]
fn stats_11a_strong_decline_uses_the_symmetric_factor_form() {
    let snapshot = comparison_snapshot(30_000, Some(100_000));

    assert_eq!(
        snapshot.hero.comparison_presentation,
        Some(ComparisonPresentation::Factor {
            direction: ComparisonDirection::Down,
            value: ComparisonFactor::Decimal { whole: 0, tenth: 3 },
        })
    );
}

/// STATS-11a: a nonzero decline must never round to the false factor `×0`.
#[test]
fn stats_11a_extreme_nonzero_decline_stays_below_one_tenth() {
    let snapshot = comparison_snapshot(1_000, Some(100_000));

    assert_eq!(
        snapshot.hero.comparison_presentation,
        Some(ComparisonPresentation::Factor {
            direction: ComparisonDirection::Down,
            value: ComparisonFactor::LessThanOneTenth,
        })
    );
}

/// STATS-11a: a baseline below the product's one-minute display granularity is
/// qualitative data, not a denominator for an explosive numeric comparison.
#[test]
fn stats_11a_zero_or_near_zero_baseline_uses_neither_percent_nor_factor() {
    for previous_ms in [None, Some(59_999)] {
        let snapshot = comparison_snapshot(3_600_000, previous_ms);

        assert_eq!(
            snapshot.hero.comparison_presentation,
            Some(ComparisonPresentation::New)
        );
        assert_eq!(snapshot.hero.comparison_percent, None);
    }
}

fn comparison_snapshot(current_ms: i64, previous_ms: Option<i64>) -> super::StatsSnapshot {
    let conn = migrated_conn();
    conn.conn()
        .execute(
            "INSERT INTO tracks (id, path, title, artist, album, duration_ms, added_at) \
         VALUES (1, '/music/track.flac', 'Track', 'Artist', 'Album', 4000000, 0)",
            [],
        )
        .unwrap();
    if let Some(previous_ms) = previous_ms {
        insert_event(&conn, timestamp(2025, 3, 1), previous_ms);
    }
    insert_event(&conn, timestamp(2026, 3, 1), current_ms);
    compute(&conn, StatsPeriod::YearToDate(2026), NOW_2026_07_19, &Utc).unwrap()
}

fn migrated_conn() -> crate::db::Db {
    crate::db::Db::open_in_memory().unwrap()
}

fn insert_event(conn: &crate::db::Db, played_at: i64, ms_played: i64) {
    conn.conn()
        .execute(
            "INSERT INTO listen_events (track_id, played_at, ms_played) VALUES (1, ?1, ?2)",
            params![played_at, ms_played],
        )
        .unwrap();
}

fn timestamp(year: i32, month: u32, day: u32) -> i64 {
    Utc.with_ymd_and_hms(year, month, day, 12, 0, 0)
        .single()
        .unwrap()
        .timestamp()
}
