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
