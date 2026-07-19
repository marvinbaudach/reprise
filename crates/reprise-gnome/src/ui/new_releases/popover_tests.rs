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
    }
}

#[test]
fn nr_5_opening_the_popover_never_requests_navigation() {
    let effect = opening_effect(&[release("one"), release("two")]);

    assert_eq!(effect.seen_ids, ["one", "two"]);
    assert!(!effect.navigates);
}

#[test]
fn nr_6_failure_keeps_updated_age_with_an_inline_cached_hint() {
    let presentation = footer_presentation(Some(100), 3_700, true);

    assert_eq!(presentation.updated, "Updated 1 h ago");
    assert!(presentation.show_cached_failure);
}

#[test]
fn nr_4_see_all_appears_for_overflow_or_hidden_entries() {
    assert!(!see_all_visible(5, 5, 0));
    assert!(see_all_visible(6, 5, 0));
    assert!(see_all_visible(5, 5, 1));
}

/// UX NR-4: hiding has to be reachable from the popover itself.
///
/// The short-list case above (`!see_all_visible(5, 5, 0)`) is exactly the
/// state in which the digest view is unreachable — so if "Hide" lived
/// only there, a user with few releases could never hide one, and the
/// digest's "N hidden · Show" footer could never appear for them. The row
/// carries the action, which closes that loop.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn nr_4_popover_rows_offer_hide_without_the_digest_view() {
    if gtk4::init().is_err() {
        return;
    }
    let hidden: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    let sink = hidden.clone();
    let on_hide: Rc<dyn Fn(&str)> = Rc::new(move |mbid: &str| {
        sink.borrow_mut().push(mbid.to_string());
    });

    let row = build_release_row(&release("rg-sample"), false, &on_hide);

    let button =
        last_button(row.upcast_ref::<gtk4::Widget>()).expect("a popover row exposes a Hide button");
    button.emit_clicked();
    assert_eq!(hidden.borrow().as_slice(), ["rg-sample"]);
}

fn last_button(widget: &gtk4::Widget) -> Option<gtk4::Button> {
    let mut found = None;
    let mut child = widget.first_child();
    while let Some(current) = child {
        if let Ok(button) = current.clone().downcast::<gtk4::Button>() {
            found = Some(button);
        }
        child = current.next_sibling();
    }
    found
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

    let state = NewReleasesPopover::new(conn, PathBuf::from("unused.db"), Rc::new(|| {}));

    assert!(!state.button.is_visible());
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn nr_3_header_button_is_visible_only_when_releases_exist_after_first_fetch() {
    gtk4::init().unwrap();
    let conn = reprise_core::db::open(None).unwrap();
    reprise_core::db::migrate(&conn).unwrap();
    let conn = Rc::new(RefCell::new(conn));
    let state = NewReleasesPopover::new(conn.clone(), PathBuf::from("unused.db"), Rc::new(|| {}));
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
    let state = NewReleasesPopover::new(conn.clone(), PathBuf::from("unused.db"), Rc::new(|| {}));
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
