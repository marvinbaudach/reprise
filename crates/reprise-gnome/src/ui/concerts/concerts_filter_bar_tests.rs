use super::*;

fn filtered() -> ConcertFilter {
    ConcertFilter {
        radius_km: Some(100.0),
        country: Some("DE".into()),
        horizon: DateHorizon::Next3Months,
        include_similar: true,
    }
}

#[test]
fn conc_2_each_chip_removes_exactly_one_constraint_and_clear_is_default() {
    let filter = filtered();
    assert_eq!(remove_filter(&filter, FilterFacet::Radius).radius_km, None);
    assert_eq!(remove_filter(&filter, FilterFacet::Country).country, None);
    assert_eq!(
        remove_filter(&filter, FilterFacet::Horizon).horizon,
        DateHorizon::AllUpcoming
    );
    assert!(!remove_filter(&filter, FilterFacet::Source).include_similar);
}

#[test]
fn conc_2_radius_is_active_only_when_location_makes_it_meaningful() {
    let conn = crate::test_db::open().unwrap();
    reprise_core::location::set_default_radius_km(&conn, 500.0).unwrap();
    let filter = config::persisted_filter(&conn).unwrap();
    assert_eq!(filter.radius_km, Some(500.0));
    assert!(!active_facets(&filter, false).contains(&FilterFacet::Radius));
    assert!(active_facets(&filter, true).contains(&FilterFacet::Radius));
}

#[test]
fn conc_2_location_chip_names_the_city_and_off_state_names_the_radius() {
    let filter = ConcertFilter {
        radius_km: Some(500.0),
        ..ConcertFilter::default()
    };
    assert_eq!(
        chip_label(&filter, FilterFacet::Radius, Some("Zürich")),
        "Zürich · 500 km"
    );
    assert_eq!(radius_off_label(&filter), Some("500 km · off".to_owned()));
}

#[test]
fn conc_6_source_pill_exists_for_enabled_or_cached_similar_artists() {
    assert!(!source_facet_visible(false, false));
    assert!(source_facet_visible(true, false));
    assert!(source_facet_visible(false, true));
}

#[test]
fn persisted_filter_round_trips_every_sticky_facet() {
    let conn = crate::test_db::open().unwrap();
    persist_filter(&conn, &filtered()).unwrap();
    assert_eq!(config::persisted_filter(&conn).unwrap(), filtered());
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn search_4a_concerts_escape_and_chip_share_the_section_clear_path() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let bar = ConcertsFilterBar::new(Rc::new(crate::test_db::open().unwrap()));
    let entry = gtk4::SearchEntry::new();
    let lens = gtk4::ToggleButton::new();
    let popover = crate::ui::window::search_popover::SearchPopover::new(&lens, &entry);
    let search = crate::ui::window::section_search::SectionSearch::new(&entry, &popover, &lens);
    search.register(
        SearchScope::Concerts,
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
    search.activate(SearchScope::Concerts, "Concerts");
    bar.set_on_query_changed({
        let bar = Rc::downgrade(&bar);
        let search = Rc::downgrade(&search);
        move |query| {
            let bar = bar.upgrade().expect("Concerts bar still exists");
            assert_eq!(bar.query(), "winterthur", "delegate before mutating");
            if let Some(search) = search.upgrade() {
                search.set_query(SearchScope::Concerts, query);
            }
        }
    });

    entry.set_text("winterthur");
    bar.set_query("winterthur");
    bar.set_committed_query("winterthur");
    assert_eq!(
        popover.press_close_key(gtk4::gdk::Key::Escape),
        gtk4::glib::Propagation::Stop
    );
    bar.layout.assert_search_cleared(&bar.query());

    entry.set_text("winterthur");
    bar.set_query("winterthur");
    bar.set_committed_query("winterthur");
    bar.layout
        .slot_child(crate::ui::filter_bar_layout::FilterBarSlot::Search)
        .and_downcast::<gtk4::Button>()
        .expect("Concerts search chip")
        .emit_clicked();
    bar.layout.assert_search_cleared(&bar.query());
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn conc_2_filter_header_has_fixed_height_and_disabled_radius_hint() {
    gtk4::init().unwrap();
    let conn = Rc::new(crate::test_db::open().unwrap());
    let bar = ConcertsFilterBar::new(conn);
    assert_eq!(
        bar.root.height_request(),
        filter_bar_layout::FILTER_BAR_MIN_HEIGHT
    );
    let radius = bar.facet_list.row_at_index(0).unwrap();
    assert!(!radius.is_sensitive());
    assert_eq!(
        radius.tooltip_text().as_deref(),
        Some("Set a location in Preferences")
    );

    let opened = Rc::new(Cell::new(false));
    let flag = opened.clone();
    bar.set_on_open_location(move || flag.set(true));
    bar.set_counts(44, 44);
    assert_eq!(bar.result_label.text(), "44 concerts");
    let off = child_buttons(&bar.chips)
        .into_iter()
        .find(|button| button.label().as_deref() == Some("1000 km · off"))
        .expect("the inactive radius is a visible navigation chip");
    assert!(off.is_sensitive());
    off.emit_clicked();
    assert!(opened.get());

    let location = reprise_core::location::AppLocation {
        latitude: 47.376,
        longitude: 8.541,
        name: "Zürich".into(),
        country_code: Some("CH".into()),
    };
    bar.set_context(Some(&location), false, false);
    bar.set_counts(44, 44);
    assert_eq!(bar.result_label.text(), "44 of 44 concerts");
    assert!(child_buttons(&bar.chips).into_iter().any(|button| {
        button
            .label()
            .is_some_and(|label| label.starts_with("Zürich · 1000 km"))
    }));
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn fil_2a_concerts_fill_filters_count_and_clear_slots_in_order() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let conn = Rc::new(crate::test_db::open().unwrap());
    let bar = ConcertsFilterBar::new(conn);
    bar.set_query("winterthur");
    bar.set_committed_query("winterthur");
    bar.set_counts(3, 44);

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
        .expect("query fills the search slot");
    assert!(first
        .downcast::<gtk4::Button>()
        .ok()
        .and_then(|button| button.label())
        .is_some_and(|label| label.starts_with('⌕')));
    assert!(!descendant_labels(bar.widget())
        .iter()
        .any(|text| text == "FILTER"));
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

fn child_buttons(container: &gtk4::Box) -> Vec<gtk4::Button> {
    std::iter::successors(container.first_child(), WidgetExt::next_sibling)
        .filter_map(|widget| widget.downcast::<gtk4::Button>().ok())
        .collect()
}
