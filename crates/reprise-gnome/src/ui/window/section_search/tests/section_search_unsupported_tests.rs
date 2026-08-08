use super::*;

// UX SEARCH-8a: where there is no list, the lens is insensitive, says why,
// and the bar cannot be revealed.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn search_8a_sections_without_a_list_offer_no_search() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let harness = harness();

    harness.search.activate(SearchScope::Tracks, "Music");
    assert!(harness.toggle.is_sensitive());

    harness
        .search
        .activate(SearchScope::Unsupported, "My Stats");

    assert!(!harness.toggle.is_sensitive());
    assert_eq!(
        harness.toggle.tooltip_text().as_deref(),
        Some("Nothing to filter in My Stats")
    );
    assert!(!harness.search_bar.is_search_mode());
    assert!(!harness.search.supports_search());
    assert!(harness.search_bar.key_capture_widget().is_none());
}
