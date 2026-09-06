use super::*;

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn search_8a_a_query_neither_persists_nor_depends_on_persistence() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let conn = Rc::new(crate::test_db::open().unwrap());
    let bar = PodcastsFilterBar::new(conn.clone(), PodcastKind::Rss);
    bar.apply_filter(PodcastFilter {
        unplayed_only: true,
        ..PodcastFilter::default()
    });
    bar.set_query("wer");
    assert_eq!(bar.filter().query, "wer", "the query is applied in-session");
    let stored = podcasts::config::load_filter(&conn).unwrap();
    assert!(
        stored.unplayed_only,
        "the facet the user picked is persisted"
    );
    assert_eq!(
        PodcastFilter::from_facets(&stored),
        PodcastFilter {
            unplayed_only: true,
            ..PodcastFilter::default()
        },
        "nothing the query touched reached the database"
    );
    bar.set_query("");
    assert_eq!(bar.filter().query, "");
    assert!(podcasts::config::load_filter(&conn).unwrap().unplayed_only);
}

#[test]
fn src_2_add_action_is_tinted_button_not_chip() {
    assert_eq!(buttons::ADD_ACTION_CLASS, "reprise-btn-add");
    assert_ne!(buttons::ADD_ACTION_CLASS, filter_bar_layout::CHIP_CSS_CLASS);
    assert!(!buttons::ADD_ACTION_CLASS.contains("chip"));
}

#[test]
fn active_filter_detection_tracks_unplayed_and_downloaded_independently() {
    use super::super::podcasts_presentation::active;
    assert!(!active(&PodcastFilter::default()));
    assert!(active(&PodcastFilter {
        unplayed_only: true,
        ..PodcastFilter::default()
    }));
    assert!(active(&PodcastFilter {
        downloaded_only: true,
        ..PodcastFilter::default()
    }));
    assert!(active(&PodcastFilter {
        unplayed_only: true,
        downloaded_only: true,
        ..PodcastFilter::default()
    }));
    assert!(!active(&PodcastFilter {
        source: Some(PodcastKind::Youtube),
        ..PodcastFilter::default()
    }));
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn search_4a_podcasts_escape_and_chip_share_the_section_clear_path() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let bar = PodcastsFilterBar::new(Rc::new(crate::test_db::open().unwrap()), PodcastKind::Rss);
    let entry = gtk4::SearchEntry::new();
    let lens = gtk4::ToggleButton::new();
    let popover = crate::ui::window::search_popover::SearchPopover::new(&lens, &entry);
    let search = crate::ui::window::section_search::SectionSearch::new(&entry, &popover, &lens);
    search.register(
        SearchScope::Podcasts,
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
    search.activate(SearchScope::Podcasts, "Podcasts");
    bar.set_on_query_changed({
        let bar = Rc::downgrade(&bar);
        let search = Rc::downgrade(&search);
        move |query| {
            let bar = bar.upgrade().expect("Podcasts bar still exists");
            assert_eq!(
                bar.filter().query,
                "falling",
                "the chip must delegate before mutating"
            );
            if let Some(search) = search.upgrade() {
                search.set_query(SearchScope::Podcasts, query);
            }
        }
    });
    entry.set_text("falling");
    bar.set_query("falling");
    bar.set_committed_query("falling");
    assert!(
        bar.layout
            .populated_slot_order()
            .contains(&crate::ui::filter_bar_layout::FilterBarSlot::Search),
        "the committed query is showing before the click"
    );
    assert_eq!(
        popover.press_close_key(gtk4::gdk::Key::Escape),
        gtk4::glib::Propagation::Stop
    );
    bar.layout.assert_search_cleared(&bar.filter().query);
    entry.set_text("falling");
    bar.set_query("falling");
    bar.set_committed_query("falling");
    bar.layout
        .slot_child(crate::ui::filter_bar_layout::FilterBarSlot::Search)
        .and_downcast::<gtk4::Button>()
        .expect("Podcasts search chip")
        .emit_clicked();
    bar.layout.assert_search_cleared(&bar.filter().query);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn fil_2a_podcasts_and_youtube_fill_the_same_ordered_slots() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    for kind in [PodcastKind::Rss, PodcastKind::Youtube] {
        let bar = PodcastsFilterBar::new(Rc::new(crate::test_db::open().unwrap()), kind);
        assert_eq!(
            bar.root.height_request(),
            filter_bar_layout::FILTER_BAR_MIN_HEIGHT
        );
        bar.set_query("falling");
        bar.set_committed_query("falling");
        bar.set_context(
            3,
            LibrarySummary {
                shows: 4,
                episodes: 44,
                new: 2,
            },
            1,
        );
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
            &bar.result
        ));
        assert!(bar.layout.slot_contains(
            crate::ui::filter_bar_layout::FilterBarSlot::ClearAll,
            &bar.clear_all
        ));
        assert!(bar.layout.slot_contains(
            crate::ui::filter_bar_layout::FilterBarSlot::TrailingAction,
            &bar.clear_selection
        ));
        let first = bar
            .layout
            .slot_child(crate::ui::filter_bar_layout::FilterBarSlot::Search)
            .expect("search chip");
        assert!(first
            .downcast::<gtk4::Button>()
            .ok()
            .and_then(|button| button.label())
            .is_some_and(|label| label.starts_with('⌕')));
        assert!(bar.clear_all.is_visible());
        assert!(bar.clear_selection.is_visible());
    }
}
