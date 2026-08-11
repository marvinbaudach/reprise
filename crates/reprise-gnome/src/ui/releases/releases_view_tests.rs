use super::*;

fn history_entry(title: &str, artist: &str) -> HistoryEntry {
    HistoryEntry {
        release_group_mbid: format!("mbid-{title}"),
        artist_name: artist.to_owned(),
        title: title.to_owned(),
        release_type: "Album".into(),
        first_release_date: "2026-08-05".into(),
        first_seen: None,
        seen_at: None,
        hidden: false,
        hidden_at: None,
        presence: reprise_core::artist_news::LibraryPresence::Absent,
        announce_url: None,
        track_count: None,
        local_track_count: 0,
    }
}

/// UX FIL-1d: the Releases query matches **title and artist** — the two
/// fields its chip names — case-insensitively and mid-word.
#[test]
fn fil_1d_releases_query_matches_title_and_artist_only() {
    let rows = vec![
        history_entry("Pain Remains", "Lorna Shore"),
        history_entry("Antwerpen Sessions", "Quiet Hands"),
        history_entry("Elsewhere", "Sanguisugabogg"),
    ];

    let titles = |query: &str| {
        releases_matching(rows.clone(), query)
            .into_iter()
            .map(|entry| entry.title)
            .collect::<Vec<_>>()
    };

    assert_eq!(titles("wer"), ["Antwerpen Sessions"]);
    assert_eq!(titles("LORNA"), ["Pain Remains"]);
    assert_eq!(titles("remains"), ["Pain Remains"]);
    assert_eq!(titles("").len(), 3, "an empty query withholds nothing");
    assert!(titles("cattle").is_empty());
}

#[test]
fn fil_3a_releases_only_report_facets_that_can_hide_rows() {
    let mut filter = reprise_core::artist_news::ReleasesFilter::widest(false);
    assert!(!release_facets_restrict(&filter));

    filter.hidden = true;
    assert!(
        !release_facets_restrict(&filter),
        "including hidden releases broadens the scope rather than restricting it"
    );

    filter.hidden = false;
    filter.release_types = reprise_core::artist_news::ReleaseTypeSelection::default();
    assert!(release_facets_restrict(&filter));
}

