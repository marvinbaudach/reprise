//! YouTube channel windowing, Shorts visibility, and batch-selection surface.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use chrono::Local;
use gtk4::gio;
use gtk4::glib::variant::ToVariant;
use gtk4::prelude::*;
use reprise_core::podcasts::download_state::DownloadState;
use reprise_core::podcasts::EpisodeRow;

use super::podcasts_download_presentation;
use super::podcasts_groups::{self, DownloadRowWidgets};
use super::podcasts_presentation::{duration, relative_date, status_pill, RenderedSourceGroup};
use crate::ui::strings;

const INITIAL_WINDOW: usize = 10;
const EXTENDED_WINDOW: usize = 40;
const SHORT_MAX_SECONDS: i64 = 180;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct YoutubeChannelState {
    active_channel: Option<i64>,
    visible_limits: BTreeMap<i64, usize>,
    extended_channels: BTreeSet<i64>,
    /// Channels whose Shorts visibility has been explicitly toggled away
    /// from `default_shows_shorts` — the effective per-channel value is
    /// `default_shows_shorts XOR shorts_overridden.contains(id)`.
    shorts_overridden: BTreeSet<i64>,
    /// Seeded from the "Hide Shorts" preference on the Online sources page
    /// (`SET-8`) when the detail surface is built. Defaults to `false`
    /// (Shorts hidden), matching Reprise's historical hardcoded behavior.
    default_shows_shorts: bool,
    selected: BTreeMap<i64, BTreeSet<i64>>,
}

impl YoutubeChannelState {
    pub(super) fn open_channel(&mut self, subscription_id: i64) {
        self.active_channel = Some(subscription_id);
    }

    pub(super) fn close_channel(&mut self) {
        self.active_channel = None;
    }

    pub(super) fn active_channel(&self) -> Option<i64> {
        self.active_channel
    }

    pub(super) fn set_default_shows_shorts(&mut self, show: bool) {
        self.default_shows_shorts = show;
    }

    fn effective_shows_shorts(&self, subscription_id: i64) -> bool {
        self.default_shows_shorts ^ self.shorts_overridden.contains(&subscription_id)
    }

    pub(super) fn visible_episodes<'a>(
        &self,
        subscription_id: i64,
        episodes: &'a [EpisodeRow],
    ) -> Vec<&'a EpisodeRow> {
        let show_shorts = self.effective_shows_shorts(subscription_id);
        let limit = self
            .visible_limits
            .get(&subscription_id)
            .copied()
            .unwrap_or(INITIAL_WINDOW);
        episodes
            .iter()
            .filter(|episode| show_shorts || !is_short(episode))
            .take(limit)
            .collect()
    }

    pub(super) fn set_loaded_limit(&mut self, subscription_id: i64, limit: usize) {
        self.visible_limits
            .insert(subscription_id, limit.max(INITIAL_WINDOW));
        self.extended_channels.insert(subscription_id);
    }

    fn can_load_more(&self, subscription_id: i64, available: usize) -> bool {
        available > 0 && !self.extended_channels.contains(&subscription_id)
    }

    pub(super) fn set_hide_shorts(&mut self, subscription_id: i64, hide: bool) {
        let desired_show = !hide;
        if desired_show == self.default_shows_shorts {
            self.shorts_overridden.remove(&subscription_id);
        } else {
            self.shorts_overridden.insert(subscription_id);
        }
    }

    pub(super) fn set_selected(&mut self, subscription_id: i64, episode_id: i64, selected: bool) {
        let channel = self.selected.entry(subscription_id).or_default();
        if selected {
            channel.insert(episode_id);
        } else {
            channel.remove(&episode_id);
        }
        if channel.is_empty() {
            self.selected.remove(&subscription_id);
        }
    }

    pub(super) fn selected_ids(&self, subscription_id: i64) -> Vec<i64> {
        self.selected
            .get(&subscription_id)
            .map_or_else(Vec::new, |selected| selected.iter().copied().collect())
    }

    fn hide_shorts(&self, subscription_id: i64) -> bool {
        !self.effective_shows_shorts(subscription_id)
    }

    fn available_count(&self, subscription_id: i64, episodes: &[EpisodeRow]) -> usize {
        let show_shorts = self.effective_shows_shorts(subscription_id);
        episodes
            .iter()
            .filter(|episode| show_shorts || !is_short(episode))
            .count()
    }

    fn retain_selected(&mut self, subscription_id: i64, available: &[EpisodeRow]) {
        let Some(selected) = self.selected.get_mut(&subscription_id) else {
            return;
        };
        selected.retain(|episode_id| available.iter().any(|episode| episode.id == *episode_id));
        if selected.is_empty() {
            self.selected.remove(&subscription_id);
        }
    }

    fn clear_selected(&mut self, subscription_id: i64) {
        self.selected.remove(&subscription_id);
    }
}

