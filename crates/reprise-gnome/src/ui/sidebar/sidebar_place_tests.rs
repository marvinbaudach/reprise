use super::{place_for_content_page, SidebarPlace};

#[test]
fn nav_18_only_the_two_placeless_pages_leave_the_source_marking() {
    assert_eq!(
        place_for_content_page(Some("library"), None),
        SidebarPlace::Source
    );
    assert_eq!(
        place_for_content_page(Some("stats"), None),
        SidebarPlace::Source
    );
    assert_eq!(
        place_for_content_page(Some("podcasts"), None),
        SidebarPlace::Source
    );
    assert_eq!(place_for_content_page(None, None), SidebarPlace::Source);
    assert_eq!(
        place_for_content_page(Some("library-doctor"), None),
        SidebarPlace::LibraryDoctor
    );
    assert_eq!(
        place_for_content_page(Some("device-sync"), Some("pixel")),
        SidebarPlace::Device("pixel".to_string())
    );
    assert_eq!(
        place_for_content_page(Some("device-sync"), None),
        SidebarPlace::Unknown
    );
}
