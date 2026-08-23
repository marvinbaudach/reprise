use super::*;

#[test]
fn detail_views_are_supported_smoke_sources() {
    assert_eq!(parse_smoke_source("my_stats"), Some(ViewSource::MyStats));
    assert_eq!(parse_smoke_source("concerts"), Some(ViewSource::Concerts));
    assert_eq!(parse_smoke_source("releases"), Some(ViewSource::Releases));
    assert_eq!(parse_smoke_source("podcasts"), Some(ViewSource::Podcasts));
    assert_eq!(parse_smoke_source("youtube"), Some(ViewSource::Youtube));
    assert_eq!(parse_smoke_source("radio"), Some(ViewSource::Radio));
}

#[test]
fn reload_oracle_has_parseable_labels_and_exact_hundred_row_filter() {
    assert_eq!(ORACLE_FILTER, "Artist 0000");
    assert_eq!(OracleTransition::SourceSwitch.label(), "source-switch");
    assert_eq!(OracleTransition::SortChange.label(), "sort-change");
    assert_eq!(OracleTransition::ClearedSearch.label(), "cleared-search");
}

#[test]
fn reload_oracle_seed_plan_is_bounded_and_ratio_is_explicit() {
    assert_eq!(parse_oracle_rows("1"), Ok(100_000));
    assert_eq!(parse_oracle_rows("rows:10000"), Ok(10_000));
    assert!(parse_oracle_rows("rows:0").is_err());
    assert!(parse_oracle_rows("rows:100001").is_err());
    assert_eq!(oracle_ratio(&[50_000, 420_000, 418_000]), Some(8.4));
}

#[test]
fn reload_oracle_rejects_a_wrong_restored_search_count() {
    assert_eq!(
        validate_oracle_count(OracleCountGuard::RestoredSearch, 99_999, 100_000),
        Err(OracleFailure::UnexpectedRestoredCount {
            rows: 99_999,
            expected: 100_000,
        })
    );
}

#[test]
fn reload_oracle_rejects_a_sort_that_changes_the_row_count() {
    assert_eq!(
        validate_oracle_count(OracleCountGuard::SortChange, 99_999, 100_000),
        Err(OracleFailure::UnexpectedSortCount {
            rows: 99_999,
            expected: 100_000,
        })
    );
}

#[test]
fn reload_oracle_rejects_a_transition_without_a_complete_measurement() {
    let entries = [
        "1ms ReloadBreakdown reload_id=8 whole_us=10".to_string(),
        "2ms Reload reload_id=8 next_frame_us=broken".to_string(),
    ];

    assert_eq!(
        oracle_measurement(&entries, OracleTransition::SortChange, 7),
        Err(OracleFailure::MissingMeasurement {
            transition: "sort-change",
        })
    );
}

#[test]
fn reload_oracle_rejects_an_incomplete_sample_set() {
    assert_eq!(
        complete_oracle_samples(&[10, 20]),
        Err(OracleFailure::IncompleteSamples { count: 2 })
    );
}

#[test]
fn reload_oracle_rejects_a_stale_trail_entry() {
    let entries = [
        "1ms ReloadBreakdown reload_id=7 whole_us=10".to_string(),
        "2ms Reload reload_id=7 next_frame_us=20".to_string(),
    ];

    assert_eq!(
        oracle_measurement(&entries, OracleTransition::ClearedSearch, 7),
        Err(OracleFailure::StaleTrailEntry {
            transition: "cleared-search",
        })
    );
}

#[test]
fn reload_oracle_pairs_event_and_breakdown_by_reload_id() {
    let entries = [
        "1ms ReloadBreakdown reload_id=8 whole_us=10".to_string(),
        "2ms Reload reload_id=8 next_frame_us=20".to_string(),
        "3ms ReloadBreakdown reload_id=9 whole_us=11".to_string(),
        "4ms Reload reload_id=9 next_frame_us=21".to_string(),
        "5ms ReloadBreakdown reload_id=10 whole_us=12".to_string(),
    ];

    let measurement = oracle_measurement(&entries, OracleTransition::SourceSwitch, 8).unwrap();
    assert_eq!(measurement.reload_id, 9);
    assert!(measurement.event.contains("reload_id=9"));
    assert!(measurement.breakdown.contains("reload_id=9"));
    assert_eq!(measurement.next_frame_us, 21);
}

#[test]
fn reload_oracle_failure_lines_are_machine_parseable() {
    assert_eq!(
        oracle_failure_line(
            &OracleFailure::UnexpectedRestoredCount {
                rows: 9,
                expected: 10,
            },
            "1.25",
        ),
        "REPRISE_RELOAD_ORACLE error=unexpected-restored-count rows=9 expected=10 loadavg=1.25"
    );
    assert_eq!(
        oracle_failure_line(
            &OracleFailure::UnexpectedSortCount {
                rows: 9,
                expected: 10,
            },
            "ignored",
        ),
        "REPRISE_RELOAD_ORACLE error=unexpected-sort-count rows=9 expected=10"
    );
    assert_eq!(
        oracle_failure_line(
            &OracleFailure::MissingMeasurement {
                transition: "sort-change",
            },
            "ignored",
        ),
        "REPRISE_RELOAD_ORACLE error=missing-measurement transition=sort-change"
    );
    assert_eq!(
        oracle_failure_line(
            &OracleFailure::StaleTrailEntry {
                transition: "cleared-search",
            },
            "ignored",
        ),
        "REPRISE_RELOAD_ORACLE error=stale-trail-entry transition=cleared-search"
    );
    assert_eq!(
        oracle_failure_line(&OracleFailure::IncompleteSamples { count: 2 }, "ignored"),
        "REPRISE_RELOAD_ORACLE error=incomplete-samples count=2"
    );
}
