use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use reprise_core::library::settings::PlayerBarPosition;
use reprise_view::search_scope::SearchScope;

use super::SearchPopover;
use crate::ui::filter_bar_layout::{FilterBarLayout, FilterBarSlot};
use crate::ui::window::section_search::SectionSearch;

#[test]
fn search_4a_modified_escape_proceeds_without_aborting_search() {
    let aborted = Rc::new(std::cell::Cell::new(false));
    let abort_on_escape: Rc<RefCell<Option<super::AbortCallback>>> = Rc::new(RefCell::new(Some({
        let aborted = Rc::clone(&aborted);
        Rc::new(move || aborted.set(true))
    })));

    assert_eq!(
        super::handle_search_key(
            gtk4::gdk::Key::Escape,
            gtk4::gdk::ModifierType::CONTROL_MASK,
            &gtk4::glib::WeakRef::new(),
            &gtk4::glib::WeakRef::new(),
            &abort_on_escape,
        ),
        gtk4::glib::Propagation::Proceed
    );
    assert!(!aborted.get(), "Ctrl+Escape must preserve the active query");
}

struct ReceiptHarness {
    window: gtk4::Window,
    entry: gtk4::SearchEntry,
    search: SearchPopover,
    coordinator: Rc<SectionSearch>,
    layout: FilterBarLayout,
    facets: gtk4::Box,
    applied: Rc<RefCell<Vec<String>>>,
}

impl ReceiptHarness {
    fn new() -> Self {
        let window = gtk4::Window::builder()
            .default_width(720)
            .default_height(360)
            .build();
        let entry = gtk4::SearchEntry::new();
        entry.set_search_delay(0);
        let lens = gtk4::ToggleButton::new();
        let search = SearchPopover::new(&lens, &entry);
        let coordinator = SectionSearch::new(&entry, &search, &lens);
        let layout = FilterBarLayout::new();
        let facets = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        facets.append(&gtk4::Button::with_label("Facet"));
        layout.fill_facets(&facets);
        let applied = Rc::new(RefCell::new(Vec::new()));
        let applying = Rc::clone(&applied);
        let committed_layout = layout.clone();
        let coordinator_for_clear = Rc::downgrade(&coordinator);
        coordinator.register(
            SearchScope::Tracks,
            move |query| applying.borrow_mut().push(query.to_owned()),
            move |query| {
                let coordinator = coordinator_for_clear.clone();
                committed_layout.replace_scoped_search(SearchScope::Tracks, query, move || {
                    if let Some(coordinator) = coordinator.upgrade() {
                        coordinator.set_query(SearchScope::Tracks, "");
                    }
                });
            },
            || {},
        );

        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        root.append(&lens);
        root.append(layout.root());
        window.set_child(Some(&root));
        window.present();
        settle();

        Self {
            window,
            entry,
            search,
            coordinator,
            layout,
            facets,
            applied,
        }
    }

    fn open_and_type(&self, query: &str) {
        self.search.open();
        settle();
        assert!(self.search.is_open(), "test precondition: popover opened");
        self.entry.set_text(query);
        settle_until("live query reaches apply sink", || {
            self.applied
                .borrow()
                .last()
                .is_some_and(|value| value == query.trim())
        });
    }

    fn search_chip(&self) -> Option<gtk4::Button> {
        self.layout
            .slot_child(FilterBarSlot::Search)
            .and_downcast::<gtk4::Button>()
    }
}

impl Drop for ReceiptHarness {
    fn drop(&mut self) {
        self.window.close();
    }
}

fn settle() {
    while gtk4::glib::MainContext::default().iteration(false) {}
}

