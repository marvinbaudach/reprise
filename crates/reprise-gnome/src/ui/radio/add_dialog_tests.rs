use super::*;

#[test]
fn src_3a_radio_add_dialog_submits_search_or_url_through_one_field() {
    assert_eq!(
        classify_input("ambient radio"),
        AddInput::Search("ambient radio".into())
    );
    assert_eq!(
        classify_input("https://radio.example/listen.pls"),
        AddInput::Url("https://radio.example/listen.pls".into())
    );
    assert_eq!(classify_input("   "), AddInput::Empty);
}

#[test]
fn dialog_state_ignores_stale_results_and_requires_a_valid_preview() {
    let state = AddDialogState::default();
    let (state, first) = state.begin(&AddInput::Search("metal".into()));
    let (state, second) = state.begin(&AddInput::Url("https://radio.example/live".into()));
    assert!(matches!(state.phase, AddDialogPhase::Previewing));
    assert_eq!(
        state.clone().accept(first, AddResult::Search(Vec::new())),
        state
    );
    let preview = StationPreview::manual("Example", "https://radio.example/live");
    let accepted = state.accept(second, AddResult::Preview(preview));
    assert!(matches!(accepted.phase, AddDialogPhase::Preview(_)));
    assert!(accepted.can_confirm());
}

#[test]
fn src_5_radio_search_hides_existing_favorites() {
    let candidates = vec![
        StationCandidate {
            uuid: "existing".into(),
            name: "Existing".into(),
            url_resolved: "https://radio.test/existing/".into(),
            codec: None,
            bitrate_kbps: None,
            country_code: None,
            genre: None,
            tags: Vec::new(),
            votes: 1,
            favicon_url: Some("https://radio.test/existing.png".into()),
        },
        StationCandidate {
            uuid: "new".into(),
            name: "New".into(),
            url_resolved: "https://radio.test/new".into(),
            codec: None,
            bitrate_kbps: None,
            country_code: None,
            genre: None,
            tags: Vec::new(),
            votes: 2,
            favicon_url: Some("https://radio.test/new.png".into()),
        },
    ];

    let visible = radio::search::filter_new_stations(
        candidates,
        &[("existing".into(), "https://radio.test/existing".into())],
    );

    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].uuid, "new");
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn src_7_a_successful_radio_add_acknowledges_the_row_in_place() {
    gtk4::init().unwrap();
    let conn = Rc::new(RefCell::new(reprise_core::db::open_migrated(None).unwrap()));
    let dialog = RadioAddDialog::new(conn, || {});
    dialog.render_results(vec![StationCandidate {
        uuid: "new".into(),
        name: "New".into(),
        url_resolved: "https://radio.test/new".into(),
        codec: None,
        bitrate_kbps: None,
        country_code: None,
        genre: None,
        tags: Vec::new(),
        votes: 2,
        favicon_url: None,
    }]);

    let row = dialog
        .widgets
        .results
        .first_child()
        .and_downcast::<gtk4::ListBoxRow>()
        .expect("search result row");
    let button = row
        .child()
        .and_downcast::<gtk4::Box>()
        .and_then(|content| content.last_child())
        .and_downcast::<gtk4::Button>()
        .expect("add button");
    button.emit_clicked();

    // SRC-7: the row stays and its action becomes an inactive acknowledgement,
    // so the add is visible; only the next submitted search drops it (SRC-5).
    assert!(
        dialog.widgets.results.first_child().is_some(),
        "the result row must survive a successful add"
    );
    assert!(
        !button.is_sensitive(),
        "the acknowledged action must not be pressable again"
    );
    assert!(button.has_css_class("reprise-source-added"));
}

/// `SRC-11` / `NET-1a`: the method both radio favicon call sites
/// (`render_results`, `render_preview`) use to gate their source image must
/// reflect both the global online-sources switch and the Source Images
/// module.
#[test]
fn src_11_radio_add_dialog_images_allowed_is_the_net_1a_and() {
    let conn = reprise_core::db::open_migrated(None).unwrap();
    // Neither the global gate nor the module is on by default.
    assert!(!images_allowed(&conn));

    reprise_core::modules::set_enabled(&conn, &reprise_core::modules::SOURCE_IMAGES_MODULE, true)
        .unwrap();
    assert!(
        images_allowed(&conn),
        "module on, global on (default) => allowed"
    );

    reprise_core::online_sources::set_enabled(&conn, false).unwrap();
    assert!(!images_allowed(&conn), "module on, global off => blocked");
}

