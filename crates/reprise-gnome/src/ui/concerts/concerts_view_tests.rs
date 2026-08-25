use super::*;

fn build_view(conn: Rc<Db>, runtime: &Rc<ConcertsRuntime>) -> ConcertsView {
    ConcertsView::new(
        conn,
        runtime,
        &Rc::new(crate::ui::location_broadcast::LocationBroadcast::default()),
    )
}

fn row_with_links(ticket_url: Option<&str>, event_url: Option<&str>) -> ConcertRow {
    ConcertRow {
        id: 1,
        availability: reprise_core::concerts::TicketAvailability::Unknown,
        date_key: "2026-08-20".into(),
        starts_at: "2026-08-20T19:00:00".into(),
        artist_name: "Artist".into(),
        venue: "Venue".into(),
        city: "Zurich".into(),
        region: None,
        country: Some("CH".into()),
        latitude: Some(47.3769),
        longitude: Some(8.5417),
        distance_km: Some(100.0),
        ticket_url: ticket_url.map(str::to_owned),
        ticket_source: Some("Ticketmaster".into()),
        event_url: event_url.map(str::to_owned),
        provider: "ticketmaster".into(),
        is_similar: false,
        similar_to: None,
    }
}

fn column_by_id(view: &gtk4::ColumnView, id: &str) -> gtk4::ColumnViewColumn {
    (0..view.columns().n_items())
        .find_map(|index| {
            let column = view
                .columns()
                .item(index)
                .and_downcast::<gtk4::ColumnViewColumn>()?;
            (column.id().as_deref() == Some(id)).then_some(column)
        })
        .unwrap_or_else(|| panic!("missing Concerts column {id}"))
}

