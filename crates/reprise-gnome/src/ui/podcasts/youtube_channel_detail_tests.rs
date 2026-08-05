//! Tests for `podcasts::youtube_channel_detail`, split out to keep the main module under the 800-line file-size gate.

use std::collections::BTreeMap;

use reprise_core::podcasts::download_state::DownloadState;
use reprise_core::podcasts::{EpisodeRow, PodcastKind};

use super::*;
use crate::ui::podcasts::podcasts_presentation::{
    source_summary, RenderedSourceGroup, SourceSummary,
};

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
        is_new: false,
    }
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

fn render_channel_row(resume: bool) -> gtk4::Widget {
    let host = gtk4::Stack::new();
    let detail = YoutubeChannelDetail::new(&host, false);
    let mut episode = episode(1, Some(3_600));
    if resume {
        episode.position_ms = 1_800_000;
    }
    detail.build_episode_row(&episode, &mut BTreeMap::new(), &mut BTreeMap::new())
}

/// `SRC-16`: the same episode status reads as the same chip on both episode
/// surfaces, including Resume's measured percentage.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn src_16_the_channel_page_renders_the_status_as_a_chip() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let row = render_channel_row(true);
    let chips = descendants(&row)
        .into_iter()
        .filter_map(|widget| widget.downcast::<gtk4::Label>().ok())
        .filter(|label| label.has_css_class("reprise-source-row-chip"))
        .collect::<Vec<_>>();
    assert_eq!(chips.len(), 1);
    assert!(chips[0].text().starts_with("Resume"));
}

/// `SRC-17`: the channel page reserves the same quiet row-menu surface.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn src_17_the_channel_page_hides_its_row_menu_until_hover_focus_or_selection() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let row = render_channel_row(false);
    let menu = descendants(&row)
        .into_iter()
        .find_map(|widget| widget.downcast::<gtk4::MenuButton>().ok())
        .expect("channel episode menu");
    assert_eq!(menu.opacity(), 0.0);
}

/// `POD-20`: adopting the chip and reveal grammar never changes this
/// surface's permanent play glyph.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn pod_20_the_channel_page_keeps_its_permanent_play_glyph() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let row = render_channel_row(false);
    let play = descendants(&row)
        .into_iter()
        .filter_map(|widget| widget.downcast::<gtk4::Image>().ok())
        .find(|image| image.icon_name().as_deref() == Some("media-playback-start-symbolic"))
        .expect("permanent play glyph");
    assert_eq!(play.opacity(), 1.0);
}

