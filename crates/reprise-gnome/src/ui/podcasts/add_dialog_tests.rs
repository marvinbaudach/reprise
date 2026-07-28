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
    let surface = build_surface(PodcastKind::Rss);

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

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn src_8_add_dialog_results_scroll_vertically_only() {
    gtk4::init().unwrap();
    let surface = build_surface(PodcastKind::Rss);
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
    let surface = build_surface(PodcastKind::Rss);
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

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn src_5_result_rows_use_the_source_artwork_surface() {
    gtk4::init().unwrap();
    let row = candidate_row("Show", "Publisher", PodcastKind::Rss, None);
    let image = row
        .first_child()
        .and_downcast::<gtk4::Stack>()
        .expect("source image stack");

    assert!(image.has_css_class("reprise-source-image"));
    assert_eq!(image.width_request(), 40);
    assert_eq!(image.height_request(), 40);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn src_7_a_successful_subscribe_acknowledges_the_row_in_place() {
    gtk4::init().unwrap();
    let conn = Rc::new(RefCell::new(reprise_core::db::open_migrated(None).unwrap()));
    let parent = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    let on_added: OnAdded = Rc::new(|_| {});
    append_heading(&parent, strings::PODCAST_APPLE_RESULTS);
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
        ytdlp_path: None,
        refresh_hours: 6,
    };
    assert!(configured_auto_download_default(Some(&config)));
    assert!(!configured_auto_download_default(None));
}
