use std::cell::RefCell as StdRefCell;

use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::browser::navigation::{NavigationIntent, SidebarTarget};
use reprise_core::browser::{AlbumKey, ArtistKey, BrowserPlace};

use super::*;
use crate::ui::nav_history::{NavHistory, NavPlace};

#[path = "tests/section_search_unsupported_tests.rs"]
mod unsupported_tests;

struct Harness {
    search: Rc<SectionSearch>,
    entry: gtk4::SearchEntry,
    toggle: gtk4::ToggleButton,
    search_bar: gtk4::SearchBar,
    track_filter_layout: crate::ui::filter_bar_layout::FilterBarLayout,
    applied: Rc<StdRefCell<Vec<(SearchScope, String)>>>,
    facets_cleared: Rc<StdRefCell<Vec<SearchScope>>>,
}

fn harness() -> Harness {
    let window = adw::ApplicationWindow::builder().build();
    let entry = gtk4::SearchEntry::new();
    let search_bar = gtk4::SearchBar::new();
    search_bar.connect_entry(&entry);
    let toggle = gtk4::ToggleButton::new();
    let search = SectionSearch::new(&entry, &search_bar, &toggle, &window);
    let track_filter_layout = crate::ui::filter_bar_layout::FilterBarLayout::new();
    let applied = Rc::new(StdRefCell::new(Vec::new()));
    let facets_cleared = Rc::new(StdRefCell::new(Vec::new()));
    for scope in [
        SearchScope::Tracks,
        SearchScope::Podcasts,
        SearchScope::Radio,
    ] {
        let sink = applied.clone();
        let cleared = facets_cleared.clone();
        let track_filter_layout = track_filter_layout.clone();
        search.register(
            scope,
            move |query| {
                sink.borrow_mut().push((scope, query.to_owned()));
                if scope != SearchScope::Tracks {
                    return;
                }
                track_filter_layout.replace_scoped_search(SearchScope::Tracks, query, || {});
            },
            move || cleared.borrow_mut().push(scope),
        );
    }
    Harness {
        search,
        entry,
        toggle,
        search_bar,
        track_filter_layout,
        applied,
        facets_cleared,
    }
}

fn track_chip_label(harness: &Harness) -> Option<String> {
    harness
        .track_filter_layout
        .slot_child(crate::ui::filter_bar_layout::FilterBarSlot::Search)?
        .downcast::<gtk4::Button>()
        .ok()?
        .label()
        .map(|label| label.to_string())
}

fn settle() {
    while gtk4::glib::MainContext::default().iteration(false) {}
}

/// GTK debounces `search-changed` by ~150 ms, so a typed query reaches
/// its section a moment after the keystroke. Pump until it does rather
/// than asserting into that window.
fn settle_until(label: &str, condition: impl Fn() -> bool) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !condition() {
        settle();
        assert!(std::time::Instant::now() < deadline, "timed out: {label}");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

// UX SEARCH-8a: switching views drops the query and collapses the field,
// because the destination is a new search context.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn search_8a_switching_views_drops_the_query_and_collapses_the_bar() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let harness = harness();

    harness.search.activate(SearchScope::Tracks, "Music");
    harness.toggle.set_active(true);
    harness.search_bar.set_search_mode(true);
    harness.entry.set_text("falling");
    settle();

    harness.search.activate(SearchScope::Podcasts, "Podcasts");
    settle();
    assert_eq!(
        harness.entry.text(),
        "",
        "the Podcasts section starts without the Library query"
    );
    assert!(!harness.toggle.is_active());
    assert!(!harness.search_bar.is_search_mode());

    harness.search.activate(SearchScope::Tracks, "Music");
    settle();
    assert_eq!(
        harness.entry.text(),
        "",
        "returning through a new view switch must not resurrect Music's old query"
    );
}

