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
    assert_ne!(
        label,
        strings::youtube_chip_genre("Metal"),
        "the Apple page must not spend its one slot on the library chip"
    );
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
    let row = candidate_row("Show", &subtitle, None, None, PodcastKind::Rss, None, false);
    let labels = row
        .root
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

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn src_9_candidate_rows_expose_the_existing_root_and_subtitle_for_wave_two() {
    gtk4::init().unwrap();
    let row = candidate_row(
        "Channel",
        "3 matching videos",
        None,
        Some("channel"),
        PodcastKind::Youtube,
        None,
        false,
    );

    assert_eq!(row.subtitle.text(), "3 matching videos");
    assert!(row.root.is::<gtk4::Box>());
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn src_21_highlighted_title_keeps_its_end_ellipsis_and_pango_attributes() {
    gtk4::init().unwrap();
    let row = candidate_row(
        "A matched title long enough to require ellipsizing inside the fixed-width dialog row",
        "Publisher · New 1 week ago",
        Some("Publisher"),
        Some("matched"),
        PodcastKind::Rss,
        None,
        false,
    );
    let window = gtk4::Window::builder()
        .default_width(280)
        .default_height(100)
        .child(&row.root)
        .build();
    window.present();
    let main_loop = gtk4::glib::MainLoop::new(None, false);
    let quit = main_loop.clone();
    gtk4::glib::timeout_add_local_once(std::time::Duration::from_millis(60), move || quit.quit());
    main_loop.run();

    let labels = row
        .root
        .first_child()
        .and_then(|child| child.next_sibling())
        .and_downcast::<gtk4::Box>()
        .expect("result labels");
    let title_line = labels
        .first_child()
        .and_downcast::<gtk4::Box>()
        .expect("result title line");
    let title = title_line
        .first_child()
        .and_downcast::<gtk4::Label>()
        .expect("result title");

    assert_eq!(title.ellipsize(), gtk4::pango::EllipsizeMode::End);
    assert!(title.uses_markup());
    assert!(title.layout().is_ellipsized());
    assert!(
        title.layout().attributes().is_some(),
        "Pango must retain the accent-bold span while ellipsizing the title"
    );
    window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn src_22_unexplained_marker_is_accessible_and_keeps_its_space() {
    gtk4::init().unwrap();
    let adjacent_row = candidate_row(
        "The Jasta Show",
        "GaS Digital Network · New last week",
        Some("GaS Digital Network"),
        Some("Metalcore"),
        PodcastKind::Rss,
        None,
        false,
    );
    let adjacent_labels = adjacent_row
        .root
        .first_child()
        .and_then(|child| child.next_sibling())
        .and_downcast::<gtk4::Box>()
        .expect("result labels");
    let adjacent_title_line = adjacent_labels
        .first_child()
        .and_downcast::<gtk4::Box>()
        .expect("result title line");
    let adjacent_title = adjacent_title_line
        .first_child()
        .and_downcast::<gtk4::Label>()
        .expect("result title");
    let adjacent_marker = adjacent_title_line
        .last_child()
        .and_downcast::<gtk4::Image>()
        .expect("unexplained search match marker after the title");
    let row = candidate_row(
        "The Jasta Show with an exceptionally long title that must ellipsize before the marker",
        "GaS Digital Network · New last week",
        Some("GaS Digital Network"),
        Some("Metalcore"),
        PodcastKind::Rss,
        None,
        false,
    );
    let labels = row
        .root
        .first_child()
        .and_then(|child| child.next_sibling())
        .and_downcast::<gtk4::Box>()
        .expect("result labels");
    let title_line = labels
        .first_child()
        .and_downcast::<gtk4::Box>()
        .expect("result title line");
    let title = title_line
        .first_child()
        .and_downcast::<gtk4::Label>()
        .expect("result title");
    let marker = title_line
        .last_child()
        .and_downcast::<gtk4::Image>()
        .expect("unexplained search match marker after the title");
    let explanation = strings::text(strings::PODCAST_SEARCH_MATCH_NOT_SHOWN);

    assert_eq!(
        marker.icon_name().as_deref(),
        Some(crate::ui::icons::UNEXPLAINED_SEARCH_MATCH)
    );
    assert_eq!(marker.tooltip_text().as_deref(), Some(explanation.as_str()));
    assert!(
        !adjacent_title.hexpands(),
        "the title must not consume the free space between its text and the marker"
    );
    assert!(marker.has_css_class("reprise-text-secondary"));
    assert_eq!(
        marker.pixel_size(),
        16,
        "the quiet marker must remain legible at the subtitle's visual weight"
    );
    assert!(gtk4::test_accessible_has_role(
        &marker,
        gtk4::AccessibleRole::Img
    ));
    assert!(gtk4::test_accessible_has_property(
        &marker,
        gtk4::AccessibleProperty::Description
    ));

    let marker_minimum = marker.measure(gtk4::Orientation::Horizontal, -1).0;
    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    content.append(&adjacent_row.root);
    content.append(&row.root);
    let window = gtk4::Window::builder()
        .default_width(280)
        .default_height(160)
        .child(&content)
        .build();
    window.present();
    let main_loop = gtk4::glib::MainLoop::new(None, false);
    let quit = main_loop.clone();
    gtk4::glib::timeout_add_local_once(std::time::Duration::from_millis(60), move || quit.quit());
    main_loop.run();

    let adjacent_title_natural = adjacent_title.measure(gtk4::Orientation::Horizontal, -1).1;
    assert_eq!(
        adjacent_title.width(),
        adjacent_title_natural,
        "a short title must keep only its text width so the marker travels with it"
    );
    let adjacent_title_origin = adjacent_title
        .compute_point(&adjacent_title_line, &gtk4::graphene::Point::new(0.0, 0.0))
        .expect("title position inside its line");
    let adjacent_marker_origin = adjacent_marker
        .compute_point(&adjacent_title_line, &gtk4::graphene::Point::new(0.0, 0.0))
        .expect("marker position inside its line");
    assert_eq!(
        adjacent_marker_origin.x(),
        adjacent_title_origin.x()
            + adjacent_title.width() as f32
            + adjacent_title_line.spacing() as f32,
        "the marker must sit immediately after the title"
    );
    assert!(title.layout().is_ellipsized());
    assert!(marker_minimum > 0);
    assert!(
        marker.width() >= marker_minimum,
        "the title must yield space to the {marker_minimum}px marker, got {}px",
        marker.width()
    );
    window.close();
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
        None,
        Some("Metalcore"),
        PodcastKind::Rss,
        None,
        false,
    );
    let short = candidate_row(
        "Show",
        "Publisher",
        None,
        Some("Metalcore"),
        PodcastKind::Rss,
        None,
        false,
    );

    let (long_minimum, _, _, _) = long.root.measure(gtk4::Orientation::Horizontal, -1);
    let (short_minimum, _, _, _) = short.root.measure(gtk4::Orientation::Horizontal, -1);

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
    let row = candidate_row(
        "Show",
        "Publisher",
        None,
        None,
        PodcastKind::Rss,
        None,
        false,
    );
    let image = row
        .root
        .first_child()
        .and_downcast::<gtk4::Stack>()
        .expect("source image stack");

    assert!(image.has_css_class("reprise-source-image"));
    assert_eq!(image.width_request(), 40);
    assert_eq!(image.height_request(), 40);
}

/// `SRC-11` / `NET-1a`: the helper every result and preview row in this
/// dialog uses to gate its source image must reflect both the global
/// online-sources switch and the Artwork module — not just one of
/// them.
#[test]
fn src_11_add_dialog_images_allowed_is_the_net_1a_and() {
    let conn = crate::test_db::open().unwrap();
    // Neither the global gate nor the module is on by default.
    assert!(!images_allowed(&conn));

    reprise_core::modules::set_enabled(&conn, &reprise_core::modules::ARTWORK_MODULE, true)
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
        None,
        None,
        PodcastKind::Rss,
        Some("https://images.test/net-1a-add-dialog.jpg"),
        false,
    );
    let image = row
        .root
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
    let added = Rc::new(Cell::new(None));
    let added_for_callback = added.clone();
    let on_added: OnAdded = Rc::new(move |subscription_id, import_latest| {
        added_for_callback.set(Some((subscription_id, import_latest)));
    });
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
            follower_count: None,
            channel_id: None,
            matching_video_count: None,
        },
        None,
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
    let (subscription_id, import_latest) = added.get().expect("add callback");
    assert!(subscription_id > 0);
    assert!(import_latest);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn the_add_dialog_offers_an_automatic_fill_switch() {
    gtk4::init().unwrap();
    let conn = Rc::new(crate::test_db::open().unwrap());
    let parent = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    let on_added: OnAdded = Rc::new(|_, _| {});
    append_preview(
        &parent,
        Preview {
            kind: PodcastKind::Rss,
            title: "Show".into(),
            author: None,
            image_url: None,
            count: 10,
            url: "https://example.test/feed".into(),
            guids: Vec::new(),
        },
        10,
        true,
        &conn,
        &on_added,
    );

    let option_buttons = std::iter::successors(parent.first_child(), WidgetExt::next_sibling)
        .filter_map(|child| child.downcast::<gtk4::CheckButton>().ok())
        .collect::<Vec<_>>();
    let automatic_fill_label = strings::text(strings::PODCAST_AUTO_DOWNLOAD);
    let automatic_fill = option_buttons
        .iter()
        .find(|button| button.label().as_deref() == Some(automatic_fill_label.as_str()))
        .expect("the preview must offer automatic filling for this subscription");
    assert!(automatic_fill.is_active());
}

