//! Tests for `podcasts::youtube_channel_detail`, split out to keep the main module under the 800-line file-size gate.

use std::collections::BTreeMap;

use reprise_core::podcasts::download_state::DownloadState;
use reprise_core::podcasts::{EpisodeRow, PodcastKind};

use super::*;
use crate::ui::podcasts::podcasts_presentation::{source_summary, RenderedSourceGroup};

fn episode(id: i64, duration_secs: Option<i64>) -> EpisodeRow {
    EpisodeRow {
        id,
        subscription_id: 7,
        guid: format!("video-{id}"),
        title: format!("Video {id}"),
        show: "Channel".into(),
        show_image_url: None,
        image_url: None,
        kind: PodcastKind::Youtube,
        audio_url: format!("https://youtube.test/watch?v={id}"),
        page_url: None,
        published_at: Some(id),
        duration_secs,
        downloaded_path: None,
        downloaded_bytes: None,
        played_at: None,
        position_ms: 0,
        first_seen_at: id,
    }
}

#[test]
fn pod_10_channel_opens_with_latest_ten_long_form_videos() {
    let episodes = (1..=12)
        .rev()
        .map(|id| episode(id, Some(if id == 11 { 60 } else { 600 })))
        .collect::<Vec<_>>();
    let state = YoutubeChannelState::default();

    let visible = state.visible_episodes(7, &episodes);

    assert_eq!(
        visible.iter().map(|episode| episode.id).collect::<Vec<_>>(),
        [12, 10, 9, 8, 7, 6, 5, 4, 3, 2]
    );
}

#[test]
fn pod_10_loaded_range_expands_one_channel_window_without_affecting_another() {
    let episodes = (1..=25)
        .rev()
        .map(|id| episode(id, Some(600)))
        .collect::<Vec<_>>();
    let mut state = YoutubeChannelState::default();

    state.set_loaded_limit(7, 40);

    assert_eq!(state.visible_episodes(7, &episodes).len(), 25);
    assert_eq!(state.visible_episodes(8, &episodes).len(), 10);
}

#[test]
fn pod_10_shorts_filter_can_be_disabled_per_channel() {
    let episodes = vec![episode(2, Some(600)), episode(1, Some(60))];
    let mut state = YoutubeChannelState::default();

    assert_eq!(state.visible_episodes(7, &episodes).len(), 1);
    state.set_hide_shorts(7, false);
    assert_eq!(state.visible_episodes(7, &episodes).len(), 2);
}

/// `SET-9`: the Online sources page's "Hide Shorts" default seeds new
/// channels' Shorts visibility, but a channel's own explicit toggle
/// still overrides it — turning the global default off must not
/// silently flip a channel someone already set the other way.
#[test]
fn set_9_hide_shorts_default_seeds_new_channels_but_per_channel_override_wins() {
    let episodes = vec![episode(2, Some(600)), episode(1, Some(60))];
    let mut state = YoutubeChannelState::default();
    state.set_default_shows_shorts(true); // "Hide Shorts" preference is off

    // Untouched channel follows the default: Shorts are shown.
    assert_eq!(state.visible_episodes(7, &episodes).len(), 2);

    // This channel explicitly hides Shorts, overriding the default.
    state.set_hide_shorts(7, true);
    assert_eq!(state.visible_episodes(7, &episodes).len(), 1);

    // A different, untouched channel still follows the default.
    assert_eq!(state.visible_episodes(8, &episodes).len(), 2);
}

#[test]
fn pod_10_batch_selection_is_stable_and_channel_scoped() {
    let mut state = YoutubeChannelState::default();

    state.set_selected(7, 11, true);
    state.set_selected(7, 12, true);
    state.set_selected(8, 21, true);
    state.set_selected(7, 11, false);

    assert_eq!(state.selected_ids(7), vec![12]);
    assert_eq!(state.selected_ids(8), vec![21]);
}

#[test]
fn pod_10_channel_projection_windows_children_but_preserves_full_summary() {
    let episodes = (1..=12)
        .rev()
        .map(|id| episode(id, Some(600)))
        .collect::<Vec<_>>();
    let group = reprise_core::podcasts::SourceGroup {
        subscription_id: 7,
        title: "Channel".into(),
        author: None,
        image_url: None,
        kind: PodcastKind::Youtube,
        sync_to_phone: false,
        episodes,
    };
    let rendered = RenderedSourceGroup {
        summary: source_summary(&group, &BTreeMap::<i64, DownloadState>::new()),
        group,
    };

    let projected = project_channel(&rendered, &YoutubeChannelState::default());

    assert_eq!(projected.group.episodes.len(), 10);
    assert_eq!(projected.summary.episode_count, 12);
}

