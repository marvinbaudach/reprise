use super::*;

#[test]
fn keeps_requested_source_when_its_row_still_exists() {
    let (source, fell_back) = resolve_select_source(ViewSource::Playlist(3), true);
    assert_eq!(source, ViewSource::Playlist(3));
    assert!(!fell_back);
}

#[test]
fn falls_back_to_library_when_requested_row_is_gone() {
    let (source, fell_back) = resolve_select_source(ViewSource::Missing, false);
    assert_eq!(source, ViewSource::Library);
    assert!(fell_back);
}

#[test]
fn falls_back_to_library_when_a_smart_list_vanished() {
    let (source, fell_back) = resolve_select_source(ViewSource::Smart(7), false);
    assert_eq!(source, ViewSource::Library);
    assert!(fell_back);
}

#[test]
fn restored_source_reuses_the_vanished_source_fallback() {
    assert_eq!(
        resolve_select_source(ViewSource::Playlist(99), false).0,
        ViewSource::Library
    );
    assert_eq!(
        resolve_select_source(ViewSource::Queue, true).0,
        ViewSource::Queue
    );
}
