use gtk4::prelude::*;

use super::*;

#[test]
fn nr_33_type_toggles_are_independent_and_empty_means_every_type() {
    let selection = ReleaseTypeSelection::default();
    let selection = toggle_type(selection, TypeChip::Album, false);
    let selection = toggle_type(selection, TypeChip::Ep, false);
    assert!(selection.is_empty());
    assert!(selection.includes("Album"));
    assert!(selection.includes("EP"));
    assert!(selection.includes("Single"));
}

#[test]
fn fil_2a_widest_scope_count_line_names_shown_and_total() {
    assert_eq!(release_count_presentation(19, 19), "19 gaps");
    assert_eq!(release_count_presentation(168, 629), "168 of 629 gaps");
}

#[test]
fn sticky_release_filter_round_trips_every_facet() {
    let conn = crate::test_db::open().unwrap();
    let filter = ReleasesFilter {
        release_types: ReleaseTypeSelection {
            album: true,
            ep: false,
            single: true,
        },
        window: ReleaseWindow::TenYears,
        hidden: true,
    };
    persist_filter(&conn, &filter).unwrap();
    assert_eq!(persisted_releases_filter(&conn).unwrap(), filter);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn search_4a_releases_escape_and_chip_share_the_section_clear_path() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let bar = ReleasesFilterBar::new(Rc::new(crate::test_db::open().unwrap()));
    let entry = gtk4::SearchEntry::new();
    let lens = gtk4::ToggleButton::new();
    let popover = crate::ui::window::search_popover::SearchPopover::new(&lens, &entry);
    let search = crate::ui::window::section_search::SectionSearch::new(&entry, &popover, &lens);
    search.register(
        SearchScope::Releases,
        {
            let bar = Rc::downgrade(&bar);
            move |query| {
                if let Some(bar) = bar.upgrade() {
                    bar.set_query(query);
                }
            }
        },
        {
            let bar = Rc::downgrade(&bar);
            move |query| {
                if let Some(bar) = bar.upgrade() {
                    bar.set_committed_query(query);
                }
            }
        },
        || {},
    );
    search.activate(SearchScope::Releases, "Releases");
    bar.set_on_query_changed({
        let bar = Rc::downgrade(&bar);
        let search = Rc::downgrade(&search);
        move |query| {
            let bar = bar.upgrade().expect("Releases bar still exists");
            assert_eq!(
                bar.query(),
                "falling",
                "the chip must delegate before mutating"
            );
            if let Some(search) = search.upgrade() {
                search.set_query(SearchScope::Releases, query);
            }
        }
    });
    entry.set_text("falling");
    bar.set_query("falling");
    bar.set_committed_query("falling");
    assert_eq!(
        popover.press_close_key(gtk4::gdk::Key::Escape),
        gtk4::glib::Propagation::Stop
    );
    bar.layout.assert_search_cleared(&bar.query());
    entry.set_text("falling");
    bar.set_query("falling");
    bar.set_committed_query("falling");
    bar.layout
        .slot_child(crate::ui::filter_bar_layout::FilterBarSlot::Search)
        .and_downcast::<gtk4::Button>()
        .expect("Releases search chip")
        .emit_clicked();
    bar.layout.assert_search_cleared(&bar.query());
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn nr_33_filter_header_is_permanent_and_reserves_its_height() {
    gtk4::init().unwrap();
    let bar = ReleasesFilterBar::new(Rc::new(crate::test_db::open().unwrap()));
    assert_eq!(
        bar.root.height_request(),
        crate::ui::filter_bar_layout::FILTER_BAR_MIN_HEIGHT
    );
    assert!(bar.chips.first_child().is_some());
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn fil_2a_releases_fill_filters_count_and_clear_slots_without_a_caption() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let bar = ReleasesFilterBar::new(Rc::new(crate::test_db::open().unwrap()));
    bar.set_query("falling");
    bar.set_committed_query("falling");
    bar.set_counts(15, 1_664);
    assert!(bar.layout.slot_contains(
        crate::ui::filter_bar_layout::FilterBarSlot::Facets,
        &bar.chips
    ));
    assert!(bar.layout.slot_contains(
        crate::ui::filter_bar_layout::FilterBarSlot::AddFilter,
        &bar.add_filter
    ));
    assert!(bar.layout.slot_contains(
        crate::ui::filter_bar_layout::FilterBarSlot::Count,
        &bar.result_label
    ));
    assert!(bar.layout.slot_contains(
        crate::ui::filter_bar_layout::FilterBarSlot::ClearAll,
        &bar.clear_all
    ));
    let first = bar
        .layout
        .slot_child(crate::ui::filter_bar_layout::FilterBarSlot::Search)
        .expect("the query produces the first chip");
    assert!(first
        .downcast::<gtk4::Button>()
        .ok()
        .and_then(|button| button.label())
        .is_some_and(|label| label.starts_with('⌕')));
    assert!(!descendant_labels(bar.widget())
        .iter()
        .any(|text| text == "FILTER"));
    assert!(bar.clear_all.is_visible());
}

fn descendant_labels(widget: &impl IsA<gtk4::Widget>) -> Vec<String> {
    let mut labels = Vec::new();
    let mut child = widget.as_ref().first_child();
    while let Some(current) = child {
        if let Ok(label) = current.clone().downcast::<gtk4::Label>() {
            labels.push(label.text().to_string());
        }
        labels.extend(descendant_labels(&current));
        child = current.next_sibling();
    }
    labels
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn fil_2a_the_default_filter_row_offers_no_clear_all() {
    gtk4::init().unwrap();
    let bar = ReleasesFilterBar::new(Rc::new(crate::test_db::open().unwrap()));
    bar.set_counts(168, 629);
    assert!(
        !bar.clear_all.get_visible(),
        "a default filter row has nothing to clear"
    );
    bar.apply_filter(ReleasesFilter::widest(false));
    assert!(
        bar.clear_all.get_visible(),
        "widening the scope is a change, and a change is undoable"
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn window_chip_opens_the_inline_picker_without_changing_the_filter() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let bar = ReleasesFilterBar::new(Rc::new(crate::test_db::open().unwrap()));
    let window = gtk4::Window::new();
    window.set_child(Some(bar.widget()));
    window.present();
    while gtk4::glib::MainContext::default().iteration(false) {}
    let before = bar.filter();
    let window_label = window_label(before.window);
    let window_chip = std::iter::successors(bar.chips.first_child(), gtk4::Widget::next_sibling)
        .filter_map(|widget| widget.downcast::<gtk4::Button>().ok())
        .find(|button| button.label().as_deref() == Some(window_label.as_str()))
        .expect("the permanent window chip is an action without a remove suffix");

    window_chip.emit_clicked();

    assert_eq!(bar.filter(), before);
    assert_eq!(
        std::iter::successors(bar.value_list.first_child(), gtk4::Widget::next_sibling).count(),
        4,
        "the one-click action opens all four date-window choices"
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn add_filter_is_disabled_when_every_addable_release_value_is_selected() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let bar = ReleasesFilterBar::new(Rc::new(crate::test_db::open().unwrap()));
    bar.apply_filter(ReleasesFilter {
        release_types: ReleaseTypeSelection {
            album: true,
            ep: true,
            single: true,
        },
        window: ReleaseWindow::TenYears,
        hidden: true,
    });

    assert!(!bar.add_filter.is_sensitive());
}
