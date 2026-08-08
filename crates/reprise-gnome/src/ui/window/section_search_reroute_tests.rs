use gtk4::prelude::*;
use libadwaita as adw;
use reprise_core::view_source::ViewSource;

use super::section_search::SectionSearch;

// UX SEARCH-8a: routing to the exact active source is not a destination
// switch and must not destroy an in-progress query.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn search_8a_rerouting_the_active_source_preserves_its_query() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let window = adw::ApplicationWindow::builder().build();
    let entry = gtk4::SearchEntry::new();
    let search_bar = gtk4::SearchBar::new();
    search_bar.connect_entry(&entry);
    let toggle = gtk4::ToggleButton::new();
    let search = SectionSearch::new(&entry, &search_bar, &toggle, &window);

    search.activate_source(&ViewSource::Library, "Music");
    toggle.set_active(true);
    search_bar.set_search_mode(true);
    entry.set_text("falling");

    search.activate_source(&ViewSource::Library, "Music");

    assert_eq!(entry.text(), "falling");
    assert!(toggle.is_active());
    assert!(search_bar.is_search_mode());
}