fn settle_until(label: &str, condition: impl Fn() -> bool) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !condition() {
        settle();
        assert!(std::time::Instant::now() < deadline, "timed out: {label}");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn popover_owns_the_search_entry_and_scope_caption() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let lens = gtk4::ToggleButton::new();
    let entry = gtk4::SearchEntry::new();
    let search = SearchPopover::new(&lens, &entry);

    assert_eq!(search.widget().position(), gtk4::PositionType::Bottom);
    assert_eq!(search.widget().halign(), gtk4::Align::End);
    assert!(!search.widget().has_arrow());
    assert!(search.widget().is_autohide());
    assert!(entry.is_ancestor(search.widget()));

    search.set_scope(SearchScope::Podcasts);
    assert_eq!(search.scope_text(), "Searches episode titles");
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn search_10_opening_search_changes_no_allocated_height() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let window = gtk4::Window::builder()
        .default_width(900)
        .default_height(600)
        .build();
    let header = libadwaita::HeaderBar::new();
    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    content.append(&gtk4::Label::new(Some("Library")));
    let player = gtk4::ActionBar::new();
    player.set_center_widget(Some(&gtk4::Label::new(Some("Player"))));
    let shell = crate::ui::library_player_bar::LibraryPlayerBarShell::new(
        &content,
        Some(player.upcast_ref()),
        PlayerBarPosition::Bottom,
    );
    let entry = gtk4::SearchEntry::new();
    let chrome = crate::ui::window::library_chrome::build(&header, shell.widget(), &entry, &window);
    window.set_child(Some(&chrome.root));
    window.present();
    settle();

    let measure = || {
        let player_bounds = player
            .compute_bounds(&window)
            .expect("player bar has window-relative bounds");
        (
            content.height(),
            (player_bounds.y() + player_bounds.height()).round() as i32,
            window.height(),
        )
    };
    let closed_before = measure();
    assert_eq!(closed_before.1, closed_before.2);

    chrome.search.open();
    settle();
    assert!(
        chrome.search.widget().is_visible(),
        "the middle measurement must not pass with a popover that never opened"
    );
    let open = measure();

    chrome.search.close();
    settle();
    let closed_after = measure();
    assert_eq!(open, closed_before);
    assert_eq!(closed_after, closed_before);
    window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn search_11_open_popover_shows_no_chip_while_the_count_already_filters() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let harness = ReceiptHarness::new();

    harness.open_and_type("wer");

    assert!(harness.search_chip().is_none());
    assert_eq!(
        harness.applied.borrow().last().map(String::as_str),
        Some("wer")
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn search_12_closing_commits_exactly_one_chip_before_the_facets() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let harness = ReceiptHarness::new();
    harness.open_and_type("wer");

    harness.search.close();
    settle_until("closing creates the receipt", || {
        harness.search_chip().is_some()
    });

    let chip = harness.search_chip().expect("one committed search chip");
    let slot = chip.parent().and_downcast::<gtk4::Box>().unwrap();
    assert_eq!(
        slot.first_child(),
        slot.last_child(),
        "the slot has one child"
    );
    assert!(harness.facets.first_child().is_some());
    // `slot_order()` walks the wrapper boxes, which `FilterBarLayout::new`
    // appends in a fixed order whether or not anything lives in them — asking
    // it about Search vs Facets can never fail and would pass with the commit
    // mechanism ripped out. Only the *populated* slots say something about
    // this run.
    let order = harness.layout.populated_slot_order();
    let search = order
        .iter()
        .position(|slot| *slot == FilterBarSlot::Search)
        .expect("the committed chip occupies the search slot");
    let facets = order
        .iter()
        .position(|slot| *slot == FilterBarSlot::Facets)
        .expect("the facet chips are still there");
    assert!(search < facets, "the committed chip leads the facet chips");
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn search_13_closing_with_a_blank_query_commits_nothing() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    for query in ["", "   "] {
        let harness = ReceiptHarness::new();
        harness.search.open();
        settle();
        harness.entry.set_text(query);
        harness.search.close();
        settle();

        assert!(harness.search_chip().is_none(), "query={query:?}");
        assert!(harness.facets.first_child().is_some(), "query={query:?}");
    }
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn search_14_escape_discards_while_enter_commits_the_filtered_result_set() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let harness = ReceiptHarness::new();
    harness.open_and_type("wer");

    assert_eq!(
        harness.search.press_close_key(gtk4::gdk::Key::Escape),
        gtk4::glib::Propagation::Stop
    );
    settle_until("Escape releases the filter", || {
        harness
            .applied
            .borrow()
            .last()
            .is_some_and(String::is_empty)
    });
    assert!(!harness.search.is_open());
    assert!(harness.entry.text().is_empty());
    assert!(harness.search_chip().is_none());

    harness.open_and_type("wer");
    assert_eq!(
        harness.search.press_close_key(gtk4::gdk::Key::Return),
        gtk4::glib::Propagation::Stop
    );
    settle_until("Enter commits the receipt", || {
        harness.search_chip().is_some()
    });
    assert!(!harness.search.is_open());
    assert_eq!(harness.entry.text(), "wer");
    assert!(harness.search_chip().is_some());
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn nav_6a_escape_is_consumed_before_navigation() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let harness = ReceiptHarness::new();
    harness.open_and_type("wer");

    assert_eq!(
        harness.search.press_close_key(gtk4::gdk::Key::Escape),
        gtk4::glib::Propagation::Stop
    );
    assert!(harness.entry.text().is_empty());
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn search_4a_closed_popover_escape_removes_the_committed_chip_and_filter() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let harness = ReceiptHarness::new();
    harness.open_and_type("wer");
    assert_eq!(
        harness.search.press_close_key(gtk4::gdk::Key::Return),
        gtk4::glib::Propagation::Stop
    );
    settle_until("Enter commits the search chip", || {
        harness.search_chip().is_some()
    });

    assert!(harness.coordinator.clear_active_query());
    settle_until("closed-popover Escape releases the filter", || {
        harness
            .applied
            .borrow()
            .last()
            .is_some_and(String::is_empty)
    });

    assert!(harness.entry.text().is_empty());
    assert!(harness.search_chip().is_none());
}

/// UX SEARCH-4a: the capture controller and fallback must both reach the same
/// one-stage abort path.
///
/// `press_close_key` calls the handler directly, so it proves what the handler
/// does and nothing about whether the handler wins. Two things decide that,
/// and both are properties of the toolkit rather than of our code:
///
/// 1. our key controller sits on the entry in the **capture** phase, so it
///    runs before `GtkSearchEntry`'s own bubble-phase key bindings, and
/// 2. `stop-search` — the signal Escape would reach if it ever got past us —
///    also clears the query and closes the popover.
///
/// If the phase is changed to Bubble in a refactor, the entry or navigation
/// can consume Escape first. This test is the structural counterproof.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn search_4a_escape_is_captured_and_stop_search_uses_the_same_abort() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let harness = ReceiptHarness::new();
    harness.open_and_type("wer");

    let phases: Vec<gtk4::PropagationPhase> = harness
        .entry
        .observe_controllers()
        .into_iter()
        .flatten()
        .filter_map(|controller| controller.downcast::<gtk4::EventControllerKey>().ok())
        .map(|controller| controller.propagation_phase())
        .collect();
    assert!(
        phases.contains(&gtk4::PropagationPhase::Capture),
        "the close key controller must capture, or the entry's own bindings run first: {phases:?}"
    );

    harness.entry.emit_stop_search();
    settle_until("stop-search releases the filter", || {
        harness
            .applied
            .borrow()
            .last()
            .is_some_and(String::is_empty)
    });
    assert!(harness.entry.text().is_empty());
    assert!(!harness.search.is_open());
    assert!(harness.search_chip().is_none());
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn search_15_reopening_hides_the_chip_and_prefills_the_entry() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let harness = ReceiptHarness::new();
    harness.open_and_type("wer");
    harness.search.close();
    settle_until("first close commits the receipt", || {
        harness.search_chip().is_some()
    });

    harness.search.open();
    settle();
    assert!(harness.search_chip().is_none());
    assert_eq!(harness.entry.text(), "wer");
    assert_eq!(harness.entry.position(), 3);

    harness.search.close();
    settle_until("second close restores the receipt", || {
        harness.search_chip().is_some()
    });
    let chip = harness.search_chip().unwrap();
    let slot = chip.parent().and_downcast::<gtk4::Box>().unwrap();
    assert_eq!(slot.first_child(), slot.last_child());
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn search_7a_clicking_outside_closes_and_keeps_the_filter() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let harness = ReceiptHarness::new();
    // SEARCH-2c gives focus back to the list on *every* close, so the one path
    // that does not run our own close helper has to be held to it explicitly.
    let returned_focus = Rc::new(std::cell::Cell::new(false));
    harness.search.set_focus_on_close({
        let returned_focus = Rc::clone(&returned_focus);
        Rc::new(move || {
            returned_focus.set(true);
            true
        })
    });
    harness.open_and_type("wer");

    // `popdown` is the path GTK's autohide click takes: it never reaches
    // `close()`, only the `closed` signal.
    harness.search.widget().popdown();
    settle_until("autohide commits the receipt", || {
        harness.search_chip().is_some()
    });

    assert!(!harness.search.is_open());
    assert_eq!(harness.entry.text(), "wer");
    assert_eq!(
        harness.applied.borrow().last().map(String::as_str),
        Some("wer")
    );
    assert!(
        returned_focus.get(),
        "a click outside must hand focus back to the list, like Escape does"
    );
}