#[test]
fn pod_10_channel_detail_is_an_explicit_open_and_close_location() {
    let mut state = YoutubeChannelState::default();

    state.open_channel(7);
    assert_eq!(state.active_channel(), Some(7));
    state.close_channel();
    assert_eq!(state.active_channel(), None);
}

#[test]
fn pod_11_channel_header_summary_reflects_window_shorts_and_downloads_together() {
    // 11 long-form videos plus one Short (id 11, 60s).
    let episodes = (1..=12)
        .rev()
        .map(|id| episode(id, Some(if id == 11 { 60 } else { 600 })))
        .collect::<Vec<_>>();
    let mut state = YoutubeChannelState::default();
    let mut download_states = BTreeMap::new();
    download_states.insert(12, DownloadState::Downloaded { bytes: 1_000 });
    download_states.insert(2, DownloadState::Downloaded { bytes: 2_000 });

    // Initial 10-of-11 window (Shorts hidden), two episodes downloaded.
    let shown = state.visible_episodes(7, &episodes).len();
    let available = state.available_count(7, &episodes);
    let summary = podcasts_download_presentation::channel_download_summary(
        shown,
        available,
        &episodes,
        &download_states,
    );
    assert_eq!((summary.shown, summary.available), (10, 11));
    assert_eq!(summary.downloaded_count, 2);
    assert_eq!(summary.downloaded_bytes, 3_000);

    // "Load more" changes the window, not the download totals.
    state.set_loaded_limit(7, 40);
    let shown = state.visible_episodes(7, &episodes).len();
    let available = state.available_count(7, &episodes);
    let summary = podcasts_download_presentation::channel_download_summary(
        shown,
        available,
        &episodes,
        &download_states,
    );
    assert_eq!((summary.shown, summary.available), (11, 11));
    assert_eq!(summary.downloaded_count, 2);
    assert_eq!(summary.downloaded_bytes, 3_000);

    // Revealing Shorts changes the window again; downloads stay correct.
    state.set_hide_shorts(7, false);
    let shown = state.visible_episodes(7, &episodes).len();
    let available = state.available_count(7, &episodes);
    let summary = podcasts_download_presentation::channel_download_summary(
        shown,
        available,
        &episodes,
        &download_states,
    );
    assert_eq!((summary.shown, summary.available), (12, 12));
    assert_eq!(summary.downloaded_count, 2);

    // A newly finished download changes the totals without touching the window.
    download_states.insert(11, DownloadState::Downloaded { bytes: 500 });
    let summary = podcasts_download_presentation::channel_download_summary(
        shown,
        available,
        &episodes,
        &download_states,
    );
    assert_eq!(summary.downloaded_count, 3);
    assert_eq!(summary.downloaded_bytes, 3_500);

    // Deleting a download drops it from both the count and the total.
    download_states.remove(&12);
    let summary = podcasts_download_presentation::channel_download_summary(
        shown,
        available,
        &episodes,
        &download_states,
    );
    assert_eq!(summary.downloaded_count, 2);
    assert_eq!(summary.downloaded_bytes, 2_500);
}

/// `SRC-11` / `NET-1a`: the channel-detail header is one of the source
/// image entry points — with `images_allowed: false` (set via
/// `update`) it must stay on the glyph fallback even though the group
/// carries a real `image_url`.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn src_11_channel_header_stays_on_the_fallback_when_images_are_not_allowed() {
    gtk4::init().unwrap();
    let stack = gtk4::Stack::new();
    let detail = YoutubeChannelDetail::new(&stack, true);
    let group = reprise_core::podcasts::SourceGroup {
        subscription_id: 7,
        title: "Channel".into(),
        author: None,
        image_url: Some("https://images.test/net-1a-channel-header.jpg".into()),
        kind: PodcastKind::Youtube,
        sync_to_phone: false,
        episodes: Vec::new(),
    };
    let rendered = RenderedSourceGroup {
        summary: source_summary(&group, &BTreeMap::<i64, DownloadState>::new()),
        group,
    };
    detail.update(
        std::slice::from_ref(&rendered),
        &BTreeMap::new(),
        &[],
        &BTreeMap::new(),
        false,
    );

    let header = detail
        .build_header(&rendered)
        .downcast::<gtk4::Box>()
        .unwrap();
    let artwork = header
        .first_child()
        .and_then(|back| back.next_sibling())
        .and_downcast::<gtk4::Stack>()
        .expect("source image stack");
    assert_eq!(artwork.visible_child_name().as_deref(), Some("fallback"));
}

fn find_phone_indicator(header: &gtk4::Box) -> Option<gtk4::Widget> {
    let mut child = header.first_child();
    while let Some(widget) = child {
        if widget
            .downcast_ref::<gtk4::Image>()
            .is_some_and(|image| image.icon_name().as_deref() == Some("phone-symbolic"))
        {
            return Some(widget);
        }
        child = widget.next_sibling();
    }
    None
}

