use super::*;
use reprise_core::db::Db;

fn release(id: &str) -> reprise_core::artist_news::StoredRelease {
    reprise_core::artist_news::StoredRelease {
        release_group_mbid: id.into(),
        artist_name: "Artist".into(),
        artist_mbid: "artist-id".into(),
        title: "Release".into(),
        release_type: "Album".into(),
        first_release_date: "2026-08-01".into(),
        fetched_at: 100,
        seen_at: None,
        hidden: false,
        fallback_accent: "#123456".into(),
        presence: reprise_core::artist_news::LibraryPresence::Absent,
        announce_url: None,
        track_count: None,
        local_track_count: 0,
    }
}

#[test]
fn nr_5b_opening_the_popover_never_requests_navigation() {
    let effect = opening_effect(&[release("one"), release("two")]);

    assert_eq!(effect.seen_ids, ["one", "two"]);
    assert!(!effect.navigates);
}

/// The popover used to cap stamping at `POPOVER_LIMIT` (5), matching the old
/// capped row list. The list now scrolls instead of capping, so opening it
/// must stamp every listed release as seen, not just the first few.
#[test]
fn nr_9a_opening_stamps_every_listed_release_seen() {
    let releases: Vec<_> = (1..=7).map(|n| release(&format!("release-{n}"))).collect();

    let effect = opening_effect(&releases);

    assert_eq!(effect.seen_ids.len(), 7);
    assert_eq!(
        effect.seen_ids,
        releases
            .iter()
            .map(|release| release.release_group_mbid.clone())
            .collect::<Vec<_>>()
    );
}

#[test]
fn nr_6_failure_keeps_updated_age_with_an_inline_cached_hint() {
    let presentation = footer_presentation(Some(100), 3_700, true);

    assert_eq!(presentation.updated, "Updated 1 h ago");
    assert!(presentation.show_cached_failure);
}

#[test]
fn shared_footer_uses_the_oldest_active_feed_and_names_failures() {
    assert_eq!(
        oldest_active_feed_timestamp(true, Some(200), true, Some(100)),
        Some(100)
    );
    assert_eq!(
        oldest_active_feed_timestamp(true, Some(200), true, None),
        None
    );
    assert_eq!(
        oldest_active_feed_timestamp(true, Some(200), false, None),
        Some(200)
    );
    assert!(fetch_failure_text(false, true).contains("Concerts"));
    let both = fetch_failure_text(true, true);
    assert!(both.contains("saved releases") && both.contains("Concerts"));
}

/// A no-op stand-in for the window-supplied navigation callback: these
/// tests exercise fetch/render/badge behavior, not NR-13 navigation (that
/// lives in `release_row.rs`'s own tests).
fn noop_show_album() -> release_row::OnShowAlbum {
    Rc::new(|_, _| {})
}

fn test_popover(conn: Rc<Db>, database_path: PathBuf) -> Rc<NewReleasesPopover> {
    let concerts_runtime = ConcertsRuntime::setup(&conn);
    NewReleasesPopover::new(
        conn,
        database_path,
        concerts_runtime,
        noop_show_album(),
        Rc::new(|_| {}),
    )
}

#[test]
fn periodic_fetch_due_is_true_only_when_enabled_idle_and_due() {
    assert!(periodic_fetch_due(true, false, true));
}

#[test]
fn periodic_fetch_due_is_false_while_disabled() {
    assert!(!periodic_fetch_due(false, false, true));
    assert!(!periodic_fetch_due(false, true, true));
    assert!(!periodic_fetch_due(false, false, false));
}

#[test]
fn periodic_fetch_due_is_false_while_a_fetch_is_already_running() {
    assert!(!periodic_fetch_due(true, true, true));
}

#[test]
fn periodic_fetch_due_is_false_when_not_yet_due() {
    assert!(!periodic_fetch_due(true, false, false));
}

