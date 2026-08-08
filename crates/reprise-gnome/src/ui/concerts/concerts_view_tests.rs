use super::*;

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn conc_3_concerts_view_exposes_six_columns_and_row_activation() {
    gtk4::init().unwrap();
    let conn = Rc::new(crate::test_db::open().unwrap());
    let runtime = ConcertsRuntime::setup(&conn);
    let view = ConcertsView::new(conn, &runtime);
    let root = view.root().clone().downcast::<gtk4::Box>().unwrap();
    let stack = root
        .first_child()
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
    assert_eq!(table.columns().n_items(), 6);
    assert!(!table.enables_rubberband());
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
    let view = ConcertsView::new(conn, &runtime);
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
fn conc_5a_footer_keeps_fetch_progress_below_the_live_table() {
    gtk4::init().unwrap();
    let conn = Rc::new(crate::test_db::open().unwrap());
    let runtime = ConcertsRuntime::setup(&conn);
    let view = ConcertsView::new(conn, &runtime);
    let root = view.root().clone().downcast::<gtk4::Box>().unwrap();
    let footer = root.last_child().and_downcast::<gtk4::Box>().unwrap();
    let fetch_stack = footer.last_child().and_downcast::<gtk4::Stack>().unwrap();
    assert!(fetch_stack.child_by_name(FETCH_BUTTON_PAGE).is_some());
    assert!(fetch_stack.child_by_name(FETCH_SPINNER_PAGE).is_some());
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn conc_4b_settings_changes_re_evaluate_credentials_and_refresh_dependents() {
    gtk4::init().unwrap();
    let conn = Rc::new(crate::test_db::open().unwrap());
    let runtime = ConcertsRuntime::setup(&conn);
    let view = ConcertsView::new(conn.clone(), &runtime);
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
    assert!(!view.shared.fetch_stack.is_visible());

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
    assert!(view.shared.fetch_stack.is_visible());
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
    assert!(!view.shared.fetch_stack.is_visible());
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