fn is_short(episode: &EpisodeRow) -> bool {
    episode
        .duration_secs
        .is_some_and(|seconds| (0..=SHORT_MAX_SECONDS).contains(&seconds))
}

fn project_channel(
    rendered: &RenderedSourceGroup,
    state: &YoutubeChannelState,
) -> RenderedSourceGroup {
    let mut projected = rendered.clone();
    projected.group.episodes = state
        .visible_episodes(rendered.group.subscription_id, &rendered.group.episodes)
        .into_iter()
        .cloned()
        .collect();
    projected
}

pub(super) struct YoutubeChannelDetail {
    root: gtk4::ScrolledWindow,
    content: gtk4::Box,
    host: gtk4::Stack,
    state: RefCell<YoutubeChannelState>,
    groups: RefCell<Vec<RenderedSourceGroup>>,
    download_states: RefCell<BTreeMap<i64, DownloadState>>,
    download_widgets: RefCell<BTreeMap<i64, DownloadRowWidgets>>,
}

impl YoutubeChannelDetail {
    /// `default_hide_shorts` seeds every channel's initial Shorts
    /// visibility from the "Hide Shorts" row on the Online sources page
    /// (`SET-8`); a channel's own explicit toggle always overrides it.
    pub(super) fn new(host: &gtk4::Stack, default_hide_shorts: bool) -> Rc<Self> {
        let content = gtk4::Box::new(gtk4::Orientation::Vertical, 10);
        content.set_margin_top(12);
        content.set_margin_bottom(12);
        content.set_margin_start(12);
        content.set_margin_end(12);
        let root = gtk4::ScrolledWindow::builder()
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .child(&content)
            .build();
        let mut state = YoutubeChannelState::default();
        state.set_default_shows_shorts(!default_hide_shorts);
        Rc::new(Self {
            root,
            content,
            host: host.clone(),
            state: RefCell::new(state),
            groups: RefCell::new(Vec::new()),
            download_states: RefCell::new(BTreeMap::new()),
            download_widgets: RefCell::new(BTreeMap::new()),
        })
    }

    pub(super) fn widget(&self) -> &gtk4::Widget {
        self.root.upcast_ref()
    }

    pub(super) fn install_actions(self: &Rc<Self>, group: &gio::SimpleActionGroup) {
        let open = gio::SimpleAction::new("open-channel", Some(&i64::static_variant_type()));
        let weak = Rc::downgrade(self);
        open.connect_activate(move |_, target| {
            let Some(detail) = weak.upgrade() else { return };
            let Some(id) = target.and_then(gtk4::glib::Variant::get::<i64>) else {
                return;
            };
            detail.state.borrow_mut().open_channel(id);
            detail.render_active();
            detail.host.set_visible_child_name("youtube-channel");
        });
        group.add_action(&open);
        let close = gio::SimpleAction::new("close-channel", None);
        let weak = Rc::downgrade(self);
        close.connect_activate(move |_, _| {
            if let Some(detail) = weak.upgrade() {
                detail.state.borrow_mut().close_channel();
                detail.host.set_visible_child_name("list");
            }
        });
        group.add_action(&close);
    }

    pub(super) fn update(
        self: &Rc<Self>,
        groups: &[RenderedSourceGroup],
        download_states: &BTreeMap<i64, DownloadState>,
    ) {
        self.groups.replace(groups.to_vec());
        self.download_states.replace(download_states.clone());
        let active = self.state.borrow().active_channel();
        if active.is_some_and(|id| !groups.iter().any(|group| group.group.subscription_id == id)) {
            self.state.borrow_mut().close_channel();
            self.host.set_visible_child_name("list");
        } else if active.is_some() {
            self.render_active();
        }
    }

    pub(super) fn is_active(&self) -> bool {
        self.state.borrow().active_channel().is_some()
    }

    pub(super) fn update_download_state(&self, episode_id: i64, state: &DownloadState) {
        self.download_states
            .borrow_mut()
            .insert(episode_id, state.clone());
        let widgets = self.download_widgets.borrow().get(&episode_id).cloned();
        if let Some(widgets) = widgets {
            podcasts_groups::update_download_state(&widgets, state);
        }
    }

    pub(super) fn set_loaded_limit(&self, subscription_id: i64, limit: usize) {
        self.state
            .borrow_mut()
            .set_loaded_limit(subscription_id, limit);
    }