/// `SRC-19`: the chart path owns its empty sentence. Borrowing the search
/// one would quote the chip's label back as though it were a typed term and
/// then advise pasting a feed URL — advice for a search that missed, not for a
/// curated list that is empty or that this library already follows in full.
#[test]
fn src_19_an_empty_chart_does_not_speak_the_language_of_a_failed_search() {
    let label = strings::podcast_chip_popular_in_country("DE");
    let empty = strings::podcast_charts_empty("DE");

    assert_ne!(empty, strings::source_nothing_found(&label));
    assert!(
        !empty.contains(label.as_str()),
        "the chart's country label is not a search term: {empty}"
    );
    assert!(empty.contains("DE"), "the chart still says which country");
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
        "YouTube request failed — check the application log"
    );
}

#[test]
fn disabling_initial_import_persists_the_previewed_guid_baseline() {
    let guids = vec!["old-a".to_owned(), "old-b".to_owned()];
    assert_eq!(baseline_for_import_choice(false, &guids), Some(guids));
    assert_eq!(baseline_for_import_choice(true, &["old".to_owned()]), None);
}

#[test]
fn new_subscription_uses_the_selected_automatic_fill_choice() {
    let conn = crate::test_db::open().unwrap();
    let id = subscribe(
        &conn,
        &Candidate {
            kind: PodcastKind::Rss,
            title: "Show".into(),
            subtitle: "Publisher".into(),
            author: Some("Publisher".into()),
            image_url: None,
            url: "https://example.test/feed".into(),
            identity_guids: Vec::new(),
            follower_count: None,
            channel_id: None,
            matching_video_count: None,
        },
        true,
        None,
    )
    .unwrap();
    let subscription = podcasts::store::subscription(&conn, id).unwrap().unwrap();
    assert!(subscription.auto_download);
}

#[test]
fn new_subscription_discovery_inherits_the_configured_automatic_fill_default() {
    let conn = crate::test_db::open().unwrap();
    podcasts::config::set_auto_download_default(&conn, true).unwrap();
    let config = podcasts::config::load(&conn).unwrap();

    assert!(configured_auto_download_default(Some(&config)));
    assert!(!configured_auto_download_default(None));
}
