use super::*;

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
fn src_6_the_foreign_url_hint_appears_while_typing() {
    gtk4::init().unwrap();
    let surface = build_surface(PodcastKind::Rss, Connectivity::Online, true, "DE", None);

    surface.entry.set_text("https://www.youtube.com/@example");
    assert_eq!(
        surface.status.text().as_str(),
        strings::text(strings::PODCAST_URL_IS_YOUTUBE),
        "the reason must be visible before the user submits"
    );
    assert!(!surface.primary.is_sensitive());

    surface.entry.set_text("https://feeds.test/show.xml");
    assert!(
        surface.status.text().is_empty(),
        "the hint must clear once the input belongs to this dialog"
    );
    assert!(surface.primary.is_sensitive());
}

/// `NET-3` point 4: the widget-level half of the offline add dialog — the
/// reason is visible immediately (even before typing), search stays
/// disabled, and a matching URL re-enables the primary action exactly as it
/// would online. The DB-level half — that submitting the URL actually
/// creates the subscription — is proven separately by `add_dialog_input`'s
/// `net_3_the_url_path_still_works_while_search_is_disabled_offline`,
/// which needs no GTK widget at all.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn net_3_search_is_disabled_offline_but_a_url_stays_submittable() {
    gtk4::init().unwrap();
    let surface = build_surface(PodcastKind::Rss, Connectivity::Offline, true, "DE", None);

    assert_eq!(
        surface.status.text().as_str(),
        strings::text(strings::PODCAST_SEARCH_NEEDS_NETWORK),
        "the reason must be visible before the user types anything"
    );

    surface.entry.set_text("metal interviews");
    assert!(
        !surface.primary.is_sensitive(),
        "search must stay disabled while offline"
    );
    assert_eq!(
        surface.status.text().as_str(),
        strings::text(strings::PODCAST_SEARCH_NEEDS_NETWORK)
    );

    surface.entry.set_text("https://feeds.test/show.xml");
    assert!(
        surface.primary.is_sensitive(),
        "a URL belonging to this dialog must stay submittable while offline"
    );
    assert!(
        surface.status.text().is_empty(),
        "the offline reason must clear once the input is a usable URL"
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn src_8_add_dialog_results_scroll_vertically_only() {
    gtk4::init().unwrap();
    let surface = build_surface(PodcastKind::Rss, Connectivity::Online, true, "DE", None);
    let scroller = surface
        .dialog
        .child()
        .and_then(|child| find_scroller(&child))
        .expect("the result list must live in a scroller");

    assert_eq!(
        scroller.hscrollbar_policy(),
        gtk4::PolicyType::Never,
        "a horizontal scrollbar would push the row actions out of view"
    );
    assert_eq!(scroller.vscrollbar_policy(), gtk4::PolicyType::Automatic);
    assert!(
        scroller.vexpands(),
        "only the result list may absorb the leftover height"
    );
    assert!(
        surface.results.margin_end() > 0,
        "rows need clearance from the overlay scrollbar"
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn src_3a_add_dialog_has_fixed_cancel_and_primary_actions() {
    gtk4::init().unwrap();
    let surface = build_surface(PodcastKind::Rss, Connectivity::Online, true, "DE", None);
    let cancel = strings::text(strings::PODCAST_CANCEL);
    let search = strings::text(strings::PODCAST_SEARCH);
    let preview = strings::text(strings::PODCAST_PREVIEW);

    assert_eq!(surface.cancel.label().as_deref(), Some(cancel.as_str()));
    assert_eq!(surface.primary.label().as_deref(), Some(search.as_str()));
    assert!(surface.primary.has_css_class("suggested-action"));
    assert!(!surface.primary.is_sensitive());

    surface.entry.set_text("https://example.test/feed.xml");
    assert_eq!(surface.primary.label().as_deref(), Some(preview.as_str()));
    assert!(surface.primary.is_sensitive());
}

/// `SRC-15a`: the library-genre chip belongs to YouTube, where the genre is a
/// real query. The Apple dialog spends its one chip slot on `SRC-19` instead.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn src_15a_the_library_chip_appears_only_with_a_genre_to_suggest() {
    gtk4::init().unwrap();

    let without = build_surface(PodcastKind::Youtube, Connectivity::Online, true, "DE", None);
    assert!(
        without.suggestion_chip.is_none(),
        "no played genre must mean no chip at all"
    );

    let youtube = build_surface(
        PodcastKind::Youtube,
        Connectivity::Online,
        true,
        "DE",
        Some("Metal"),
    );
    assert_eq!(
        youtube
            .suggestion_chip
            .expect("the YouTube page carries the library chip")
            .label()
            .as_deref(),
        Some(strings::youtube_chip_genre("Metal").as_str()),
        "the YouTube page finds channels, not podcasts, and says so"
    );

    let apple = build_surface(
        PodcastKind::Rss,
        Connectivity::Online,
        true,
        "DE",
        Some("Metal"),
    );
    let label = apple
        .suggestion_chip
        .expect("the Apple page carries its charts chip")
        .label()
        .expect("chip label");
    assert_eq!(label, strings::podcast_chip_popular_in_country("DE"));
    assert_ne!(label, strings::podcast_chip_genre("Metal"));
}

/// `SRC-19`: what `build_surface` alone can establish — the pill exists, reads
/// the country chart, and starts beside an untouched entry. It deliberately
/// does *not* click: `build_surface` attaches no handlers, `present` wires them
/// (`AddDialogChip::Charts` into `load_charts`, `LibraryGenre` into the entry
/// plus `submit`), so an `emit_clicked` here would pass whatever the click was
/// wired to do. The chip's recorded action is the honest stand-in — it is the
/// value `present` matches on, and `Charts` is the branch that never writes to
/// the entry.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn src_19_the_apple_dialog_carries_the_charts_chip_and_the_entry_stays_empty() {
    gtk4::init().unwrap();
    let surface = build_surface(
        PodcastKind::Rss,
        Connectivity::Online,
        true,
        "DE",
        Some("Metal"),
    );
    let chip = surface
        .suggestion_chip
        .as_ref()
        .expect("an online Apple dialog must carry the charts chip");

    assert_eq!(
        chip.label().as_deref(),
        Some(strings::podcast_chip_popular_in_country("DE").as_str())
    );
    assert!(chip.has_css_class("pill"));
    assert!(surface.entry.text().is_empty());
    assert_eq!(
        surface.chip_action,
        Some(AddDialogChip::Charts {
            country: "DE".to_owned()
        }),
        "a chart is not a hidden search term — it loads into the result list"
    );
}

/// `SRC-19` / `NET-1a`: the widget-level half of the consent gate. The pure
/// decision is proven by `add_dialog_chips`'
/// `src_19_the_charts_chip_is_absent_without_network_consent`; this one proves
/// the surface asks — with podcast online sources off there is no pill to
/// click, so the two Apple requests can never be issued.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn src_19_the_charts_chip_is_absent_when_online_sources_are_off() {
    gtk4::init().unwrap();
    let surface = build_surface(
        PodcastKind::Rss,
        Connectivity::Online,
        false,
        "DE",
        Some("Metal"),
    );

    assert!(
        surface.suggestion_chip.is_none(),
        "a refused source must not offer a pill that issues its requests anyway"
    );
    assert!(surface.chip_action.is_none());
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn src_18_a_result_row_states_its_freshness_after_the_author() {
    gtk4::init().unwrap();
    let subtitle =
        super::super::add_dialog_results::rss_subtitle(Some("Ada"), Some(0), 14 * 86_400);
    let row = candidate_row("Show", &subtitle, PodcastKind::Rss, None, false);
    let labels = row
        .first_child()
        .and_then(|child| child.next_sibling())
        .and_downcast::<gtk4::Box>()
        .expect("result labels");
    let rendered = labels
        .last_child()
        .and_downcast::<gtk4::Label>()
        .expect("result subtitle");

    assert_eq!(rendered.text(), "Ada · New 2 weeks ago");
}

/// `SRC-8`: the dialog keeps one width. Neither a long show title nor a
/// long publisher line may raise a result row's *minimum* width past the
/// dialog's content width — `adw::Dialog` honours that width only as a
/// natural size, so an unellipsized label widens the window and the dialog
/// visibly changes size between two searches.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn src_8_a_long_result_row_never_widens_the_dialog() {
    gtk4::init().unwrap();
    let long = candidate_row(
        "Ein außergewöhnlich langer Podcasttitel, der die Dialogbreite deutlich überschreitet",
        "Herausgegeben von einem Sender mit einem ebenso langen Namen · 480 Folgen",
        PodcastKind::Rss,
        None,
        false,
    );
    let short = candidate_row("Show", "Publisher", PodcastKind::Rss, None, false);

    let (long_minimum, _, _, _) = long.measure(gtk4::Orientation::Horizontal, -1);
    let (short_minimum, _, _, _) = short.measure(gtk4::Orientation::Horizontal, -1);

    assert_eq!(
        long_minimum, short_minimum,
        "row text length must not change the width the row demands"
    );
    assert!(
        long_minimum <= CONTENT_WIDTH,
        "a result row must fit the {CONTENT_WIDTH}px dialog, got {long_minimum}px"
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn src_5_result_rows_use_the_source_artwork_surface() {
    gtk4::init().unwrap();
    let row = candidate_row("Show", "Publisher", PodcastKind::Rss, None, false);
    let image = row
        .first_child()
        .and_downcast::<gtk4::Stack>()
        .expect("source image stack");

    assert!(image.has_css_class("reprise-source-image"));
    assert_eq!(image.width_request(), 40);
    assert_eq!(image.height_request(), 40);
}

/// `SRC-11` / `NET-1a`: the helper every result and preview row in this
/// dialog uses to gate its source image must reflect both the global
/// online-sources switch and the Source Images module — not just one of
/// them.
#[test]
fn src_11_add_dialog_images_allowed_is_the_net_1a_and() {
    let conn = crate::test_db::open().unwrap();
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

/// `SRC-11` / `NET-1a`: with the gate closed, a result row carrying a real
/// `image_url` must stay on the glyph fallback — nothing is requested.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn src_11_result_row_stays_on_the_fallback_when_images_are_not_allowed() {
    gtk4::init().unwrap();
    let row = candidate_row(
        "Show",
        "Publisher",
        PodcastKind::Rss,
        Some("https://images.test/net-1a-add-dialog.jpg"),
        false,
    );
    let image = row
        .first_child()
        .and_downcast::<gtk4::Stack>()
        .expect("source image stack");
    assert_eq!(image.visible_child_name().as_deref(), Some("fallback"));
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn src_7_a_successful_subscribe_acknowledges_the_row_in_place() {
    gtk4::init().unwrap();
    let conn = Rc::new(crate::test_db::open().unwrap());
    let parent = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    let on_added: OnAdded = Rc::new(|_| {});
    append_heading(&parent, &strings::text(strings::PODCAST_APPLE_RESULTS));
    append_candidate(
        &parent,
        Candidate {
            kind: PodcastKind::Rss,
            title: "New show".into(),
            subtitle: "Publisher".into(),
            author: Some("Publisher".into()),
            image_url: None,
            url: "https://example.test/new-feed".into(),
            identity_guids: Vec::new(),
        },
        &conn,
        &on_added,
        false,
    );

    // The heading is appended first, so the candidate row is the last child.
    let row = parent
        .last_child()
        .and_downcast::<gtk4::Box>()
        .expect("candidate row");
    let button = row
        .last_child()
        .and_downcast::<gtk4::Button>()
        .expect("subscribe button");
    button.emit_clicked();

    // SRC-7: the row stays so the add is visibly acknowledged; only the
    // next submitted search drops it (SRC-5).
    assert_eq!(
        parent.last_child().and_downcast::<gtk4::Box>().as_ref(),
        Some(&row),
        "the result row must survive a successful add"
    );
    assert!(
        !button.is_sensitive(),
        "the acknowledged action must not be pressable again"
    );
    assert!(button.has_css_class("reprise-source-added"));
}

#[test]
fn dialogue_state_names_cover_async_lifecycle() {
    let phases = [
        AddDialogPhase::Idle,
        AddDialogPhase::Searching,
        AddDialogPhase::Previewing,
        AddDialogPhase::Results,
        AddDialogPhase::Preview,
        AddDialogPhase::Error,
    ];
    assert_eq!(phases.len(), 6);
}

/// `POD-13`: the add-source preview must never show raw provider text —
/// yt-dlp's first stderr line in particular can carry a signed URL, a query
/// token, a credential-like value or a local filesystem path. `preview_error`
/// is what `preview`'s RSS and YouTube branches map every `PodcastError`
/// through instead of `.to_string()`; feed it a payload shaped exactly like
/// a real leak and confirm none of it survives into the string the dialog
/// would display.
#[test]
fn pod_13_preview_error_never_forwards_a_leaking_payload() {
    let leaking = "GET https://cdn.example.test/ep.mp3?sig=abc123&token=SECRET-TOKEN \
        failed while writing /home/user/.local/share/reprise/podcasts/leak.mp3";

    for error in [
        podcasts::PodcastError::YtDlpFailure {
            kind: reprise_core::podcasts::ytdlp::YtDlpFailureKind::Other,
            stderr: leaking.to_owned(),
        },
        podcasts::PodcastError::Transport(leaking.to_owned()),
        podcasts::PodcastError::Body(leaking.to_owned()),
    ] {
        let message = preview_error(&error);
        for needle in [
            "token",
            "SECRET",
            "sig=",
            "cdn.example.test",
            "/home/user",
            ".local/share/reprise",
        ] {
            assert!(
                !message.contains(needle),
                "preview_error leaked {needle:?}: {message}"
            );
        }
    }
    assert_eq!(
        preview_error(&podcasts::PodcastError::YtDlpFailure {
            kind: reprise_core::podcasts::ytdlp::YtDlpFailureKind::Other,
            stderr: leaking.to_owned(),
        }),
        "YouTube source could not be read with yt-dlp"
    );
}

#[test]
fn disabling_initial_import_persists_the_previewed_guid_baseline() {
    let guids = vec!["old-a".to_owned(), "old-b".to_owned()];
    assert_eq!(baseline_for_import_choice(false, &guids), Some(guids));
    assert_eq!(baseline_for_import_choice(true, &["old".to_owned()]), None);
}

#[test]
fn new_subscription_uses_the_configured_auto_download_default() {
    let config = podcasts::config::PodcastConfig {
        import_count: 25,
        auto_download_default: true,
        cleanup_policy: podcasts::config::CleanupPolicy::KeepAll,
        youtube_import_count: 10,
        youtube_hide_shorts_default: true,
        youtube_browser: None,
        ytdlp_path: None,
        refresh_hours: 6,
        latest_per_channel_default: 5,
        keep_downloaded_default: 5,
    };
    assert!(configured_auto_download_default(Some(&config)));
    assert!(!configured_auto_download_default(None));
}
