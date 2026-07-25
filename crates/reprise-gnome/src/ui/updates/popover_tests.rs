use super::*;

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
    }
}

#[test]
fn nr_5a_opening_the_popover_never_requests_navigation() {
    let effect = opening_effect(&[release("one"), release("two")]);

    assert_eq!(effect.seen_ids, ["one", "two"]);
    assert!(!effect.navigates);
}

/// The popover used to cap stamping at `POPOVER_LIMIT` (5), matching the old
/// capped row list. The list now scrolls instead of capping, so opening it
/// must stamp every listed release as seen, not just the first few.
#[test]
fn nr_9_opening_stamps_every_listed_release_seen() {
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

/// A no-op stand-in for the window-supplied navigation callback: these
/// tests exercise fetch/render/badge behavior, not NR-13 navigation (that
/// lives in `release_row.rs`'s own tests).
fn noop_show_album() -> release_row::OnShowAlbum {
    Rc::new(|_, _| {})
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
    let conn = reprise_core::db::open(None).unwrap();
    reprise_core::db::migrate(&conn).unwrap();
    conn.execute(
        "INSERT INTO new_releases (
           release_group_mbid, artist_name, artist_mbid, title, release_type,
           first_release_date, fetched_at, fallback_accent
         ) VALUES ('release', 'Artist', 'artist', 'Release', 'Album',
                   '2026-08-01', 1, '#123456')",
        [],
    )
    .unwrap();
    let conn = Rc::new(RefCell::new(conn));

    let state = NewReleasesPopover::new(conn, PathBuf::from("unused.db"), noop_show_album());

    assert!(!state.button.is_visible());
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn nr_3_header_button_is_visible_only_when_releases_exist_after_first_fetch() {
    gtk4::init().unwrap();
    let conn = reprise_core::db::open(None).unwrap();
    reprise_core::db::migrate(&conn).unwrap();
    let conn = Rc::new(RefCell::new(conn));
    let state =
        NewReleasesPopover::new(conn.clone(), PathBuf::from("unused.db"), noop_show_album());
    assert!(!state.button.is_visible());

    reprise_core::library::settings::set_new_releases_fetch_completed(&conn.borrow(), true)
        .unwrap();

    conn.borrow()
        .execute(
            "INSERT INTO new_releases (
               release_group_mbid, artist_name, artist_mbid, title, release_type,
               first_release_date, fetched_at, fallback_accent
             ) VALUES ('release', 'Artist', 'artist', 'Release', 'Album',
                       '2026-08-01', 1, '#123456')",
            [],
        )
        .unwrap();
    reprise_core::modules::set_enabled(
        &conn.borrow(),
        &reprise_core::modules::NEW_RELEASES_MODULE,
        true,
    )
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
    let conn = reprise_core::db::open(None).unwrap();
    reprise_core::db::migrate(&conn).unwrap();
    let conn = Rc::new(RefCell::new(conn));
    let state =
        NewReleasesPopover::new(conn.clone(), PathBuf::from("unused.db"), noop_show_album());
    let runtime = ArtistNewsRuntime::setup(&conn.borrow());
    bind_runtime(&state, &runtime);

    // The real user action: consent, with nothing fetched yet.
    runtime.set_enabled(&conn.borrow(), true).unwrap();

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
fn nr_9_opening_the_popover_clears_the_badge() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    if gtk4::init().is_err() {
        return;
    }
    let conn = reprise_core::db::open(None).unwrap();
    reprise_core::db::migrate(&conn).unwrap();
    reprise_core::modules::set_enabled(&conn, &reprise_core::modules::NEW_RELEASES_MODULE, true)
        .unwrap();
    reprise_core::library::settings::set_new_releases_fetch_completed(&conn, true).unwrap();
    let now = chrono::Utc::now().timestamp();
    for mbid in ["release-one", "release-two"] {
        conn.execute(
            "INSERT INTO new_releases (
               release_group_mbid, artist_name, artist_mbid, title, release_type,
               first_release_date, fetched_at, fallback_accent
             ) VALUES (?1, 'Artist', 'artist', 'Release', 'Album',
                       '2026-08-01', ?2, '#123456')",
            rusqlite::params![mbid, now],
        )
        .unwrap();
    }
    let conn = Rc::new(RefCell::new(conn));
    let state = NewReleasesPopover::new(conn, PathBuf::from("unused.db"), noop_show_album());

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
    let conn = reprise_core::db::open(None).unwrap();
    reprise_core::db::migrate(&conn).unwrap();
    reprise_core::modules::set_enabled(&conn, &reprise_core::modules::NEW_RELEASES_MODULE, true)
        .unwrap();
    reprise_core::library::settings::set_new_releases_fetch_completed(&conn, true).unwrap();
    let conn = Rc::new(RefCell::new(conn));
    let state = NewReleasesPopover::new(conn, PathBuf::from("unused.db"), noop_show_album());

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

