use gtk4::prelude::*;

use super::*;

#[test]
fn radio_filter_facets_are_sticky_and_each_chip_removes_one_constraint() {
    let conn = crate::test_db::open().unwrap();
    let filter = RadioFilter {
        genre: Some("Metal".into()),
        country: Some("CH".into()),
        ..RadioFilter::default()
    };
    persist_filter(&conn, &filter).unwrap();
    assert_eq!(load_filter(&conn).unwrap(), filter);
    assert_eq!(
        remove_filter(&filter, RadioFilterFacet::Genre),
        RadioFilter {
            genre: None,
            country: Some("CH".into()),
            ..RadioFilter::default()
        }
    );
    assert_eq!(
        remove_filter(&filter, RadioFilterFacet::Country),
        RadioFilter {
            genre: Some("Metal".into()),
            country: None,
            ..RadioFilter::default()
        }
    );
}

#[test]
fn filters_match_case_insensitively_and_facets_are_distinct() {
    let rows = vec![
        test_station(1, Some("Metal"), Some("CH")),
        test_station(2, Some("metal"), Some("DE")),
        test_station(3, Some("Jazz"), Some("CH")),
    ];
    let filtered = filter_rows(
        &rows,
        &RadioFilter {
            genre: Some("METAL".into()),
            country: None,
            ..RadioFilter::default()
        },
    );
    assert_eq!(
        filtered.iter().map(|row| row.id).collect::<Vec<_>>(),
        [1, 2]
    );
    assert_eq!(genre_facets(&rows), ["Jazz", "Metal"]);
    assert_eq!(country_facets(&rows), ["CH", "DE"]);
}

#[test]
fn src_13_only_the_hiding_facet_is_dropped_for_a_station() {
    let station = test_station(1, Some("Metal"), Some("DE"));
    let filter = RadioFilter {
        genre: Some("Metal".into()),
        country: Some("CH".into()),
        ..RadioFilter::default()
    };
    assert_eq!(
        filter_without_hiding(&station, &filter),
        RadioFilter {
            genre: Some("Metal".into()),
            country: None,
            ..RadioFilter::default()
        }
    );
}

#[test]
fn src_13_a_visible_station_leaves_every_facet_standing() {
    let station = test_station(1, Some("Metal"), Some("CH"));
    let filter = RadioFilter {
        genre: Some("Metal".into()),
        country: Some("CH".into()),
        ..RadioFilter::default()
    };
    assert_eq!(filter_without_hiding(&station, &filter), filter);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn search_4a_radio_escape_and_chip_share_the_section_clear_path() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let bar = RadioFilterBar::new(Rc::new(crate::test_db::open().unwrap()));
    let entry = gtk4::SearchEntry::new();
    let lens = gtk4::ToggleButton::new();
    let popover = crate::ui::window::search_popover::SearchPopover::new(&lens, &entry);
    let search = crate::ui::window::section_search::SectionSearch::new(&entry, &popover, &lens);
    search.register(
        SearchScope::Radio,
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
    search.activate(SearchScope::Radio, "Radio");
    bar.set_on_query_changed({
        let bar = Rc::downgrade(&bar);
        let search = Rc::downgrade(&search);
        move |query| {
            let bar = bar.upgrade().expect("Radio bar still exists");
            assert_eq!(
                bar.filter().query,
                "nova",
                "the chip must delegate before mutating"
            );
            if let Some(search) = search.upgrade() {
                search.set_query(SearchScope::Radio, query);
            }
        }
    });
    entry.set_text("nova");
    bar.set_query("nova");
    bar.set_committed_query("nova");
    assert_eq!(
        popover.press_close_key(gtk4::gdk::Key::Escape),
        gtk4::glib::Propagation::Stop
    );
    bar.layout.assert_search_cleared(&bar.filter().query);
    entry.set_text("nova");
    bar.set_query("nova");
    bar.set_committed_query("nova");
    bar.layout
        .slot_child(crate::ui::filter_bar_layout::FilterBarSlot::Search)
        .and_downcast::<gtk4::Button>()
        .expect("Radio search chip")
        .emit_clicked();
    bar.layout.assert_search_cleared(&bar.filter().query);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn fil_2a_radio_fills_filters_count_and_clear_slots_in_order() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let bar = RadioFilterBar::new(Rc::new(crate::test_db::open().unwrap()));
    bar.set_query("nova");
    bar.set_committed_query("nova");
    bar.set_counts(3, 44);
    assert_eq!(
        bar.root.height_request(),
        filter_bar_layout::FILTER_BAR_MIN_HEIGHT
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
        &bar.count
    ));
    assert!(bar.layout.slot_contains(
        crate::ui::filter_bar_layout::FilterBarSlot::ClearAll,
        &bar.clear_all
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
    assert!(bar.add_filter.has_css_class("pill"));
    assert!(bar.clear_all.is_visible());
}

#[test]
fn fil_1d_radio_query_matches_station_names_and_composes_with_facets() {
    let mut nova = test_station(1, Some("Jazz"), Some("de"));
    nova.name = "Radio Nova".into();
    let mut werk = test_station(2, Some("Jazz"), Some("de"));
    werk.name = "Werkstatt FM".into();
    let mut antwerp = test_station(3, Some("Rock"), Some("be"));
    antwerp.name = "Antwerpen Live".into();
    let rows = vec![nova, werk, antwerp];
    let names = |filter: &RadioFilter| {
        filter_rows(&rows, filter)
            .into_iter()
            .map(|row| row.name)
            .collect::<Vec<_>>()
    };
    assert_eq!(
        names(&RadioFilter {
            query: "wer".into(),
            ..RadioFilter::default()
        }),
        ["Werkstatt FM", "Antwerpen Live"],
        "leading and mid-word matches, case-insensitively"
    );
    assert_eq!(
        names(&RadioFilter {
            genre: Some("Jazz".into()),
            query: "wer".into(),
            ..RadioFilter::default()
        }),
        ["Werkstatt FM"],
        "the query narrows what the genre chip already returned"
    );
    assert_eq!(names(&RadioFilter::default()).len(), 3);
    assert!(RadioFilter {
        query: "wer".into(),
        ..RadioFilter::default()
    }
    .is_active());
    assert!(!RadioFilter::default().is_active());
}

#[test]
fn search_8a_radio_query_is_never_restored_from_settings() {
    let db = crate::test_db::open().unwrap();
    persist_filter(
        &db,
        &RadioFilter {
            genre: Some("Jazz".into()),
            query: "wer".into(),
            ..RadioFilter::default()
        },
    )
    .unwrap();
    let restored = load_filter(&db).unwrap();
    assert_eq!(restored.genre.as_deref(), Some("Jazz"));
    assert_eq!(restored.query, "");
}

fn test_station(id: i64, genre: Option<&str>, country: Option<&str>) -> StationRow {
    StationRow {
        id,
        uuid: None,
        name: format!("Station {id}"),
        stream_url: format!("https://radio.example/{id}"),
        homepage: None,
        favicon_url: None,
        genre: genre.map(str::to_owned),
        codec: None,
        bitrate_kbps: None,
        country_code: country.map(str::to_owned),
        votes: None,
        added_at: 10,
        removed_at: None,
    }
}
