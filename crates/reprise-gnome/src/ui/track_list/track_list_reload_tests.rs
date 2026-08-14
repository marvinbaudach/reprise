use std::cell::RefCell;

use reprise_core::view_source::ViewSource;

use super::{filter_change_viewport, source_snapshot, viewport_after_clearing, ReloadViewport};

#[test]
fn startup_reload_requests_are_served_once_when_startup_finishes() {
    let load = super::super::startup_load::StartupLoad::deferred();

    assert!(!load.request());
    assert!(!load.request());
    assert!(load.finish());
    assert!(!load.finish());
    assert!(load.request());
}

#[test]
fn source_snapshot_releases_the_borrow_before_reentrant_work() {
    let source = RefCell::new(ViewSource::Library);

    let snapshot = source_snapshot(&source);
    *source.borrow_mut() = ViewSource::Queue;

    assert!(matches!(snapshot, ViewSource::Library));
    assert!(matches!(*source.borrow(), ViewSource::Queue));
}

/// SEARCH-9: three outcomes, decided solely by whether the *new* query is
/// empty. Adding a character, deleting one and replacing the text are the
/// same case — the result set is new either way, so the eye belongs at its
/// top. Only emptying the query goes back to where the user came from.
#[test]
fn search_9_filter_change_decides_viewport_by_the_new_query() {
    assert!(matches!(
        filter_change_viewport("", "Match", false),
        ReloadViewport::Top
    ));
    assert!(matches!(
        filter_change_viewport("Mat", "Match", false),
        ReloadViewport::Top
    ));
    assert!(matches!(
        filter_change_viewport("Match", "Mat", false),
        ReloadViewport::Top
    ));
    assert!(matches!(
        filter_change_viewport("Match", "", false),
        ReloadViewport::RestorePreSearch
    ));
    assert!(matches!(
        filter_change_viewport("Match", "Match", false),
        ReloadViewport::PreserveAnchor
    ));
}

#[test]
fn search_16_clearing_chooses_its_viewport_from_the_search_that_ran() {
    assert!(matches!(
        viewport_after_clearing(true, true),
        ReloadViewport::CenterPlayingElsePreSearch
    ));
    assert!(matches!(
        viewport_after_clearing(true, false),
        ReloadViewport::RestorePreSearch
    ));
    assert!(matches!(
        viewport_after_clearing(false, true),
        ReloadViewport::CenterPlayingTrack
    ));
    assert!(matches!(
        viewport_after_clearing(false, false),
        ReloadViewport::CenterPlayingTrack
    ));
}