fn pump_until(label: &str, condition: impl Fn() -> bool) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !condition() {
        while gtk4::glib::MainContext::default().iteration(false) {}
        assert!(std::time::Instant::now() < deadline, "timed out: {label}");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn nr_20_releases_view_exposes_filters_seven_columns_and_footer() {
    gtk4::init().unwrap();
    let conn = Rc::new(crate::test_db::open().unwrap());
    let view = ReleasesView::new(conn, PathBuf::new());
    let root = view.root().clone().downcast::<gtk4::Box>().unwrap();
    assert_eq!(root.observe_children().n_items(), 4);
    let stack = root
        .first_child()
        .and_then(|child| child.next_sibling())
        .and_then(|child| child.next_sibling())
        .and_downcast::<gtk4::Stack>()
        .unwrap();
    let table = stack
        .child_by_name(LIST_PAGE)
        .and_downcast::<gtk4::Overlay>()
        .and_then(|overlay| overlay.child())
        .and_downcast::<gtk4::ScrolledWindow>()
        .and_then(|scrolled| scrolled.child())
        .and_downcast::<gtk4::ColumnView>()
        .unwrap();
    let columns = table.columns();
    assert_eq!(columns.n_items(), 7);
    let cover = columns
        .item(0)
        .and_downcast::<gtk4::ColumnViewColumn>()
        .unwrap();
    assert!(
        cover.id().is_none(),
        "the pinned leading cover column must remain id-less"
    );
    let cover_title = strings::text(strings::COLUMN_COVER);
    assert_eq!(
        cover.title().as_deref(),
        Some(cover_title.as_str()),
        "the id-less leading column must be the pinned cover"
    );
    assert_eq!(
        columns
            .item(1)
            .and_downcast::<gtk4::ColumnViewColumn>()
            .unwrap()
            .id()
            .as_deref(),
        Some("date"),
        "Date must be the first id-carrying column"
    );
}

fn insert_release(conn: &Db, mbid: &str, title: &str) {
    crate::test_db::connection(conn)
        .execute(
            "INSERT INTO new_releases (
               release_group_mbid, artist_name, artist_mbid, title, release_type,
               first_release_date, fetched_at
             ) VALUES (?1, 'Artist', 'artist-id', ?2, 'Album', '2026-08-05', 1)",
            rusqlite::params![mbid, title],
        )
        .unwrap();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn nr_32_deleted_release_memory_is_reflected_in_releases_view() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let conn = Rc::new(crate::test_db::open().unwrap());
    insert_release(&conn, "deleted", "Deleted Album");
    crate::test_db::connection(&conn)
        .execute(
            "INSERT INTO tracks (
               id, path, title, artist, album_artist, album, added_at
             ) VALUES (1, '/music/deleted.flac', 'Deleted Song',
                       'Artist', 'Artist', 'Deleted Album', 0)",
            [],
        )
        .unwrap();
    reprise_core::queries::exclude_tracks_matching_paths(
        &conn,
        &[(1, PathBuf::from("/music/deleted.flac"))],
        100,
    )
    .unwrap();

    let view = ReleasesView::new(conn.clone(), PathBuf::new());
    let window = gtk4::Window::new();
    window.set_default_size(900, 600);
    window.set_child(Some(view.root()));
    window.present();
    view.refresh();
    crate::ui::source_context_surface::settle_layout();

    assert_eq!(view.shared.model.store().n_items(), 0);
    assert_eq!(
        reprise_core::artist_news::hidden_release_count(&conn).unwrap(),
        1
    );
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
fn fil_3a_releases_end_line_counts_gaps_and_recovers_with_clear_all() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let conn = Rc::new(crate::test_db::open().unwrap());
    insert_release(&conn, "afd", "Afd release");
    insert_release(&conn, "other", "Different release");
    crate::test_db::connection(&conn)
        .execute(
            "INSERT INTO tracks (path, title, artist, album_artist, album, play_count, added_at)
             VALUES ('/music/artist.flac', 'Song', 'Artist', 'Artist', 'Owned', 0, 0)",
            [],
        )
        .unwrap();
    let view = ReleasesView::new(conn, PathBuf::new());
    let window = gtk4::Window::new();
    window.set_default_size(900, 600);
    window.set_child(Some(view.root()));
    window.present();

    view.shared.filter_bar.show_widest();
    view.set_search_query("afd");
    crate::ui::source_context_surface::settle_layout();
    assert_eq!(view.shared.model.store().n_items(), 1);
    assert_eq!(
        view.shared.stack.visible_child_name().as_deref(),
        Some(LIST_PAGE)
    );

    let line = descendant_with_class::<gtk4::Label>(
        view.root(),
        crate::ui::end_of_results::LINE_CSS_CLASS,
    )
    .expect("Releases owns the shared end-of-results line");
    assert_eq!(line.text(), "End of results — 1 gap hidden by search “afd”");
    assert!(line.is_visible());
    let recovery = descendant_with_class::<gtk4::Button>(
        view.root(),
        crate::ui::end_of_results::RECOVERY_CSS_CLASS,
    )
    .expect("Releases owns the shared recovery pill");
    assert_eq!(recovery.label().as_deref(), Some("Show all 2 gaps"));
    recovery.emit_clicked();
    crate::ui::source_context_surface::settle_layout();
    assert_eq!(view.shared.filter_bar.query(), "");
    assert!(!line.is_visible());
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn nr_22_fetch_action_replaces_stale_age_with_progress_then_updates_completion_age() {
    gtk4::init().unwrap();
    let conn = Rc::new(crate::test_db::open().unwrap());
    reprise_core::modules::set_enabled(&conn, &reprise_core::modules::NEW_RELEASES_MODULE, true)
        .unwrap();
    reprise_core::library::settings::set_new_releases_last_completed_at(
        &conn,
        chrono::Utc::now().timestamp() - 360,
    )
    .unwrap();

    let path = conn.path().unwrap();
    let view = ReleasesView::new(conn, path);
    view.shared
        .fetch_override
        .replace(Some(std::sync::Arc::new(|path, publish| {
            publish(artist_news::RefreshProgress {
                checked: 0,
                total: 1,
            });
            std::thread::sleep(std::time::Duration::from_millis(250));
            let db = Db::open_migrated(Some(path))
                .map_err(|error| artist_news::NewsError::Database(error.to_string()))?;
            reprise_core::library::settings::set_new_releases_last_completed_at(
                &db,
                chrono::Utc::now().timestamp(),
            )
            .map_err(|error| artist_news::NewsError::Database(error.to_string()))?;
            publish(artist_news::RefreshProgress {
                checked: 1,
                total: 1,
            });
            Ok(artist_news::RefreshReport {
                artists_queued: 1,
                artists_fetched: 1,
                ..artist_news::RefreshReport::default()
            })
        })));
    view.refresh();
    let stale_age = view.shared.updated.text();
    assert!(stale_age.contains("6 min ago"), "{stale_age}");

    view.shared.fetch_button.emit_clicked();
    assert!(!view.shared.updated.is_visible());
    assert_eq!(
        view.shared.fetch_stack.visible_child_name().as_deref(),
        Some(FETCH_SPINNER_PAGE)
    );
    pump_until("determinate release progress", || {
        view.shared.progress.text().as_deref() == Some("Checked 0 of 1 artists")
    });
    assert!(view.shared.progress.is_visible());

    pump_until("release fetch completion", || !view.shared.fetching.get());
    assert!(!view.shared.progress.is_visible());
    assert!(view.shared.updated.is_visible());
    assert_eq!(view.shared.updated.text().as_str(), "Updated just now");
    assert!(view.shared.fetch_button.is_sensitive());
    assert_eq!(
        view.shared.fetch_stack.visible_child_name().as_deref(),
        Some(FETCH_ICON_PAGE)
    );
}