/// `POD-12` / `D3`: the channel detail header's "On phone" indicator
/// must track the same per-device selection the channel toggle writes
/// (`podcasts_context_menu` / `podcasts_device_sync::install_action`) —
/// absent while unselected, present the moment a connected device is
/// selected. It must also stay a plain, non-interactive `gtk4::Image`
/// with no controller attached: this view has no code path that could
/// write the selection back through it, only through the existing
/// toggle.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn pod_12_channel_header_on_phone_indicator_reflects_the_toggle_and_stays_read_only() {
    gtk4::init().unwrap();
    let stack = gtk4::Stack::new();
    let detail = YoutubeChannelDetail::new(&stack, true);
    let group = reprise_core::podcasts::SourceGroup {
        subscription_id: 7,
        title: "Channel".into(),
        author: None,
        image_url: None,
        kind: PodcastKind::Youtube,
        sync_to_phone: false,
        episodes: Vec::new(),
    };
    let rendered = RenderedSourceGroup {
        summary: source_summary(&group, &BTreeMap::<i64, DownloadState>::new()),
        group,
    };
    let phone = PodcastSyncDevice {
        id: "mtp:phone".into(),
        name: "Phone".into(),
    };

    // Connected, but not selected for this channel: no indicator.
    detail.update(
        std::slice::from_ref(&rendered),
        &BTreeMap::new(),
        std::slice::from_ref(&phone),
        &BTreeMap::new(),
        false,
    );
    let header = detail
        .build_header(&rendered)
        .downcast::<gtk4::Box>()
        .unwrap();
    assert!(
        find_phone_indicator(&header).is_none(),
        "indicator must not appear before the toggle selects this channel"
    );

    // The channel toggle selects this subscription for the connected
    // device — the indicator must now reflect it.
    let mut selected = BTreeMap::new();
    selected.insert(7, vec!["mtp:phone".to_owned()]);
    detail.update(
        std::slice::from_ref(&rendered),
        &BTreeMap::new(),
        &[phone],
        &selected,
        false,
    );
    let header = detail
        .build_header(&rendered)
        .downcast::<gtk4::Box>()
        .unwrap();
    let indicator = find_phone_indicator(&header).expect("indicator must appear once selected");
    assert!(indicator.is::<gtk4::Image>(), "must stay a plain glyph");
    assert_eq!(
        indicator.observe_controllers().n_items(),
        0,
        "must carry no gesture/action controller — it has no write path back to selection"
    );
}

fn children_of(root: &gtk4::Box) -> Vec<gtk4::Widget> {
    std::iter::successors(root.first_child(), gtk4::prelude::WidgetExt::next_sibling).collect()
}

/// `POD-14`: a channel whose every current entry is a hidden Short
/// shows the dedicated notice — not a silently blank row list — and
/// its "Show Shorts anyway" action reveals them. Would go red if the
/// notice were skipped (blank window) or if it appeared for a channel
/// that genuinely has non-Short content.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn pod_14_only_shorts_here_offers_a_way_to_reveal_them() {
    gtk4::init().unwrap();
    let stack = gtk4::Stack::new();
    // Shorts hidden by default.
    let detail = YoutubeChannelDetail::new(&stack, true);
    let group = reprise_core::podcasts::SourceGroup {
        subscription_id: 7,
        title: "Channel".into(),
        author: None,
        image_url: None,
        kind: PodcastKind::Youtube,
        sync_to_phone: false,
        episodes: vec![episode(2, Some(60)), episode(1, Some(30))],
    };
    let rendered = RenderedSourceGroup {
        summary: source_summary(&group, &BTreeMap::<i64, DownloadState>::new()),
        group,
    };
    detail.update(
        std::slice::from_ref(&rendered),
        &BTreeMap::new(),
        &[],
        &BTreeMap::new(),
        false,
    );
    detail.state.borrow_mut().open_channel(7);
    detail.render_active();

    // Header, controls, notice — no episode rows, since every entry is
    // a hidden Short.
    let children = children_of(&detail.content);
    assert_eq!(children.len(), 3);
    assert!(children[2].has_css_class("reprise-shorts-only-notice"));

    let notice = children[2].clone().downcast::<gtk4::Box>().unwrap();
    let button = children_of(&notice)
        .into_iter()
        .find_map(|child| child.downcast::<gtk4::Button>().ok())
        .expect("the notice carries a \"Show Shorts anyway\" button");
    button.emit_clicked();

    // Revealing Shorts must actually change the render, not just flip
    // an inert flag: the notice is gone and both episodes now render.
    let children = children_of(&detail.content);
    assert!(!children
        .iter()
        .any(|child| child.has_css_class("reprise-shorts-only-notice")));
    assert_eq!(children.len(), 4, "header, controls, and two episode rows");
}