#[test]
fn nr_7_disabled_module_hides_the_button_and_blocks_fetch() {
    assert_eq!(
        module_effect(false, true, true, false),
        ModuleEffect {
            button_visible: false,
            fetch_allowed: false,
            empty: EmptyPresentation::Hidden,
            badge_allowed: false,
        }
    );
    assert_eq!(
        module_effect(true, false, true, false),
        ModuleEffect {
            button_visible: false,
            fetch_allowed: true,
            empty: EmptyPresentation::NoReleases,
            badge_allowed: false,
        }
    );
    assert_eq!(
        module_effect(true, true, true, false),
        ModuleEffect {
            button_visible: true,
            fetch_allowed: true,
            empty: EmptyPresentation::Hidden,
            badge_allowed: true,
        }
    );
}

#[test]
fn nr_8_first_fetch_and_retry_keep_feedback_reachable_without_a_badge() {
    assert_eq!(
        module_effect(true, false, false, true),
        ModuleEffect {
            button_visible: true,
            fetch_allowed: true,
            empty: EmptyPresentation::Checking,
            badge_allowed: false,
        }
    );
    assert_eq!(
        module_effect(true, false, false, false),
        ModuleEffect {
            button_visible: true,
            fetch_allowed: true,
            empty: EmptyPresentation::NoReleases,
            badge_allowed: false,
        }
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn nr_7_header_button_stays_hidden_with_cached_releases_while_disabled() {
    gtk4::init().unwrap();
    let conn = crate::test_db::open().unwrap();
    crate::test_db::connection(&conn)
        .execute(
            "INSERT INTO new_releases (
           release_group_mbid, artist_name, artist_mbid, title, release_type,
           first_release_date, fetched_at, fallback_accent
         ) VALUES ('release', 'Artist', 'artist', 'Release', 'Album',
                   '2026-08-01', 1, '#123456')",
            [],
        )
        .unwrap();
    let conn = Rc::new(conn);

    let state = test_popover(conn, PathBuf::from("unused.db"));

    assert!(!state.button.is_visible());
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn nr_3a_header_button_is_visible_only_when_releases_exist_after_first_fetch() {
    gtk4::init().unwrap();
    let conn = Rc::new(crate::test_db::open().unwrap());
    let state = test_popover(conn.clone(), PathBuf::from("unused.db"));
    assert!(!state.button.is_visible());

    reprise_core::library::settings::set_new_releases_fetch_completed(&conn, true).unwrap();

    crate::test_db::connection(&conn)
        .execute(
            "INSERT INTO new_releases (
               release_group_mbid, artist_name, artist_mbid, title, release_type,
               first_release_date, fetched_at, fallback_accent
             ) VALUES ('release', 'Artist', 'artist', 'Release', 'Album',
                       '2026-08-01', 1, '#123456')",
            [],
        )
        .unwrap();
    reprise_core::modules::set_enabled(&conn, &reprise_core::modules::NEW_RELEASES_MODULE, true)
        .unwrap();
    state.render(false, false);

    assert!(state.button.is_visible());
}

/// The reachability gap NR-8 closes. Every other test here enables the
/// module *after* inserting a release, so none of them walks the path a
/// real user takes: switch the plugin on while the table is still empty.
/// On that path the sparkle never appears, "Fetch now" lives inside the
/// popover behind it, and nothing else requests a fetch — so the feature
/// can never populate itself. Green rule-by-rule tests missed this because
/// the defect sits between the rules, not inside one.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn nr_8_enabling_the_module_reaches_a_fetch() {
    gtk4::init().unwrap();
    let conn = Rc::new(crate::test_db::open().unwrap());
    let state = test_popover(conn.clone(), PathBuf::from("unused.db"));
    let runtime = ArtistNewsRuntime::setup(&conn);
    bind_runtime(&state, &runtime);

    // The real user action: consent, with nothing fetched yet.
    runtime.set_enabled(&conn, true).unwrap();

    assert!(
        state.button.is_visible(),
        "enabling the module must leave a reachable entry point, otherwise \
         the user consents and nothing can ever happen"
    );
    assert!(state.fetching.get(), "enabling starts the first fetch");
    assert_eq!(
        state.empty.text(),
        strings::text(strings::NEW_RELEASES_CHECKING)
    );
    assert!(!state.badge.is_visible());
}