    fn render_active(self: &Rc<Self>) {
        let Some(subscription_id) = self.state.borrow().active_channel() else {
            return;
        };
        let Some(rendered) = self
            .groups
            .borrow()
            .iter()
            .find(|group| group.group.subscription_id == subscription_id)
            .cloned()
        else {
            return;
        };
        self.state
            .borrow_mut()
            .retain_selected(subscription_id, &rendered.group.episodes);
        let state = self.state.borrow().clone();
        let projected = project_channel(&rendered, &state);
        while let Some(child) = self.content.first_child() {
            self.content.remove(&child);
        }
        self.content.append(&self.build_header(&rendered));
        self.content
            .append(&self.build_controls(&rendered, &projected));
        let mut widgets = BTreeMap::new();
        for episode in &projected.group.episodes {
            self.content
                .append(&self.build_episode_row(episode, &mut widgets));
        }
        self.download_widgets.replace(widgets);
    }

    fn build_header(self: &Rc<Self>, rendered: &RenderedSourceGroup) -> gtk4::Widget {
        let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
        let back = gtk4::Button::from_icon_name("go-previous-symbolic");
        back.add_css_class("flat");
        back.set_tooltip_text(Some(&strings::text(strings::YOUTUBE_BACK_TO_CHANNELS)));
        back.set_action_name(Some("podcasts.close-channel"));
        row.append(&back);
        let image = super::source_image::SourceImage::new(
            rendered.group.image_url.as_deref(),
            "video-x-generic-symbolic",
            48,
        );
        row.append(image.widget());
        let title = gtk4::Label::new(Some(&rendered.group.title));
        title.add_css_class("title-2");
        title.set_xalign(0.0);
        title.set_hexpand(true);
        row.append(&title);
        let unsubscribe = gtk4::Button::with_label(&strings::text(strings::PODCAST_UNSUBSCRIBE));
        unsubscribe.add_css_class("destructive-action");
        unsubscribe.set_action_name(Some("podcasts.unsubscribe"));
        unsubscribe.set_action_target_value(Some(&rendered.group.subscription_id.to_variant()));
        row.append(&unsubscribe);
        row.upcast()
    }

    fn build_controls(
        self: &Rc<Self>,
        rendered: &RenderedSourceGroup,
        projected: &RenderedSourceGroup,
    ) -> gtk4::Widget {
        let subscription_id = rendered.group.subscription_id;
        let controls = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        controls.add_css_class("toolbar");
        let shown = projected.group.episodes.len();
        let available = {
            let state = self.state.borrow();
            state.available_count(subscription_id, &rendered.group.episodes)
        };
        // POD-11: downloaded count/bytes cover the whole channel, not just
        // the visible window or the Shorts-filtered set, so the total reads
        // as real disk usage.
        let download_states = self.download_states.borrow().clone();
        let summary = podcasts_download_presentation::channel_download_summary(
            shown,
            available,
            &rendered.group.episodes,
            &download_states,
        );
        let window = gtk4::Label::new(Some(&strings::youtube_channel_summary(
            summary.shown,
            summary.available,
            summary.downloaded_count,
            summary.downloaded_bytes,
        )));
        window.set_hexpand(true);
        window.set_xalign(0.0);
        controls.append(&window);
        let load_more = gtk4::Button::with_label(&strings::text(strings::YOUTUBE_LOAD_MORE));
        let can_load_more = self
            .state
            .borrow()
            .can_load_more(subscription_id, available);
        load_more.set_visible(can_load_more);
        load_more.set_action_name(Some("podcasts.load-more"));
        load_more.set_action_target_value(Some(
            &(
                subscription_id,
                u32::try_from(EXTENDED_WINDOW).unwrap_or(40),
            )
                .to_variant(),
        ));
        controls.append(&load_more);
        let hide_shorts =
            gtk4::CheckButton::with_label(&strings::text(strings::YOUTUBE_HIDE_SHORTS));
        let shorts_hidden = self.state.borrow().hide_shorts(subscription_id);
        hide_shorts.set_active(shorts_hidden);
        let weak = Rc::downgrade(self);
        hide_shorts.connect_toggled(move |toggle| {
            if let Some(detail) = weak.upgrade() {
                detail
                    .state
                    .borrow_mut()
                    .set_hide_shorts(subscription_id, toggle.is_active());
                detail.render_active();
            }
        });
        controls.append(&hide_shorts);
        let selected = gtk4::Label::new(None);
        let download = gtk4::Button::with_label(&strings::text(strings::YOUTUBE_DOWNLOAD_SELECTED));
        let remove = gtk4::Button::with_label(&strings::text(strings::YOUTUBE_REMOVE_SELECTED));
        remove.add_css_class("destructive-action");
        let state = self.state.borrow().clone();
        update_batch_controls(&state, subscription_id, &selected, &download, &remove);
        self.wire_batch_actions(subscription_id, &download, &remove);
        controls.append(&selected);
        controls.append(&download);
        controls.append(&remove);
        controls.upcast()
    }