// UX SEARCH-8a: track sources share one SearchScope, but choosing another
// sidebar destination is still a view switch and starts empty.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn search_8a_switching_track_views_drops_the_query_despite_the_shared_scope() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let harness = harness();

    harness
        .search
        .activate_source(&ViewSource::Library, "Music");
    harness.toggle.set_active(true);
    harness.search_bar.set_search_mode(true);
    harness.entry.set_text("falling");
    settle();

    let history = NavHistory::default();
    let mut library = BrowserPlace::from(ViewSource::Library);
    library.track_state_mut().unwrap().search = "falling".into();
    history.record_route(&NavPlace::browser(library.clone()));
    let destination = history
        .navigate_from(
            NavigationIntent::Sidebar(SidebarTarget::RecentlyAdded),
            library,
        )
        .expect("Recently Added must be a different sidebar destination");
    harness
        .search
        .activate_source(&destination.view_source(), "Recently Added");
    let destination_query = &destination
        .browser_place()
        .track_state()
        .expect("Recently Added is a track view")
        .search;
    harness
        .search
        .set_query(SearchScope::Tracks, destination_query);
    settle();

    assert_eq!(harness.entry.text(), "");
    assert!(!harness.toggle.is_active());
    assert!(!harness.search_bar.is_search_mode());
}

// UX SEARCH-8a/FIL-1c: a metadata intent drills into the Library's filter
// context rather than choosing a new sidebar destination. Its history
// place carries the query into the Artist page, and Back restores the
// complete remembered Library state without search owning parallel
// origin state.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn search_8a_drilling_into_an_artist_place_keeps_query_and_chip_then_back_restores_them() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let harness = harness();
    let history = NavHistory::default();

    harness
        .search
        .activate_source(&ViewSource::Library, "Music");
    harness.entry.set_text("falling");
    let chip_probe = harness.track_filter_layout.clone();
    settle_until("the Library search chip appears", move || {
        chip_probe
            .slot_child(crate::ui::filter_bar_layout::FilterBarSlot::Search)
            .is_some()
    });

    let mut library = BrowserPlace::from(ViewSource::Library);
    let library_state = library.track_state_mut().unwrap();
    library_state.search = "falling".into();
    library_state.browse.genre = Some("Metalcore".into());
    history.record_route(&NavPlace::browser(library.clone()));
    let artist = history
        .navigate_from(
            NavigationIntent::OpenArtist {
                artist: ArtistKey::new("Lorna Shore"),
                anchor_track_id: None,
            },
            library,
        )
        .expect("the Artist page must be a new history place");

    let artist_query = &artist
        .browser_place()
        .track_state()
        .expect("the Artist page is a track place")
        .search;
    harness.search.set_query(SearchScope::Tracks, artist_query);
    settle();

    assert_eq!(harness.entry.text(), "falling");
    assert_eq!(
        track_chip_label(&harness).as_deref(),
        Some("⌕ “falling” in track, artist and album  ×")
    );

    let restored = history
        .go_back_from(artist.browser_place().clone())
        .expect("Back must restore the filtered Library place");
    harness
        .search
        .activate_source(&restored.view_source(), "Music");
    let restored_state = restored
        .browser_place()
        .track_state()
        .expect("the restored Library is a track place");
    harness
        .search
        .set_query(SearchScope::Tracks, &restored_state.search);
    let chip_probe = harness.track_filter_layout.clone();
    settle_until("Back restores the Library search chip", move || {
        chip_probe
            .slot_child(crate::ui::filter_bar_layout::FilterBarSlot::Search)
            .is_some()
    });

    assert_eq!(harness.entry.text(), "falling");
    assert_eq!(restored_state.browse.genre.as_deref(), Some("Metalcore"));
    assert_eq!(
        track_chip_label(&harness).as_deref(),
        Some("⌕ “falling” in track, artist and album  ×")
    );
}

// UX SEARCH-8a: while a view stays active, its query reaches that view and
// no other.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn search_8a_a_query_is_only_applied_to_the_active_view() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let harness = harness();

    harness.search.activate(SearchScope::Podcasts, "Podcasts");
    harness.entry.set_text("wer");
    let applied_probe = harness.applied.clone();
    settle_until("the typed query reaches its section", move || {
        applied_probe
            .borrow()
            .contains(&(SearchScope::Podcasts, "wer".to_owned()))
    });

    let applied = harness.applied.borrow().clone();
    assert!(
        applied
            .iter()
            .all(|(scope, query)| query.is_empty() || *scope == SearchScope::Podcasts),
        "a non-empty Podcasts query must never be handed to another view: {applied:?}"
    );
    assert!(applied.contains(&(SearchScope::Tracks, String::new())));
    assert!(applied.contains(&(SearchScope::Podcasts, "wer".to_owned())));
    assert!(harness.search.is_active(SearchScope::Podcasts));
    assert!(!harness.search.is_active(SearchScope::Tracks));
}