/// NR-9: opening the popover stamps every listed release seen, so the badge
/// (visible beforehand for the unseen releases) must be gone afterwards.
/// `render(true, ..)` is called directly rather than emitting the popover's
/// real "show" signal: the popover here is never parented under a realized
/// toplevel (no test in this file maps one), and GTK's real show handling
/// tries to create a native surface for it, which segfaults without one.
/// `connect_show` in `wire()` calls exactly this method first, so this
/// exercises the same production code path the real signal would.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn nr_9a_opening_the_popover_clears_the_badge() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    if gtk4::init().is_err() {
        return;
    }
    let conn = crate::test_db::open().unwrap();
    reprise_core::modules::set_enabled(&conn, &reprise_core::modules::NEW_RELEASES_MODULE, true)
        .unwrap();
    reprise_core::library::settings::set_new_releases_fetch_completed(&conn, true).unwrap();
    let now = chrono::Utc::now().timestamp();
    for mbid in ["release-one", "release-two"] {
        crate::test_db::connection(&conn)
            .execute(
                "INSERT INTO new_releases (
               release_group_mbid, artist_name, artist_mbid, title, release_type,
               first_release_date, fetched_at, fallback_accent
             ) VALUES (?1, 'Artist', 'artist', 'Release', 'Album',
                       '2026-08-01', ?2, '#123456')",
                rusqlite::params![mbid, now],
            )
            .unwrap();
    }
    let conn = Rc::new(conn);
    let state = test_popover(conn, PathBuf::from("unused.db"));

    assert!(
        state.badge.is_visible(),
        "two unseen releases should badge before the popover ever opens"
    );

    state.render(true, false);

    assert!(
        !state.badge.is_visible(),
        "opening stamps every listed release seen, so the badge must clear"
    );
}

/// B5: the hourly background staleness timer's lifecycle is coupled to the
/// enabled subscription, not to the popover being open. `fetch_completed` is
/// set beforehand so `enabled_changed(true)` takes the `render` branch rather
/// than `fetch_now` — this test only checks the timer field, not a real
/// fetch, and must not spawn a worker thread that reaches the network.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn enabling_starts_the_refresh_timer_and_disabling_stops_it() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    if gtk4::init().is_err() {
        return;
    }
    let conn = crate::test_db::open().unwrap();
    reprise_core::modules::set_enabled(&conn, &reprise_core::modules::NEW_RELEASES_MODULE, true)
        .unwrap();
    reprise_core::library::settings::set_new_releases_fetch_completed(&conn, true).unwrap();
    let conn = Rc::new(conn);
    let state = test_popover(conn, PathBuf::from("unused.db"));

    assert!(
        !state.has_active_timer(),
        "no timer runs before the module is known to be enabled"
    );

    state.enabled_changed(true);

    assert!(
        state.has_active_timer(),
        "enabling the module must start the hourly staleness timer"
    );
    assert!(
        !state.fetching.get(),
        "fetch_completed was set beforehand, so this must not have started a fetch"
    );

    state.enabled_changed(false);

    assert!(
        !state.has_active_timer(),
        "disabling the module must stop the hourly staleness timer"
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn nr_5b_jump_rows_route_to_full_views_without_an_internal_page() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    if gtk4::init().is_err() {
        return;
    }
    let conn = Rc::new(crate::test_db::open().unwrap());
    let routed = Rc::new(RefCell::new(Vec::new()));
    let on_open_view: OnOpenView = {
        let routed = routed.clone();
        Rc::new(move |target| routed.borrow_mut().push(target))
    };
    let runtime = ConcertsRuntime::setup(&conn);
    let state = NewReleasesPopover::new(
        conn,
        PathBuf::from("unused.db"),
        runtime,
        noop_show_album(),
        on_open_view,
    );

    state.releases_jump.emit_clicked();
    state.concerts_jump.emit_clicked();

    assert_eq!(
        routed.borrow().as_slice(),
        [
            reprise_core::browser::navigation::SidebarTarget::Releases,
            reprise_core::browser::navigation::SidebarTarget::Concerts,
        ]
    );
}
