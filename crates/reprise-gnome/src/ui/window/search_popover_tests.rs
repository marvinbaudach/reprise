use gtk4::prelude::*;
use reprise_view::search_scope::SearchScope;

use super::SearchPopover;

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn popover_owns_the_search_entry_and_scope_caption() {
    gtk4::init().unwrap();
    let lens = gtk4::ToggleButton::new();
    let entry = gtk4::SearchEntry::new();
    let search = SearchPopover::new(&lens, &entry);

    assert_eq!(search.widget().position(), gtk4::PositionType::Bottom);
    assert_eq!(search.widget().halign(), gtk4::Align::End);
    assert!(!search.widget().has_arrow());
    assert!(entry.is_ancestor(search.widget()));

    search.set_scope(SearchScope::Podcasts);
    assert_eq!(search.scope_text(), "Searches episode titles");
}
