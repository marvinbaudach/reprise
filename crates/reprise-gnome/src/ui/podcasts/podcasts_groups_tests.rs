//! Tests for the grouped podcast surface. Split out of
//! `podcasts_groups.rs` so that file stays under the 800-line gate —
//! the merge of the artwork and multi-selection work pushed it to 849.

use super::*;

fn episode(image_url: Option<&str>) -> EpisodeRow {
    EpisodeRow {
        id: 1,
        subscription_id: 1,
        guid: "episode".into(),
        title: "A compact episode title".into(),
        show: "Show".into(),
        show_image_url: None,
        image_url: image_url.map(str::to_owned),
        kind: PodcastKind::Rss,
        audio_url: "https://example.test/episode.mp3".into(),
        page_url: None,
        published_at: None,
        duration_secs: Some(3_180),
        downloaded_path: None,
        downloaded_bytes: None,
        played_at: None,
        position_ms: 0,
        first_seen_at: 1,
        is_new: false,
    }
}

#[test]
fn src_11_youtube_group_without_channel_artwork_uses_the_newest_episode_thumbnail() {
    let mut newest = episode(Some("https://img.test/newest.jpg"));
    newest.kind = PodcastKind::Youtube;
    let mut older = episode(Some("https://img.test/older.jpg"));
    older.kind = PodcastKind::Youtube;
    let group = SourceGroup {
        subscription_id: 1,
        title: "Channel".into(),
        author: None,
        image_url: None,
        kind: PodcastKind::Youtube,
        sync_to_phone: false,
        episodes: vec![newest, older],
    };

    assert_eq!(group_image_url(&group), Some("https://img.test/newest.jpg"));
}

#[test]
fn src_11_group_artwork_prefers_its_source_and_never_borrows_for_rss() {
    let mut episode = episode(Some("https://img.test/episode.jpg"));
    episode.kind = PodcastKind::Youtube;
    let mut group = SourceGroup {
        subscription_id: 1,
        title: "Source".into(),
        author: None,
        image_url: Some("https://img.test/source.jpg".into()),
        kind: PodcastKind::Youtube,
        sync_to_phone: false,
        episodes: vec![episode],
    };

    assert_eq!(group_image_url(&group), Some("https://img.test/source.jpg"));

    group.image_url = None;
    group.kind = PodcastKind::Rss;
    assert_eq!(group_image_url(&group), None);
}

