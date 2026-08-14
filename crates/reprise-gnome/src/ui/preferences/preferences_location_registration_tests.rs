use super::{page_index_by_name, PageId, PAGE_ORDER};

#[test]
fn location_page_sits_between_library_and_plugins() {
    assert_eq!(
        PAGE_ORDER,
        [
            PageId::Playback,
            PageId::Appearance,
            PageId::Layout,
            PageId::Library,
            PageId::Location,
            PageId::Plugins,
        ]
    );
    assert_eq!(page_index_by_name("location"), Some(4));
    assert_eq!(page_index_by_name("plugins"), Some(5));
    for retired in ["online_sources", "new_releases", "concerts"] {
        assert_eq!(page_index_by_name(retired), None);
    }
}