fn find_button(widget: &gtk4::Widget) -> Option<gtk4::Button> {
    if let Ok(button) = widget.clone().downcast::<gtk4::Button>() {
        return Some(button);
    }
    let mut child = widget.first_child();
    while let Some(current) = child {
        if let Some(button) = find_button(&current) {
            return Some(button);
        }
        child = current.next_sibling();
    }
    None
}

fn find_all_buttons(widget: &gtk4::Widget, out: &mut Vec<gtk4::Button>) {
    if let Ok(button) = widget.clone().downcast::<gtk4::Button>() {
        out.push(button);
    }
    let mut child = widget.first_child();
    while let Some(current) = child {
        find_all_buttons(&current, out);
        child = current.next_sibling();
    }
}

/// C1: "Show history" navigates the stack to `HISTORY_PAGE`, and the history
/// page's back button (built fresh by `show_history` on every navigation)
/// returns it to `LIST_PAGE` — the list<->history navigation B2 stubbed out.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn nr_12_show_history_switches_the_stack_and_back_returns_to_the_list() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    if gtk4::init().is_err() {
        return;
    }
    let conn = reprise_core::db::open(None).unwrap();
    reprise_core::db::migrate(&conn).unwrap();
    let conn = Rc::new(RefCell::new(conn));
    let state = NewReleasesPopover::new(conn, PathBuf::from("unused.db"), noop_show_album());

    assert_eq!(state.stack.visible_child_name().as_deref(), Some(LIST_PAGE));

    state.show_history();

    assert_eq!(
        state.stack.visible_child_name().as_deref(),
        Some(HISTORY_PAGE),
        "Show history must switch the stack to the history page"
    );

    let back_button = find_button(state.history_page.upcast_ref::<gtk4::Widget>())
        .expect("the history page exposes a back button");
    back_button.emit_clicked();

    assert_eq!(
        state.stack.visible_child_name().as_deref(),
        Some(LIST_PAGE),
        "the history page's back button must return to the list page"
    );
}

/// C1: clicking "Restore" on a hidden entry in the history page must
/// actually un-hide it in the database (via `restore_release`) and rebuild
/// the history page — exercising the real `on_restore` wiring `show_history`
/// builds, not just the pure `history_action` mapping already covered in
/// `history_page.rs`.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn nr_12_restoring_from_the_history_page_unhides_the_release() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    if gtk4::init().is_err() {
        return;
    }
    let conn = reprise_core::db::open(None).unwrap();
    reprise_core::db::migrate(&conn).unwrap();
    conn.execute(
        "INSERT INTO new_releases (
           release_group_mbid, artist_name, artist_mbid, title, release_type,
           first_release_date, fetched_at, fallback_accent
         ) VALUES ('release', 'Artist', 'artist', 'Release', 'Album',
                   '2026-01-01', 1, '#123456')",
        [],
    )
    .unwrap();
    reprise_core::artist_news::set_release_hidden(&conn, "release", true).unwrap();
    let conn = Rc::new(RefCell::new(conn));
    let state =
        NewReleasesPopover::new(conn.clone(), PathBuf::from("unused.db"), noop_show_album());

    state.show_history();

    let mut buttons = Vec::new();
    find_all_buttons(
        state.history_page.upcast_ref::<gtk4::Widget>(),
        &mut buttons,
    );
    let restore_button = buttons
        .into_iter()
        .find(|button| button.icon_name().as_deref() == Some("view-reveal-symbolic"))
        .expect("the only (hidden) entry offers a Restore action");

    restore_button.emit_clicked();

    let hidden = reprise_core::artist_news::hidden_release_count(&conn.borrow()).unwrap();
    assert_eq!(hidden, 0, "clicking Restore must un-hide the release");
}