fn descendants(widget: &gtk4::Widget) -> Vec<gtk4::Widget> {
    let mut found = Vec::new();
    let mut child = widget.first_child();
    while let Some(current) = child {
        found.push(current.clone());
        found.extend(descendants(&current));
        child = current.next_sibling();
    }
    found
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn compact_episode_row_has_no_play_button_and_stays_within_height_budget() {
    gtk4::init().unwrap();
    let bytes = gtk4::glib::Bytes::from_owned(vec![0x66_u8; 64 * 64 * 4]);
    let texture: gtk4::gdk::Texture =
        gtk4::gdk::MemoryTexture::new(64, 64, gtk4::gdk::MemoryFormat::R8g8b8a8, &bytes, 64 * 4)
            .upcast();
    super::super::source_image::remember_texture(
        "https://img.test/episode.jpg".to_owned(),
        32,
        32,
        texture,
    );
    for row in [episode(None), episode(Some("https://img.test/episode.jpg"))] {
        let mut widgets = BTreeMap::new();
        let rendered = episode_row(
            &row,
            &TitleParts {
                distinct: row.title.clone(),
                dimmed: None,
            },
            &mut widgets,
            &EpisodeRenderContext {
                mark: None,
                download_state: &DownloadState::NotDownloaded,
                images_allowed: false,
                network: RowNetworkState {
                    connectivity: Connectivity::Online,
                    unavailable_now: false,
                },
                selection: &Rc::new(RefCell::new(PodcastSelection::default())),
                selected_ids: &[],
            },
        );
        let buttons = descendants(&rendered)
            .into_iter()
            .filter_map(|widget| widget.downcast::<gtk4::Button>().ok())
            .collect::<Vec<_>>();

        assert!(
            buttons
                .iter()
                .all(|button| button.action_name().as_deref() != Some("podcasts.play")),
            "row activation replaces the per-row play button"
        );
        let (_, natural, _, _) = rendered.measure(gtk4::Orientation::Vertical, -1);
        assert!(natural <= 52, "natural row height was {natural}px");
    }
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn not_downloaded_has_no_redundant_status_label() {
    gtk4::init().unwrap();

    let status = download_status(&DownloadState::NotDownloaded)
        .downcast::<gtk4::Box>()
        .unwrap();

    assert!(status.first_child().is_none());
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn collapsed_group_renders_ten_episodes_and_one_show_all_action() {
    gtk4::init().unwrap();
    let episodes = (1..=15)
        .map(|id| {
            let mut row = episode(None);
            row.id = id;
            row.guid = format!("episode-{id}");
            row.title = format!("Episode {id}");
            row
        })
        .collect::<Vec<_>>();
    let group = SourceGroup {
        subscription_id: 1,
        title: "Show".into(),
        author: None,
        image_url: None,
        kind: PodcastKind::Rss,
        sync_to_phone: false,
        episodes,
    };
    let rendered = RenderedSourceGroup {
        summary: SourceSummary {
            episode_count: 15,
            new_count: 0,
            downloaded_bytes: 0,
            latest_published_at: None,
        },
        group,
    };
    let container = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    replace(
        &container,
        &[rendered],
        None,
        &Rc::new(RefCell::new(BTreeSet::new())),
        &Rc::new(RefCell::new(BTreeSet::new())),
        &BTreeMap::new(),
        &[],
        &BTreeMap::new(),
        false,
        Connectivity::Online,
        None,
        &Rc::new(RefCell::new(PodcastSelection::default())),
    );

    let rows = container
        .first_child()
        .and_downcast::<gtk4::Expander>()
        .and_then(|expander| expander.child())
        .and_downcast::<gtk4::Box>()
        .expect("episode rows");
    let child_count =
        std::iter::successors(rows.first_child(), gtk4::prelude::WidgetExt::next_sibling).count();
    assert_eq!(child_count, 11);
    let show_all = rows
        .last_child()
        .and_downcast::<gtk4::Button>()
        .expect("show-all action");
    assert_eq!(show_all.label().as_deref(), Some("Show all 15 episodes"));
    assert_eq!(
        show_all.action_name().as_deref(),
        Some("podcasts.show-all-episodes")
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn src_5_one_expander_is_rendered_per_source_group() {
    gtk4::init().unwrap();
    let container = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    let group = SourceGroup {
        subscription_id: 1,
        title: "Show".into(),
        author: None,
        image_url: None,
        kind: PodcastKind::Rss,
        sync_to_phone: false,
        episodes: Vec::new(),
    };
    let rendered = RenderedSourceGroup {
        summary: SourceSummary {
            episode_count: 0,
            new_count: 0,
            downloaded_bytes: 0,
            latest_published_at: None,
        },
        group,
    };
    let widgets = replace(
        &container,
        &[rendered],
        None,
        &Rc::new(RefCell::new(BTreeSet::new())),
        &Rc::new(RefCell::new(BTreeSet::new())),
        &BTreeMap::new(),
        &[],
        &BTreeMap::new(),
        false,
        Connectivity::Online,
        None,
        &Rc::new(RefCell::new(PodcastSelection::default())),
    );
    assert!(widgets.is_empty());
    assert!(container.first_child().is_some());
    assert!(container
        .first_child()
        .and_downcast::<gtk4::Expander>()
        .is_some());
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn src_12_grouped_selection_survives_render_rebuild_with_a_keyboard_checkbox() {
    gtk4::init().unwrap();
    let mut selection = PodcastSelection::default();
    selection.set_selected(1, true);
    let selection = Rc::new(RefCell::new(selection));
    let expanded_sources = Rc::new(RefCell::new(BTreeSet::from([1])));
    let group = SourceGroup {
        subscription_id: 1,
        title: "Show".into(),
        author: None,
        image_url: None,
        kind: PodcastKind::Rss,
        sync_to_phone: false,
        episodes: vec![episode(None)],
    };
    let rendered = RenderedSourceGroup {
        summary: SourceSummary {
            episode_count: 1,
            new_count: 0,
            downloaded_bytes: 0,
            latest_published_at: None,
        },
        group,
    };
    let container = gtk4::Box::new(gtk4::Orientation::Vertical, 0);

    for _ in 0..2 {
        replace(
            &container,
            std::slice::from_ref(&rendered),
            None,
            &expanded_sources,
            &Rc::new(RefCell::new(BTreeSet::new())),
            &BTreeMap::new(),
            &[],
            &BTreeMap::new(),
            false,
            Connectivity::Online,
            None,
            &selection,
        );
        let checkbox = container
            .first_child()
            .and_downcast::<gtk4::Expander>()
            .and_then(|expander| expander.child())
            .and_downcast::<gtk4::Box>()
            .and_then(|episodes| episodes.first_child())
            .and_downcast::<gtk4::Box>()
            .and_then(|row| row.first_child())
            .and_downcast::<gtk4::CheckButton>()
            .expect("selected episode checkbox");
        assert!(checkbox.is_active());
        assert!(checkbox.is_focusable());
    }
}

/// `SRC-4`: unsubscribing has exactly one place to operate it — the context
/// menu. The hover star duplicated the same destructive action on this row.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn src_4_the_group_header_offers_no_second_unsubscribe_control() {
    gtk4::init().unwrap();
    let group = SourceGroup {
        subscription_id: 7,
        title: "Show".into(),
        author: None,
        image_url: None,
        kind: PodcastKind::Rss,
        sync_to_phone: false,
        episodes: Vec::new(),
    };
    let header = group_header(
        &group,
        &SourceSummary {
            episode_count: 0,
            new_count: 0,
            downloaded_bytes: 0,
            latest_published_at: None,
        },
        &[],
        &[],
        false,
    );
    let header = header.downcast::<gtk4::Box>().unwrap();
    let buttons = descendants(header.upcast_ref())
        .into_iter()
        .filter_map(|widget| widget.downcast::<gtk4::Button>().ok())
        .collect::<Vec<_>>();
    assert!(buttons
        .iter()
        .all(|button| button.action_name().as_deref() != Some("podcasts.unsubscribe")));
    assert!(header
        .last_child()
        .and_downcast::<gtk4::MenuButton>()
        .is_some());
}

/// `SRC-4`: source-level unsubscribe lives in the context menu model, and
/// nowhere else in this module.
#[test]
fn src_4_unsubscribe_exists_only_as_a_menu_action() {
    let source = include_str!("podcasts_groups.rs");

    assert!(
        !source.contains("starred-symbolic"),
        "the hover star is gone from the grouped source header"
    );
    assert!(
        !source.contains("reveal_unsubscribe_on_hover_or_focus"),
        "with no star there is nothing to reveal on hover"
    );
}

/// `SRC-11` / `NET-1a`: the library group header is one of the source
/// image entry points — with `images_allowed: false` it must stay on the
/// glyph fallback even though the group carries a real `image_url`.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn src_11_group_header_stays_on_the_fallback_when_images_are_not_allowed() {
    gtk4::init().unwrap();
    let group = SourceGroup {
        subscription_id: 9,
        title: "Show".into(),
        author: None,
        image_url: Some("https://images.test/net-1a-group-header.jpg".into()),
        kind: PodcastKind::Rss,
        sync_to_phone: false,
        episodes: Vec::new(),
    };
    let header = group_header(
        &group,
        &SourceSummary {
            episode_count: 0,
            new_count: 0,
            downloaded_bytes: 0,
            latest_published_at: None,
        },
        &[],
        &[],
        false,
    );
    let header = header.downcast::<gtk4::Box>().unwrap();
    let artwork = header
        .first_child()
        .and_downcast::<gtk4::Stack>()
        .expect("source image stack");
    assert_eq!(artwork.visible_child_name().as_deref(), Some("fallback"));
}

/// `POD-13`: the classified reason must be a second, always-visible
/// label sitting next to the "Download failed" heading — not hidden
/// behind `set_tooltip_text`, which a keyboard or touch user can never
/// trigger.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn pod_13_a_failed_download_shows_its_classified_reason_without_hovering() {
    gtk4::init().unwrap();
    let state = DownloadState::Failed {
        message: "podcast source could not be reached".into(),
    };

    let status = download_status(&state).downcast::<gtk4::Box>().unwrap();

    let heading = status
        .first_child()
        .and_downcast::<gtk4::Label>()
        .expect("the fixed 'Download failed' heading");
    assert_eq!(
        heading.text(),
        strings::text(strings::PODCAST_DOWNLOAD_FAILED)
    );

    let reason = heading
        .next_sibling()
        .and_downcast::<gtk4::Label>()
        .expect("the classified reason must be a second visible label");
    assert_eq!(reason.text(), "podcast source could not be reached");
}

/// `POD-13`: the retry contract must be reachable and distinguishable —
/// the action stays clickable (not stuck disabled) and its affordance
/// reads as "try again" rather than the plain first-download button.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn pod_13_a_failed_download_offers_a_sensitive_retry_action() {
    gtk4::init().unwrap();
    let widgets = DownloadRowWidgets {
        root: gtk4::Box::new(gtk4::Orientation::Horizontal, 0),
        status: gtk4::Box::new(gtk4::Orientation::Vertical, 0),
        action: gtk4::Button::new(),
        marker: gtk4::Box::new(gtk4::Orientation::Horizontal, 0),
    };

    update_download_state(
        &widgets,
        &DownloadState::Failed {
            message: "podcast source could not be reached".into(),
        },
    );

    assert!(widgets.action.is_sensitive());
    assert_eq!(
        widgets.action.icon_name().as_deref(),
        Some("view-refresh-symbolic")
    );
    assert_eq!(
        widgets.action.tooltip_text().as_deref(),
        Some(strings::text(strings::PODCAST_RETRY_DOWNLOAD)).as_deref()
    );
}
