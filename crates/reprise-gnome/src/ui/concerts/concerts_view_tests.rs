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
fn concerts_view_exposes_seven_columns_and_row_activation() {
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
    assert_eq!(table.columns().n_items(), 7);
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
fn conc_4b_app_location_broadcast_re_evaluates_the_open_view() {
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

fn insert_event(conn: &Db, id: i64, artist: &str) {
    crate::test_db::connection(conn)
        .execute(
            "INSERT INTO concert_events (
               id, artist_key, artist_name, starts_at, date_key, venue, city,
               country, provider, fetched_at, dedupe_key
             ) VALUES (?1, ?2, ?3, '2026-08-20T19:00:00', '2026-08-20',
                       'Venue', 'Zurich', 'CH', 'bandsintown', 1, ?4)",
            rusqlite::params![id, format!("artist-{id}"), artist, format!("event-{id}")],
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
    let bounds = line.compute_bounds(view.root()).unwrap();
    assert!(
        bounds.y() < 260.0,
        "three rows placed their end marker in the viewport void at y={} instead of below the rows",
        bounds.y()
    );
}

fn visual_fixture_row(
    id: i64,
    artist: &str,
    venue: &str,
    city: &str,
    distance_km: f64,
    availability: reprise_core::concerts::TicketAvailability,
    similar_to: Option<&str>,
) -> ConcertRow {
    ConcertRow {
        id,
        availability,
        date_key: format!("2026-08-{}", 19 + id),
        starts_at: format!("2026-08-{}T19:30:00", 19 + id),
        artist_name: artist.into(),
        venue: venue.into(),
        city: city.into(),
        region: None,
        country: Some("CH".into()),
        latitude: None,
        longitude: None,
        distance_km: Some(distance_km),
        ticket_url: Some(format!("https://tickets.example/{id}")),
        ticket_source: Some(if id % 2 == 0 {
            "Bandsintown".into()
        } else {
            "Ticketmaster".into()
        }),
        event_url: Some(format!("https://events.example/{id}")),
        provider: "fixture".into(),
        is_similar: similar_to.is_some(),
        similar_to: similar_to.map(str::to_owned),
    }
}

#[test]
#[ignore = "visual fixture; run through the isolated CUA session"]
fn concerts_visual_acceptance_fixture() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    libadwaita::StyleManager::default().set_color_scheme(libadwaita::ColorScheme::ForceLight);
    crate::ui::style::install_css_string_for_test(&format!(
        "{}\n{}",
        crate::ui::style::theme::theme_css(
            crate::ui::style::theme::Theme::DEFAULT,
            false,
            crate::ui::style::accent::AccentSource::App,
        ),
        crate::ui::style::app_css_for_test(),
    ));

    let conn = Rc::new(crate::test_db::open().unwrap());
    reprise_core::location::store(&conn, 47.3769, 8.5417, "Zürich", Some("CH")).unwrap();
    reprise_core::library::settings::set_setting(
        &conn,
        reprise_core::concerts::config::FILTER_RADIUS_KEY,
        "500",
    )
    .unwrap();
    let runtime = ConcertsRuntime::setup(&conn);
    let view = build_view(conn, &runtime);
    let rows = vec![
        visual_fixture_row(
            1,
            "Lorna Shore",
            "Hallenstadion",
            "Zürich",
            14.8,
            reprise_core::concerts::TicketAvailability::OnSale,
            None,
        ),
        visual_fixture_row(
            2,
            "Architects",
            "Komplex 457",
            "Zürich",
            28.0,
            reprise_core::concerts::TicketAvailability::OffSale,
            Some("Bring Me the Horizon"),
        ),
        visual_fixture_row(
            3,
            "Spiritbox",
            "Z7 Konzertfabrik",
            "Pratteln",
            500.0,
            reprise_core::concerts::TicketAvailability::Unknown,
            None,
        ),
        visual_fixture_row(
            4,
            "Sleep Token",
            "Olympiahalle",
            "München",
            500.1,
            reprise_core::concerts::TicketAvailability::OnSale,
            None,
        ),
        visual_fixture_row(
            5,
            "Bad Omens",
            "Mercedes-Benz Arena",
            "Berlin",
            850.0,
            reprise_core::concerts::TicketAvailability::OffSale,
            None,
        ),
    ];
    let mode = std::env::var("REPRISE_SMOKE_CONCERTS_FIXTURE").unwrap_or_else(|_| "table".into());
    let shown_rows = if mode == "end" {
        rows.into_iter().take(3).collect()
    } else {
        rows
    };
    view.shared.model.replace(shown_rows);
    view.shared.stack.set_visible_child_name(LIST_PAGE);
    if mode == "source" {
        view.column_model().set_visible("source", true);
    }
    let now = chrono::Utc::now().timestamp();
    view.shared.footer.apply(match mode.as_str() {
        "cached" => FeedFooterState::Cached { at: now },
        "fetching" => FeedFooterState::Fetching {
            checked: 137,
            total: 415,
        },
        "failed" => FeedFooterState::Failed { latest: now },
        "offline" => FeedFooterState::Offline { latest: now },
        _ => FeedFooterState::Loaded { at: now },
    });

    let window = gtk4::Window::builder()
        .title("Reprise Concerts visual acceptance")
        .default_width(1380)
        .default_height(760)
        .child(view.root())
        .build();
    window.present();
    crate::ui::source_context_surface::settle_layout();
    if mode == "end" {
        view.shared
            .end_of_results
            .update(super::super::concerts_end_of_results::Input {
                shown: 3,
                total: 415,
                query: String::new(),
                facets_restrict: true,
                radius_km: Some(500.0),
                city: Some("Zürich".into()),
            });
        crate::ui::source_context_surface::settle_layout();
    }
    let hold_ms = std::env::var("REPRISE_SMOKE_HOLD_MS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(250);
    gtk4::glib::MainContext::default().block_on(gtk4::glib::timeout_future(
        std::time::Duration::from_millis(hold_ms),
    ));
    window.close();
}

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

    view.refresh();
    assert_eq!(
        view.shared.empty_state.get(),
        ConcertsEmptyState::NoCredentials
    );
    assert_eq!(
        view.shared.footer.text(),
        "Concerts needs provider credentials"
    );
    assert!(!view.shared.footer.reload_is_visible());

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
        ConcertsEmptyState::NoCredentials
    );
    assert_eq!(
        view.shared.footer.text(),
        "Concerts needs provider credentials"
    );
    assert!(!view.shared.footer.reload_is_visible());
    assert_eq!(refreshes.get(), 2);
}

#[test]
fn conc_7_filter_changes_refresh_badge_dependents() {
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