fn model_row_ids(view: &ConcertsView) -> Vec<i64> {
    let store = view.shared.model.store();
    (0..store.n_items())
        .map(|index| {
            store
                .item(index)
                .and_downcast::<super::super::concerts_model::ConcertObject>()
                .expect("Concerts model rows use ConcertObject")
                .row()
                .id
        })
        .collect()
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn conc_17a_the_concerts_cover_column_is_pinned_id_less_and_unsorted() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let conn = Rc::new(crate::test_db::open().unwrap());
    let runtime = ConcertsRuntime::setup(&conn);
    let view = build_view(conn, &runtime);
    let cover = view
        .shared
        .column_view
        .columns()
        .item(0)
        .and_downcast::<gtk4::ColumnViewColumn>()
        .expect("the leading Concerts column exists");

    assert!(cover.id().is_none());
    assert!(cover.sorter().is_none());
    assert_eq!(
        cover.title().as_deref(),
        Some(crate::ui::strings::text(crate::ui::strings::COLUMN_COVER).as_str())
    );
    assert_eq!(
        cover.fixed_width(),
        crate::ui::table_column_widths::COVER_COLUMN
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn conc_17a_a_concert_cover_shows_initials_until_a_portrait_resolves() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let cache_checked = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let image = super::super::concerts_artist_cover::ConcertsArtistImage::for_test({
        let cache_checked = cache_checked.clone();
        move |_| {
            cache_checked.store(true, std::sync::atomic::Ordering::Release);
            None
        }
    });
    let tile = crate::ui::updates::release_cover::LazyReleaseCover::new_unbound(
        crate::ui::table_column_widths::COVER,
    );

    tile.set_artist_key("Falling Leaves");
    image.show(&tile);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !cache_checked.load(std::sync::atomic::Ordering::Acquire) {
        while gtk4::glib::MainContext::default().iteration(false) {}
        assert!(
            std::time::Instant::now() < deadline,
            "timed out waiting for the injected portrait resolver"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    while gtk4::glib::MainContext::default().iteration(false) {}

    assert_eq!(tile.initials_text(), "FL");
    assert!(!tile.shows_image());
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn conc_17a_a_rebound_concert_cover_never_shows_the_previous_artist() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let tile = crate::ui::updates::release_cover::LazyReleaseCover::new_unbound(
        crate::ui::table_column_widths::COVER,
    );
    tile.set_artist_key("Falling Leaves");
    let bytes = gtk4::glib::Bytes::from_owned(vec![0x80_u8; 4]);
    let old_portrait =
        gtk4::gdk::MemoryTexture::new(1, 1, gtk4::gdk::MemoryFormat::R8g8b8a8, &bytes, 4);
    tile.show_paintable(Some(old_portrait.upcast_ref::<gtk4::gdk::Paintable>()));

    tile.set_artist_key("Better Artist");

    assert_eq!(tile.initials_text(), "BA");
    assert!(!tile.shows_image());
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn conc_17a_every_sortable_concerts_header_orders_its_own_column() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let conn = Rc::new(crate::test_db::open().unwrap());
    let runtime = ConcertsRuntime::setup(&conn);
    let view = build_view(conn, &runtime);
    view.shared.location_columns.apply(true);
    let mut first = row_with_links(None, None);
    first.id = 1;
    first.artist_name = "Charlie".into();
    first.city = "Amsterdam".into();
    first.venue = "Medium".into();
    first.ticket_source = Some("Zulu".into());
    first.date_key = "2026-08-18".into();
    first.distance_km = Some(200.0);
    let mut second = row_with_links(None, None);
    second.id = 2;
    second.artist_name = "Alpha".into();
    second.city = "Zurich".into();
    second.venue = "Large".into();
    second.ticket_source = Some("Beta".into());
    second.date_key = "2026-08-19".into();
    second.distance_km = Some(100.0);
    let mut third = row_with_links(None, None);
    third.id = 3;
    third.artist_name = "Bravo".into();
    third.city = "Berlin".into();
    third.venue = "Small".into();
    third.ticket_source = Some("Alpha".into());
    third.date_key = "2026-08-17".into();
    third.distance_km = Some(300.0);
    view.shared
        .rows
        .replace(vec![first.clone(), second.clone(), third.clone()]);
    view.shared.model.replace(vec![first, second, third]);

    for (id, expected) in [
        ("artist", vec![2, 3, 1]),
        ("city", vec![1, 3, 2]),
        ("venue", vec![2, 1, 3]),
        ("source", vec![3, 2, 1]),
        ("date", vec![3, 1, 2]),
        ("distance", vec![2, 1, 3]),
    ] {
        let column = column_by_id(&view.shared.column_view, id);
        view.shared
            .column_view
            .sort_by_column(Some(&column), gtk4::SortType::Ascending);
        assert_eq!(model_row_ids(&view), expected, "wrong order for {id}");
    }
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn only_the_ticket_header_carries_no_sorter() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let conn = Rc::new(crate::test_db::open().unwrap());
    let runtime = ConcertsRuntime::setup(&conn);
    let view = build_view(conn, &runtime);

    assert!(column_by_id(&view.shared.column_view, "tickets")
        .sorter()
        .is_none());
    for id in ["artist", "city", "venue", "source", "date"] {
        assert!(
            column_by_id(&view.shared.column_view, id)
                .sorter()
                .is_some(),
            "{id} must expose a clickable sorting header"
        );
    }
}

#[path = "concerts_sort_indicator_tests.rs"]
mod sort_indicator_tests;

#[test]
fn conc_13_a_row_without_a_target_does_not_activate() {
    let row = row_with_links(None, None);
    let presentation = super::super::concerts_status_cells::row_link_presentation(&row);
    let activations = Rc::new(Cell::new(0));
    let on_open: super::super::concerts_columns::OnOpenTarget = {
        let activations = activations.clone();
        Rc::new(move |_| activations.set(activations.get() + 1))
    };

    assert!(!presentation.activatable);
    assert_eq!(presentation.tooltip, "No ticket or event link available");
    assert_eq!(presentation.accessible_description, presentation.tooltip);
    assert!(!super::super::concerts_activation::activate_row(
        &row, &on_open
    ));
    assert_eq!(activations.get(), 0);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn conc_14_the_similar_caption_shrinks_before_the_artist() {
    gtk4::init().unwrap();
    let cell = super::super::concerts_columns::build_artist_cell();

    assert_eq!(cell.root.orientation(), gtk4::Orientation::Horizontal);
    assert_eq!(cell.artist.ellipsize(), gtk4::pango::EllipsizeMode::None);
    assert!(!cell.artist.hexpands());
    assert_eq!(cell.caption.ellipsize(), gtk4::pango::EllipsizeMode::End);
    assert!(cell.caption.hexpands());
}

#[test]
fn distance_class_changes_only_above_the_radius_boundary() {
    use super::super::concerts_status_cells::distance_class;

    assert_eq!(
        distance_class(Some(500.0), Some(500.0)),
        "reprise-concert-distance-near"
    );
    assert_eq!(
        distance_class(Some(500.01), Some(500.0)),
        "reprise-concert-distance-far"
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn concerts_view_exposes_eight_columns_and_row_activation() {
    gtk4::init().unwrap();
    let conn = Rc::new(crate::test_db::open().unwrap());
    let runtime = ConcertsRuntime::setup(&conn);
    let view = build_view(conn.clone(), &runtime);
    let root = view.root().clone().downcast::<gtk4::Box>().unwrap();
    let stack = root
        .first_child()
        .and_then(|child| child.next_sibling())
        .and_then(|child| child.next_sibling())
        .and_then(|child| child.next_sibling())
        .and_downcast::<gtk4::Stack>()
        .unwrap();
    let scrolled = stack
        .child_by_name(LIST_PAGE)
        .and_downcast::<gtk4::Overlay>()
        .and_then(|overlay| overlay.child())
        .and_downcast::<gtk4::ScrolledWindow>()
        .unwrap();
    let table = scrolled.child().and_downcast::<gtk4::ColumnView>().unwrap();
    // Cover, Artist, Date, City, Venue, Distance, Tickets, Source — the
    // portrait (#600) is the eighth and the only one carrying a leading pin.
    assert_eq!(table.columns().n_items(), 8);
    assert!(!table.enables_rubberband());
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn conc_16_the_source_column_is_available_but_off_by_default() {
    gtk4::init().unwrap();
    let conn = Rc::new(crate::test_db::open().unwrap());
    let runtime = ConcertsRuntime::setup(&conn);
    let view = build_view(conn.clone(), &runtime);
    let model = view.column_model();

    assert!(model.columns().iter().any(|column| column.id == "source"));
    assert!(!model.is_visible("source"));
    model.set_visible("source", true);
    assert!(model.is_visible("source"));

    let reopened = build_view(conn, &runtime);
    assert!(reopened.column_model().is_visible("source"));
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn conc_4c_app_location_broadcast_re_evaluates_the_open_view() {
    gtk4::init().unwrap();
    let conn = Rc::new(crate::test_db::open().unwrap());
    let runtime = ConcertsRuntime::setup(&conn);
    let location_broadcast = Rc::new(crate::ui::location_broadcast::LocationBroadcast::default());
    let view = ConcertsView::new(conn.clone(), &runtime, &location_broadcast);
    let refreshes = Rc::new(Cell::new(0));
    view.set_on_refreshed({
        let refreshes = refreshes.clone();
        move || refreshes.set(refreshes.get() + 1)
    });

    reprise_core::location::store(&conn, 52.52, 13.405, "Berlin, DE", Some("DE")).unwrap();
    location_broadcast.notify();

    assert_eq!(refreshes.get(), 1);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn conc_2_location_availability_hides_distance_without_overwriting_user_choice() {
    gtk4::init().unwrap();
    let conn = Rc::new(crate::test_db::open().unwrap());
    let runtime = ConcertsRuntime::setup(&conn);
    let broadcast = Rc::new(crate::ui::location_broadcast::LocationBroadcast::default());
    let view = ConcertsView::new(conn.clone(), &runtime, &broadcast);

    view.refresh();
    assert!(!view.shared.location_columns.distance_visible());
    assert!(!view.shared.location_columns.distance_sortable());
    assert!(view.shared.location_columns.venue_expands());
    assert_eq!(
        view.shared.location_columns.venue_column_id().as_deref(),
        Some("venue"),
        "the Venue column, not City, must absorb the freed width"
    );
    assert!(!view
        .shared
        .column_model
        .columns()
        .iter()
        .any(|column| column.id == "distance"));

    reprise_core::location::store(&conn, 47.376, 8.541, "Zürich", Some("CH")).unwrap();
    broadcast.notify();
    assert!(view.shared.location_columns.distance_visible());
    assert!(view.shared.location_columns.distance_sortable());
    view.shared
        .location_columns
        .sort_by_distance(gtk4::SortType::Descending);
    reprise_core::location::clear(&conn).unwrap();
    broadcast.notify();
    assert_eq!(
        view.shared.location_columns.primary_sort().0.as_deref(),
        Some("date")
    );
    reprise_core::location::store(&conn, 47.376, 8.541, "Zürich", Some("CH")).unwrap();
    broadcast.notify();
    assert_eq!(
        view.shared.location_columns.primary_sort(),
        (Some("distance".to_owned()), gtk4::SortType::Descending),
        "restoring location must restore the exact Distance sort"
    );

    view.shared.column_model.set_visible("distance", false);
    reprise_core::location::clear(&conn).unwrap();
    broadcast.notify();
    reprise_core::location::store(&conn, 47.376, 8.541, "Zürich", Some("CH")).unwrap();
    broadcast.notify();
    assert!(
        !view.shared.location_columns.distance_visible(),
        "restoring location must honor the user's previously hidden Distance column"
    );
}

/// How far ahead a seeded event sits. The view only ever queries
/// `date_key >= today`, so a fixture pinned to a calendar date silently
/// stops producing rows on the day that date passes.
const SEEDED_EVENT_DAYS_AHEAD: i64 = 7;

fn seeded_event_date() -> String {
    (chrono::Local::now().date_naive() + chrono::Duration::days(SEEDED_EVENT_DAYS_AHEAD))
        .format("%Y-%m-%d")
        .to_string()
}

fn insert_event(conn: &Db, id: i64, artist: &str) {
    let date_key = seeded_event_date();
    crate::test_db::connection(conn)
        .execute(
            "INSERT INTO concert_events (
               id, artist_key, artist_name, starts_at, date_key, venue, city,
               country, provider, fetched_at, dedupe_key
             ) VALUES (?1, ?2, ?3, ?5, ?6,
                       'Venue', 'Zurich', 'CH', 'bandsintown', 1, ?4)",
            rusqlite::params![
                id,
                format!("artist-{id}"),
                artist,
                format!("event-{id}"),
                format!("{date_key}T19:00:00"),
                date_key
            ],
        )
        .unwrap();
}

fn descendant_with_class<T: IsA<gtk4::Widget> + Clone + 'static>(
    widget: &gtk4::Widget,
    class: &str,
) -> Option<T> {
    if widget.has_css_class(class) {
        if let Ok(found) = widget.clone().downcast::<T>() {
            return Some(found);
        }
    }
    let mut child = widget.first_child();
    while let Some(current) = child {
        if let Some(found) = descendant_with_class(&current, class) {
            return Some(found);
        }
        child = current.next_sibling();
    }
    None
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn fil_3a_concerts_end_line_counts_concerts_and_recovers_with_clear_all() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let conn = Rc::new(crate::test_db::open().unwrap());
    insert_event(&conn, 1, "Afd Artist");
    insert_event(&conn, 2, "Different Artist");
    let runtime = ConcertsRuntime::setup(&conn);
    let view = build_view(conn, &runtime);
    let window = gtk4::Window::new();
    window.set_default_size(900, 600);
    window.set_child(Some(view.root()));
    window.present();

    view.set_search_query("afd");
    crate::ui::source_context_surface::settle_layout();

    let line = descendant_with_class::<gtk4::Label>(
        view.root(),
        crate::ui::end_of_results::LINE_CSS_CLASS,
    )
    .expect("Concerts owns the shared end-of-results line");
    assert_eq!(
        line.text(),
        "End of results — 1 concert hidden by search “afd”"
    );
    assert!(line.is_visible());
    let recovery = descendant_with_class::<gtk4::Button>(
        view.root(),
        crate::ui::end_of_results::RECOVERY_CSS_CLASS,
    )
    .expect("Concerts owns the shared recovery pill");
    assert_eq!(recovery.label().as_deref(), Some("Show all 2 concerts"));
    recovery.emit_clicked();
    crate::ui::source_context_surface::settle_layout();
    assert_eq!(view.shared.filter_bar.query(), "");
    assert!(!line.is_visible());
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn fil_3a_the_concerts_end_of_results_sits_below_the_last_row() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let conn = Rc::new(crate::test_db::open().unwrap());
    for id in 1..=3 {
        insert_event(&conn, id, &format!("Shown Artist {id}"));
    }
    insert_event(&conn, 4, "Hidden Artist");
    let runtime = ConcertsRuntime::setup(&conn);
    let view = build_view(conn, &runtime);
    let window = gtk4::Window::new();
    window.set_default_size(900, 600);
    window.set_child(Some(view.root()));
    window.present();

    view.set_search_query("Shown");
    crate::ui::source_context_surface::settle_layout();

    let line = descendant_with_class::<gtk4::Label>(
        view.root(),
        crate::ui::end_of_results::LINE_CSS_CLASS,
    )
    .expect("Concerts owns the shared end-of-results line");
    // The rule is a relation, not a coordinate: the marker belongs directly
    // under the last row. A literal ceiling measured that only by accident —
    // it broke the moment #600 made rows taller with the portrait column,
    // although the marker still sat exactly where FIL-3a wants it. Anchor on
    // the last row's own label instead, so row height stays free to change.
    let last_row = descendant_label_with_text(view.root(), "Shown Artist 3")
        .expect("the third filtered row must be on screen");
    let last_bottom = last_row
        .compute_bounds(view.root())
        .map(|bounds| bounds.y() + bounds.height())
        .expect("the last row must be allocated");
    let bounds = line.compute_bounds(view.root()).unwrap();
    let viewport_bottom = view.root().height() as f32;
    let gap_below_rows = bounds.y() - last_bottom;
    let gap_to_viewport_bottom = viewport_bottom - bounds.y();
    assert!(
        gap_below_rows >= 0.0,
        "the end marker sits at y={}, above the last row's bottom edge at {last_bottom}",
        bounds.y()
    );
    // No pixel ceiling: the void case is the marker pushed to the bottom of a
    // viewport three rows cannot fill, so the marker must stay nearer to the
    // rows it closes than to that bottom edge. This holds whatever a row is
    // worth in pixels, which is exactly what the old literal could not do.
    assert!(
        gap_below_rows < gap_to_viewport_bottom,
        "three rows placed their end marker in the viewport void at y={}: \
         {gap_below_rows} px below the last row but only \
         {gap_to_viewport_bottom} px above the viewport's bottom edge at \
         {viewport_bottom}",
        bounds.y()
    );
}

fn descendant_label_with_text(widget: &gtk4::Widget, text: &str) -> Option<gtk4::Label> {
    if let Ok(label) = widget.clone().downcast::<gtk4::Label>() {
        if label.text() == text {
            return Some(label);
        }
    }
    let mut child = widget.first_child();
    while let Some(current) = child {
        if let Some(found) = descendant_label_with_text(&current, text) {
            return Some(found);
        }
        child = current.next_sibling();
    }
    None
}

#[path = "concerts_visual_tests.rs"]
mod visual_tests;

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn feed_footer_keeps_fetch_progress_below_the_live_table() {
    gtk4::init().unwrap();
    let conn = Rc::new(crate::test_db::open().unwrap());
    let runtime = ConcertsRuntime::setup(&conn);
    let view = build_view(conn, &runtime);
    let root = view.root().clone().downcast::<gtk4::Box>().unwrap();
    assert_eq!(
        root.last_child().as_ref(),
        Some(view.shared.footer.widget())
    );
    view.shared.footer.apply(FeedFooterState::NeverFetched);
    assert!(view.shared.footer.reload_is_visible());
    assert!(!view.shared.footer.progress_is_visible());
    view.shared.footer.apply(FeedFooterState::Fetching {
        checked: 2,
        total: 5,
    });
    assert!(!view.shared.footer.reload_is_visible());
    assert!(view.shared.footer.progress_is_visible());
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn reopening_a_fresh_cache_reports_checked_instead_of_loaded() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let conn = Rc::new(crate::test_db::open().unwrap());
    reprise_core::modules::set_enabled(&conn, &reprise_core::modules::CONCERTS_MODULE, true)
        .unwrap();
    reprise_core::library::settings::set_setting(
        &conn,
        reprise_core::concerts::config::TICKETMASTER_API_KEY,
        "stored-key",
    )
    .unwrap();
    crate::test_db::connection(&conn)
        .execute(
            "INSERT INTO concert_artists (
               artist_key, artist_name, last_attempt_at
             ) VALUES ('artist', 'Artist', ?1)",
            [chrono::Utc::now().timestamp()],
        )
        .unwrap();
    let runtime = ConcertsRuntime::setup(&conn);
    let view = build_view(conn, &runtime);
    view.shared.loaded_this_visit.set(true);

    view.refresh();

    assert!(view
        .shared
        .footer
        .text()
        .starts_with("Up to date — checked"));
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn conc_4c_settings_changes_re_evaluate_credentials_and_refresh_dependents() {
    gtk4::init().unwrap();
    let conn = Rc::new(crate::test_db::open().unwrap());
    reprise_core::online_sources::set_enabled(&conn, true).unwrap();
    reprise_core::modules::set_enabled(&conn, &reprise_core::modules::CONCERTS_MODULE, true)
        .unwrap();
    let runtime = ConcertsRuntime::setup(&conn);
    let view = build_view(conn.clone(), &runtime);
    let refreshes = Rc::new(Cell::new(0));
    view.set_on_refreshed({
        let refreshes = refreshes.clone();
        move || refreshes.set(refreshes.get() + 1)
    });

    reprise_core::library::settings::set_setting(
        &conn,
        reprise_core::concerts::config::TICKETMASTER_API_KEY,
        "stored-key",
    )
    .unwrap();
    runtime.notify_settings_changed();
    assert_eq!(
        view.shared.empty_state.get(),
        ConcertsEmptyState::NeverFetched
    );
    assert_eq!(view.shared.footer.text(), "Not loaded yet");
    assert!(view.shared.footer.reload_is_visible());
    assert_eq!(refreshes.get(), 1);

    reprise_core::library::settings::set_setting(
        &conn,
        reprise_core::concerts::config::TICKETMASTER_API_KEY,
        "",
    )
    .unwrap();
    runtime.notify_settings_changed();
    assert_eq!(
        view.shared.empty_state.get(),
        ConcertsEmptyState::NeverFetched
    );
    assert_eq!(view.shared.footer.text(), "Not loaded yet");
    assert!(view.shared.footer.reload_is_visible());
    assert_eq!(refreshes.get(), 2);
}

/// The Abnahme proof from `docs/plans/location-is-not-a-concerts-setting.md`
/// §3 ("The chip does not measurably filter"): the total concert count
/// without an app-wide location must equal the count with an intentionally
/// tiny radius. This drives the real `ConcertsView` refresh pipeline —
/// `render_cache()` → `filtered_events()` → the filter bar's rendered count
/// line — not `active_facets()` in isolation.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn conc_2_tiny_radius_without_a_location_never_narrows_the_shown_count() {
    gtk4::init().unwrap();
    let conn = Rc::new(crate::test_db::open().unwrap());
    insert_event(&conn, 1, "Artist One");
    insert_event(&conn, 2, "Artist Two");
    insert_event(&conn, 3, "Artist Three");
    let runtime = ConcertsRuntime::setup(&conn);
    let broadcast = Rc::new(crate::ui::location_broadcast::LocationBroadcast::default());
    let view = ConcertsView::new(conn.clone(), &runtime, &broadcast);

    view.refresh();
    assert_eq!(view.shared.rows.borrow().len(), 3);
    assert_eq!(
        view.shared.filter_bar.result_text_for_test(),
        "3 concerts",
        "without a location, the header must state the plain total"
    );

    // Persist an intentionally tiny radius the same way the real config
    // write path does, while still no app-wide location is stored.
    reprise_core::library::settings::set_setting(
        &conn,
        reprise_core::concerts::config::FILTER_RADIUS_KEY,
        "1",
    )
    .unwrap();
    runtime.notify_settings_changed();

    assert_eq!(
        view.shared.rows.borrow().len(),
        3,
        "a 1 km radius must not remove a single concert while no location is set"
    );
    assert_eq!(
        view.shared.filter_bar.result_text_for_test(),
        "3 concerts",
        "the chip must not claim a restriction it cannot enforce without a location"
    );
}

#[test]
fn nr_35_filter_changes_refresh_badge_dependents() {
    let conn = crate::test_db::open().unwrap();
    let runtime = ConcertsRuntime::setup(&conn);
    let refreshes = Rc::new(Cell::new(0));
    runtime.subscribe_settings(|| true, {
        let refreshes = refreshes.clone();
        move || refreshes.set(refreshes.get() + 1)
    });

    notify_filter_changed(&runtime);

    assert_eq!(refreshes.get(), 1);
}