/// `POD-20`: channel detail keeps its persistent thumbnail play glyph, but
/// does not swap the loaded marker under the pointer.
#[test]
fn pod_20_channel_detail_keeps_its_play_glyph_without_a_hover_swap() {
    let source = include_str!("youtube_channel_detail.rs");

    assert!(source.contains("media-playback-start-symbolic"));
    assert!(!source.contains("install_playback_hover"));
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

/// `SET-10`: the "Hide Shorts" default seeds new
/// channels' Shorts visibility, but a channel's own explicit toggle
/// still overrides it — turning the global default off must not
/// silently flip a channel someone already set the other way.
#[test]
fn set_10_hide_shorts_default_seeds_new_channels_but_per_channel_override_wins() {
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
fn src_12a_clear_selected_drops_every_channels_selection_and_reports_emptiness() {
    let mut state = YoutubeChannelState::default();
    state.set_selected(7, 11, true);
    state.set_selected(8, 21, true);

    assert!(state.clear_selected());
    assert!(state.selected_ids(7).is_empty());
    assert!(state.selected_ids(8).is_empty());
    assert!(!state.clear_selected());
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

/// `SRC-12a`: Ctrl+A on the channel page takes that page's rendered window and
/// nothing behind it.
///
/// The rule claims this surface, and until now nothing exercised
/// `select_all_visible` at all — the clause was true only by reading the code.
/// The trap it guards is a select-all that reaches past the ten-item window
/// into episodes the user cannot see, which is precisely what a selection
/// built from the unwindowed group would do.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn src_12a_channel_page_select_all_stops_at_the_rendered_window() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();

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

    let host = gtk4::Stack::new();
    let detail = YoutubeChannelDetail::new(&host, false);
    detail.state.borrow_mut().open_channel(7);
    detail.update(
        std::slice::from_ref(&rendered),
        &BTreeMap::new(),
        &[],
        &BTreeMap::new(),
        false,
        Connectivity::Online,
        None,
        None,
    );

    assert!(detail.select_all_visible());

    let selected = detail.state.borrow().selected_ids(7);
    assert_eq!(
        selected.len(),
        10,
        "the window shows ten, so ten is what Ctrl+A may take"
    );
    // The fixture is newest-first (`(1..=12).rev()`), so the ten-item window
    // keeps 12 down to 3 and it is the *lowest* ids that fall off the end —
    // the oldest videos, which is what "past the window" means on this page.
    for episode_id in [1, 2] {
        assert!(
            !selected.contains(&episode_id),
            "episode {episode_id} is past the window and must stay unreachable"
        );
    }
    assert!(
        selected.contains(&12) && selected.contains(&3),
        "the window's own newest and oldest entries are both in reach"
    );
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
        reprise_core::connectivity::Connectivity::Online,
        None,
        None,
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
        reprise_core::connectivity::Connectivity::Online,
        None,
        None,
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
        reprise_core::connectivity::Connectivity::Online,
        None,
        None,
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

fn context_gesture(widget: &gtk4::Widget) -> gtk4::GestureClick {
    let controllers = widget.observe_controllers();
    (0..controllers.n_items())
        .find_map(|index| {
            controllers
                .item(index)?
                .downcast::<gtk4::GestureClick>()
                .ok()
                .filter(|gesture| gesture.button() == gtk4::gdk::BUTTON_SECONDARY)
        })
        .expect("channel episode row secondary-click gesture")
}

fn context_keys(widget: &gtk4::Widget) -> gtk4::EventControllerKey {
    let controllers = widget.observe_controllers();
    (0..controllers.n_items())
        .find_map(|index| {
            controllers
                .item(index)?
                .downcast::<gtk4::EventControllerKey>()
                .ok()
                .filter(|keys| keys.propagation_phase() == gtk4::PropagationPhase::Capture)
        })
        .expect("channel episode row capture-phase context keys")
}

fn attached_popover(widget: &gtk4::Widget) -> gtk4::PopoverMenu {
    std::iter::successors(widget.first_child(), gtk4::prelude::WidgetExt::next_sibling)
        .find_map(|child| child.downcast::<gtk4::PopoverMenu>().ok())
        .expect("channel episode row popover")
}

fn menu_has_action(model: &gtk4::gio::MenuModel, expected: &str) -> bool {
    for item in 0..model.n_items() {
        if model
            .item_attribute_value(item, "action", Some(gtk4::glib::VariantTy::STRING))
            .and_then(|value| value.get::<String>())
            .as_deref()
            == Some(expected)
        {
            return true;
        }
        if model
            .item_link(item, "section")
            .is_some_and(|section| menu_has_action(&section, expected))
        {
            return true;
        }
    }
    false
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn src_14_channel_secondary_click_opens_for_one_row_or_the_three_row_selection() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let stack = gtk4::Stack::new();
    let detail = YoutubeChannelDetail::new(&stack, true);
    stack.add_named(detail.widget(), Some("youtube-channel"));
    let actions = gtk4::gio::SimpleActionGroup::new();
    detail.install_actions(&actions);
    stack.insert_action_group("podcasts", Some(&actions));
    let episodes = (1..=4).map(|id| episode(id, Some(600))).collect::<Vec<_>>();
    let rendered = RenderedSourceGroup {
        summary: SourceSummary {
            episode_count: 4,
            new_count: 0,
            downloaded_bytes: 0,
            latest_published_at: None,
        },
        group: reprise_core::podcasts::SourceGroup {
            subscription_id: 7,
            title: "Channel".into(),
            author: None,
            image_url: None,
            kind: PodcastKind::Youtube,
            sync_to_phone: false,
            episodes,
        },
    };
    detail.update(
        std::slice::from_ref(&rendered),
        &BTreeMap::new(),
        &[],
        &BTreeMap::new(),
        false,
        reprise_core::connectivity::Connectivity::Online,
        None,
        None,
    );
    detail.state.borrow_mut().open_channel(7);
    let selection = detail.state.borrow_mut().selection(7);
    for episode_id in [1, 2, 3] {
        selection.borrow_mut().set_selected(episode_id, true);
    }
    detail.render_active();
    let window = gtk4::Window::new();
    window.set_child(Some(&stack));
    window.present();

    let outside = detail.selection_widgets.borrow()[&4].row.clone();
    assert_eq!(
        context_keys(outside.upcast_ref()).propagation_phase(),
        gtk4::PropagationPhase::Capture
    );
    let menu = outside
        .last_child()
        .and_downcast::<gtk4::MenuButton>()
        .expect("channel detail row discovery menu");
    assert_eq!(menu.icon_name().as_deref(), Some("view-more-symbolic"));
    assert_eq!(
        menu.tooltip_text().as_deref(),
        Some(strings::text(strings::PODCAST_MORE_OPTIONS)).as_deref()
    );
    context_gesture(outside.upcast_ref()).emit_by_name::<()>("pressed", &[&1i32, &8.0f64, &8.0f64]);
    assert_eq!(selection.borrow().selected_ids(), vec![4]);
    let popover = attached_popover(outside.upcast_ref());
    assert!(popover.is_visible());
    popover.popdown();

    selection.borrow_mut().clear();
    for episode_id in [1, 2, 3] {
        selection.borrow_mut().set_selected(episode_id, true);
    }
    let inside = detail.selection_widgets.borrow()[&2].row.clone();
    context_gesture(inside.upcast_ref()).emit_by_name::<()>("pressed", &[&1i32, &8.0f64, &8.0f64]);
    assert_eq!(selection.borrow().selected_ids(), vec![1, 2, 3]);
    let popover = attached_popover(inside.upcast_ref());
    assert!(menu_has_action(
        &popover.menu_model().expect("multi-selection menu model"),
        "podcasts.mark-played-selected"
    ));
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
        reprise_core::connectivity::Connectivity::Online,
        None,
        None,
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

/// `SRC-14`: the detail view selects with the same mechanics as the grouped
/// library view — it owns its per-channel state, but the range walk is the
/// shared one, so the two surfaces cannot drift apart.
#[test]
fn src_14_the_detail_view_ranges_over_its_own_channel() {
    let mut state = YoutubeChannelState::default();

    state.apply_select(5, &[1, 2, 3], 1, SelectMode::Only);
    state.apply_select(5, &[1, 2, 3], 3, SelectMode::Range);

    assert_eq!(state.selected_ids(5), vec![1, 2, 3]);
    assert!(
        state.selected_ids(6).is_empty(),
        "one channel's range never reaches another channel"
    );
}

/// `SRC-14`: each channel keeps its own anchor, so switching channels and
/// coming back does not produce a range from a foreign row.
#[test]
fn src_14_each_channel_anchors_its_own_range() {
    let mut state = YoutubeChannelState::default();

    state.apply_select(5, &[1, 2, 3], 3, SelectMode::Only);
    state.apply_select(6, &[10, 11, 12], 10, SelectMode::Only);
    state.apply_select(5, &[1, 2, 3], 1, SelectMode::Range);

    assert_eq!(state.selected_ids(5), vec![1, 2, 3]);
    assert_eq!(
        state.selected_ids(6),
        vec![10],
        "the other channel's selection is untouched"
    );
}

/// `SRC-14`: emptying a channel's selection drops the channel rather than
/// leaving an empty set behind — the state `set_selected` has always kept.
#[test]
fn src_14_toggling_the_last_row_off_clears_the_channel() {
    let mut state = YoutubeChannelState::default();

    state.apply_select(5, &[1, 2], 1, SelectMode::Toggle);
    state.apply_select(5, &[1, 2], 1, SelectMode::Toggle);

    assert!(state.selected_ids(5).is_empty());
}
