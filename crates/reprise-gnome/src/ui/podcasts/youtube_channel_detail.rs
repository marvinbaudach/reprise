//! YouTube channel windowing, Shorts visibility, and batch-selection surface.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use chrono::Local;
use gtk4::gio;
use gtk4::glib::variant::ToVariant;
use gtk4::prelude::*;
use reprise_core::podcasts::channel_window::{
    available_count, shorts_only_hidden, visible_window, EXTENDED_WINDOW, INITIAL_WINDOW,
};
use reprise_core::podcasts::download_state::DownloadState;
use reprise_core::podcasts::EpisodeRow;

use super::podcasts_context_menu::PodcastSyncDevice;
use super::podcasts_download_presentation;
use super::podcasts_groups::{self, DownloadRowWidgets};
use super::podcasts_presentation::{
    duration, on_phone, relative_date, status_pill, RenderedSourceGroup,
};
use crate::ui::strings;

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
        visible_window(episodes, show_shorts, limit)
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
        available_count(episodes, show_shorts)
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
    /// `POD-12` / `D3`: read-only mirror of the same per-device selection
    /// `podcasts_groups::group_header` already shows on the channel list —
    /// this view never decides selection itself, only displays it (`on_phone`
    /// in `podcasts_presentation`). Writing selection stays exclusively on
    /// the existing channel toggle (`podcasts_context_menu` /
    /// `podcasts_device_sync::install_action`).
    connected_devices: RefCell<Vec<PodcastSyncDevice>>,
    selected_devices: RefCell<BTreeMap<i64, Vec<String>>>,
    /// `NET-1a` / `C1`: `online_sources::network_allowed(conn,
    /// &modules::SOURCE_IMAGES_MODULE)`, refreshed by [`Self::update`] on
    /// every render pass — this view never reads settings itself.
    images_allowed: Cell<bool>,
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
            connected_devices: RefCell::new(Vec::new()),
            selected_devices: RefCell::new(BTreeMap::new()),
            images_allowed: Cell::new(false),
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

    #[allow(clippy::too_many_arguments)]
    pub(super) fn update(
        self: &Rc<Self>,
        groups: &[RenderedSourceGroup],
        download_states: &BTreeMap<i64, DownloadState>,
        connected_devices: &[PodcastSyncDevice],
        selected_devices: &BTreeMap<i64, Vec<String>>,
        images_allowed: bool,
    ) {
        self.groups.replace(groups.to_vec());
        self.download_states.replace(download_states.clone());
        self.connected_devices.replace(connected_devices.to_vec());
        self.selected_devices.replace(selected_devices.clone());
        self.images_allowed.set(images_allowed);
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
        // `POD-13`: the window is empty *and* every entry is a hidden
        // Short — a dedicated message with a way out, rather than a
        // silently blank list that reads as broken.
        let show_shorts = !state.hide_shorts(subscription_id);
        if projected.group.episodes.is_empty()
            && shorts_only_hidden(&rendered.group.episodes, show_shorts)
        {
            self.content
                .append(&self.build_shorts_only_notice(subscription_id));
        }
        let mut widgets = BTreeMap::new();
        for episode in &projected.group.episodes {
            self.content
                .append(&self.build_episode_row(episode, &mut widgets));
        }
        self.download_widgets.replace(widgets);
    }

    /// `POD-13`: "Only Shorts here" — the decision (`shorts_only_hidden`)
    /// is a pure core projection; this only renders it and offers the way
    /// out (revealing Shorts for this channel, the existing per-channel
    /// override `hide_shorts` already provides via its checkbox above).
    fn build_shorts_only_notice(self: &Rc<Self>, subscription_id: i64) -> gtk4::Widget {
        let notice = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
        notice.add_css_class("reprise-shorts-only-notice");
        notice.set_margin_top(24);
        notice.set_margin_bottom(24);
        notice.set_halign(gtk4::Align::Center);
        notice.set_valign(gtk4::Align::Center);
        let title = gtk4::Label::new(Some(&strings::text(strings::YOUTUBE_SHORTS_ONLY_TITLE)));
        title.add_css_class("title-3");
        notice.append(&title);
        let body = gtk4::Label::new(Some(&strings::text(
            strings::YOUTUBE_SHORTS_ONLY_DESCRIPTION,
        )));
        body.add_css_class("dim-label");
        body.set_wrap(true);
        body.set_justify(gtk4::Justification::Center);
        notice.append(&body);
        let show_anyway =
            gtk4::Button::with_label(&strings::text(strings::YOUTUBE_SHOW_SHORTS_ANYWAY));
        show_anyway.add_css_class("suggested-action");
        show_anyway.set_halign(gtk4::Align::Center);
        let weak = Rc::downgrade(self);
        show_anyway.connect_clicked(move |_| {
            let Some(detail) = weak.upgrade() else {
                return;
            };
            detail
                .state
                .borrow_mut()
                .set_hide_shorts(subscription_id, false);
            detail.render_active();
        });
        notice.append(&show_anyway);
        notice.upcast()
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
            self.images_allowed.get(),
        );
        row.append(image.widget());
        let title = gtk4::Label::new(Some(&rendered.group.title));
        title.add_css_class("title-2");
        title.set_xalign(0.0);
        title.set_hexpand(true);
        row.append(&title);
        // `POD-12` / `D3`: read-only "On phone" mirror — same fact and same
        // glyph as `podcasts_groups::group_header`'s indicator, never a
        // second control. Selection is only ever written through the
        // existing channel toggle (context menu / `podcasts_device_sync`).
        let selected_device_ids = self
            .selected_devices
            .borrow()
            .get(&rendered.group.subscription_id)
            .cloned()
            .unwrap_or_default();
        if on_phone(&self.connected_devices.borrow(), &selected_device_ids) {
            let sync = gtk4::Image::from_icon_name("phone-symbolic");
            sync.set_tooltip_text(Some(&strings::text(strings::PODCAST_SYNC_PHONE)));
            row.append(&sync);
        }
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
#[path = "youtube_channel_detail_tests.rs"]
mod tests;
