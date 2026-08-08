use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use reprise_core::view_source::ViewSource;
use reprise_view::search_scope::SearchScope;

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

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn search_8a_switching_views_applies_each_empty_query_once() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let window = adw::ApplicationWindow::builder().build();
    let entry = gtk4::SearchEntry::new();
    let search_bar = gtk4::SearchBar::new();
    search_bar.connect_entry(&entry);
    let toggle = gtk4::ToggleButton::new();
    let search = SectionSearch::new(&entry, &search_bar, &toggle, &window);
    let applied = Rc::new(RefCell::new(Vec::new()));
    for scope in [SearchScope::Tracks, SearchScope::Podcasts] {
        let applied = applied.clone();
        search.register(
            scope,
            move |query| applied.borrow_mut().push((scope, query.to_owned())),
            || {},
        );
    }

    entry.set_text("falling");
    wait_for_search_signal();
    applied.borrow_mut().clear();
    search.activate(SearchScope::Podcasts, "Podcasts");
    wait_for_search_signal();

    let applied = applied.borrow();
    assert_eq!(
        applied
            .iter()
            .filter(|event| **event == (SearchScope::Tracks, String::new()))
            .count(),
        1,
        "the outgoing filter must be cleared once"
    );
    assert_eq!(
        applied
            .iter()
            .filter(|event| **event == (SearchScope::Podcasts, String::new()))
            .count(),
        1,
        "the destination filter must be cleared once"
    );
}

fn wait_for_search_signal() {
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(250);
    while std::time::Instant::now() < deadline {
        while gtk4::glib::MainContext::default().iteration(false) {}
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}
