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