/// `SRC-11` / `NET-1a`: with the gate closed, a search-result favicon must
/// stay on the glyph fallback — nothing is requested.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn src_11_radio_search_result_stays_on_the_fallback_when_images_are_not_allowed() {
    gtk4::init().unwrap();
    let conn = Rc::new(RefCell::new(reprise_core::db::open_migrated(None).unwrap()));
    let dialog = RadioAddDialog::new(conn, || {});
    dialog.render_results(vec![StationCandidate {
        uuid: "new".into(),
        name: "New".into(),
        url_resolved: "https://radio.test/new".into(),
        codec: None,
        bitrate_kbps: None,
        country_code: None,
        genre: None,
        tags: Vec::new(),
        votes: 2,
        favicon_url: Some("https://images.test/net-1a-radio.png".into()),
    }]);

    let row = dialog
        .widgets
        .results
        .first_child()
        .and_downcast::<gtk4::ListBoxRow>()
        .expect("search result row");
    let image = row
        .child()
        .and_downcast::<gtk4::Box>()
        .and_then(|content| content.first_child())
        .and_downcast::<gtk4::Stack>()
        .expect("source image stack");
    assert_eq!(image.visible_child_name().as_deref(), Some("fallback"));
}

#[test]
fn src_5_radio_url_preview_hides_an_existing_favorite() {
    let preview = StationPreview::manual("Existing", "https://radio.test/live/");
    assert!(radio::search::station_is_known(
        preview.uuid.as_deref(),
        &preview.stream_url,
        &[("".into(), "https://radio.test/live".into())]
    ));
}

#[test]
fn rad_4_playlist_type_is_detected_without_consuming_a_live_stream() {
    assert_eq!(
        playlist_kind("https://radio.example/listen.PLS?token=1"),
        Some(reprise_core::radio::playlist::PlaylistKind::Pls)
    );
    assert_eq!(
        playlist_kind("https://radio.example/listen.m3u8"),
        Some(reprise_core::radio::playlist::PlaylistKind::M3u)
    );
    assert_eq!(playlist_kind("https://radio.example/live"), None);
}

/// Walk a widget's descendants and return the first `ScrolledWindow`.
fn find_scroller(widget: &gtk4::Widget) -> Option<gtk4::ScrolledWindow> {
    if let Ok(scroller) = widget.clone().downcast::<gtk4::ScrolledWindow>() {
        return Some(scroller);
    }
    let mut child = widget.first_child();
    while let Some(current) = child {
        if let Some(found) = find_scroller(&current) {
            return Some(found);
        }
        child = current.next_sibling();
    }
    None
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn src_8_radio_results_scroll_inside_a_bounded_viewport() {
    gtk4::init().unwrap();
    let conn = Rc::new(RefCell::new(reprise_core::db::open_migrated(None).unwrap()));
    let dialog = RadioAddDialog::new(conn, || {});
    let scroller = dialog
        .widgets
        .dialog
        .child()
        .and_then(|child| find_scroller(&child))
        .expect("the station list must live in a scroller");

    assert_eq!(
        scroller.hscrollbar_policy(),
        gtk4::PolicyType::Never,
        "station rows ellipsize instead of scrolling sideways"
    );
    assert_eq!(scroller.vscrollbar_policy(), gtk4::PolicyType::Automatic);
    assert!(
        scroller.vexpands(),
        "fifty results must not push the footer past the window edge"
    );
    assert!(
        dialog.widgets.results.margin_end() > 0,
        "station rows need the same overlay-scrollbar clearance as the other dialogs"
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn net_1a_radio_search_never_reaches_the_directory_while_the_switch_is_off() {
    gtk4::init().unwrap();
    let conn = Rc::new(RefCell::new(reprise_core::db::open_migrated(None).unwrap()));
    reprise_core::online_sources::set_enabled(&conn.borrow(), false).unwrap();
    let dialog = RadioAddDialog::new(conn, || {});

    dialog.submit("metalcore");

    assert_eq!(
        dialog.widgets.status.text().as_str(),
        strings::text(strings::ONLINE_SOURCES_TURNED_OFF),
        "the refusal must be stated, not silent"
    );
    assert!(
        matches!(dialog.state.borrow().phase, AddDialogPhase::Idle),
        "no search may be dispatched, so the dialog stays idle"
    );
}