// UX SEARCH-8a: a view that clears its own chip pushes that back into the
// entry instead of leaving a query on screen that nothing applies.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn search_8a_a_view_clearing_its_chip_clears_the_entry() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let harness = harness();

    harness.search.activate(SearchScope::Radio, "Radio");
    harness.entry.set_text("nova");
    settle();

    harness.search.set_query(SearchScope::Radio, "");
    settle();

    assert_eq!(harness.entry.text(), "");
}

// UX SEARCH-8a: only a query is discarded on a view switch. The facet
// callback is reserved for the user's explicit Clear all action.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn search_8a_switching_views_leaves_facet_filters_untouched() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let harness = harness();

    harness.search.activate(SearchScope::Podcasts, "Podcasts");
    harness.entry.set_text("wer");
    settle();
    harness.search.activate(SearchScope::Radio, "Radio");
    settle();

    assert!(
        harness.facets_cleared.borrow().is_empty(),
        "switching views must not invoke either view's facet reset"
    );
    let applied = harness.applied.borrow();
    assert!(applied.contains(&(SearchScope::Podcasts, String::new())));
    assert!(applied.contains(&(SearchScope::Radio, String::new())));
}

// UX SEARCH-8a: Back is the deliberate exception. The complete query is
// recovered from the existing browser history's TrackViewState; the
// search coordinator owns no second origin or history flag.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn search_8a_back_from_a_detail_restores_the_same_lists_query_from_history() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let harness = harness();
    let history = NavHistory::default();

    harness
        .search
        .activate_source(&ViewSource::Library, "Music");
    harness.entry.set_text("falling");
    settle();

    let mut list = BrowserPlace::from(ViewSource::Library);
    list.track_state_mut().unwrap().search = "falling".into();
    history.record_route(&NavPlace::browser(list.clone()));
    let detail = history
        .navigate_from(
            NavigationIntent::OpenAlbum {
                album: AlbumKey::new("Pain Remains", "Lorna Shore"),
                anchor_track_id: None,
            },
            list,
        )
        .expect("the album detail must be a new history place");
    harness.search.set_query(SearchScope::Tracks, "");

    let restored = history
        .go_back_from(detail.browser_place().clone())
        .expect("Back must restore the list place");
    harness
        .search
        .activate_source(&restored.view_source(), "Music");
    let restored_query = &restored
        .browser_place()
        .track_state()
        .expect("the restored place is the same track list")
        .search;
    harness
        .search
        .set_query(SearchScope::Tracks, restored_query);
    settle();

    assert_eq!(harness.entry.text(), "falling");
    assert!(!harness.search_bar.is_search_mode());
}

// UX FIL-2a: "Clear all" clears the current section only.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn fil_2a_clear_all_only_touches_the_current_section() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let harness = harness();
    let cleared = Rc::new(Cell::new(0_u32));
    let counter = cleared.clone();
    harness.search.register(
        SearchScope::Podcasts,
        |_| {},
        move || {
            counter.set(counter.get() + 1);
        },
    );

    harness.search.activate(SearchScope::Tracks, "Music");
    harness.entry.set_text("falling");
    settle();
    harness.search.activate(SearchScope::Podcasts, "Podcasts");
    harness.entry.set_text("wer");
    settle();

    assert_eq!(cleared.get(), 0, "a view switch does not clear facets");
    harness.search.clear_all();
    settle();

    assert_eq!(harness.entry.text(), "");
    assert_eq!(
        cleared.get(),
        1,
        "only the visible section clears its facets"
    );
    harness.search.activate(SearchScope::Tracks, "Music");
    settle();
    assert_eq!(
        harness.entry.text(),
        "",
        "a new view switch must not resurrect Music's old query"
    );
}
