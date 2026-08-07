use std::cell::RefCell;

use reprise_core::view_source::ViewSource;

use super::{filter_change_viewport, source_snapshot, ReloadViewport};

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
        filter_change_viewport("", "Match"),
        ReloadViewport::Top
    ));
    assert!(matches!(
        filter_change_viewport("Mat", "Match"),
        ReloadViewport::Top
    ));
    assert!(matches!(
        filter_change_viewport("Match", "Mat"),
        ReloadViewport::Top
    ));
    assert!(matches!(
        filter_change_viewport("Match", ""),
        ReloadViewport::RestorePreSearch
    ));
    assert!(matches!(
        filter_change_viewport("Match", "Match"),
        ReloadViewport::PreserveAnchor
    ));
}