    fn wire_batch_actions(
        self: &Rc<Self>,
        subscription_id: i64,
        download: &gtk4::Button,
        remove: &gtk4::Button,
    ) {
        let weak = Rc::downgrade(self);
        download.connect_clicked(move |_| {
            let Some(detail) = weak.upgrade() else { return };
            let ids = detail.state.borrow().selected_ids(subscription_id);
            let states = detail.download_states.borrow().clone();
            for id in ids {
                if !matches!(
                    states.get(&id),
                    Some(
                        DownloadState::Queued
                            | DownloadState::Downloading { .. }
                            | DownloadState::Downloaded { .. }
                    )
                ) {
                    let _ = detail
                        .host
                        .activate_action("podcasts.toggle-download", Some(&id.to_variant()));
                }
            }
        });
        let weak = Rc::downgrade(self);
        remove.connect_clicked(move |_| {
            let Some(detail) = weak.upgrade() else { return };
            let ids = detail.state.borrow().selected_ids(subscription_id);
            detail.state.borrow_mut().clear_selected(subscription_id);
            for id in ids {
                let _ = detail
                    .host
                    .activate_action("podcasts.remove-episode", Some(&id.to_variant()));
            }
        });
    }

    fn build_episode_row(
        self: &Rc<Self>,
        episode: &EpisodeRow,
        widgets: &mut BTreeMap<i64, DownloadRowWidgets>,
    ) -> gtk4::Widget {
        let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        row.add_css_class("reprise-podcast-episode-row");
        let selected = gtk4::CheckButton::new();
        let is_selected = self
            .state
            .borrow()
            .selected_ids(episode.subscription_id)
            .contains(&episode.id);
        selected.set_active(is_selected);
        let weak = Rc::downgrade(self);
        let episode_id = episode.id;
        let subscription_id = episode.subscription_id;
        selected.connect_toggled(move |selected| {
            if let Some(detail) = weak.upgrade() {
                detail.state.borrow_mut().set_selected(
                    subscription_id,
                    episode_id,
                    selected.is_active(),
                );
                detail.render_active();
            }
        });
        row.append(&selected);
        let play = gtk4::Button::from_icon_name("media-playback-start-symbolic");
        play.add_css_class("flat");
        play.set_action_name(Some("podcasts.play"));
        play.set_action_target_value(Some(&episode.id.to_variant()));
        row.append(&play);
        let copy = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
        copy.set_hexpand(true);
        let title = gtk4::Label::new(Some(&episode.title));
        title.set_xalign(0.0);
        title.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        copy.append(&title);
        let subtitle = gtk4::Label::new(Some(&format!(
            "{} · {} · {}",
            relative_date(episode.published_at, Local::now().date_naive()),
            duration(episode.duration_secs),
            status_pill(episode).label
        )));
        subtitle.set_xalign(0.0);
        subtitle.add_css_class("caption");
        subtitle.add_css_class("dim-label");
        copy.append(&subtitle);
        row.append(&copy);
        let status = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
        let action = gtk4::Button::new();
        action.add_css_class("flat");
        action.set_action_name(Some("podcasts.toggle-download"));
        action.set_action_target_value(Some(&episode.id.to_variant()));
        let download_widgets = DownloadRowWidgets { status, action };
        let state = self
            .download_states
            .borrow()
            .get(&episode.id)
            .cloned()
            .unwrap_or(DownloadState::NotDownloaded);
        podcasts_groups::update_download_state(&download_widgets, &state);
        row.append(&download_widgets.status);
        row.append(&download_widgets.action);
        widgets.insert(episode.id, download_widgets);
        row.upcast()
    }
}

fn update_batch_controls(
    state: &YoutubeChannelState,
    subscription_id: i64,
    selected: &gtk4::Label,
    download: &gtk4::Button,
    remove: &gtk4::Button,
) {
    let count = state.selected_ids(subscription_id).len();
    selected.set_text(&strings::youtube_selected_count(count));
    download.set_sensitive(count > 0);
    remove.set_sensitive(count > 0);
}

#[cfg(test)]
mod tests {
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

    /// `SET-8`: the Online sources page's "Hide Shorts" default seeds new
    /// channels' Shorts visibility, but a channel's own explicit toggle
    /// still overrides it — turning the global default off must not
    /// silently flip a channel someone already set the other way.
    #[test]
    fn set_8_hide_shorts_default_seeds_new_channels_but_per_channel_override_wins() {
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
}
