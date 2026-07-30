//! Podcasts table, status states, actions, and refresh wiring.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use gtk4::gio;
use gtk4::glib::{self};
use gtk4::prelude::*;
use libadwaita as adw;
use reprise_core::connectivity::Connectivity;
use reprise_core::db::Db;
use reprise_core::podcasts::download_state::DownloadState;
use reprise_core::podcasts::{self, EpisodeRow, PodcastKind, SourceGroup};

use super::add_dialog;
use super::podcasts_context_menu;
use super::podcasts_device_sync::PodcastDeviceSyncState;
use super::podcasts_download_presentation::refreshed_download_states;
use super::podcasts_empty_state::{podcasts_empty_state_for, PodcastsEmptyState};
use super::podcasts_filter_bar::PodcastsFilterBar;
use super::podcasts_groups;
use super::podcasts_presentation::{
    active as filter_active, apply_filter, library_summary, rendered_source_groups,
    sort_newest_first,
};
use super::podcasts_removal::{
    download_commit_action, download_request_allowed, download_toggle_action, DownloadCommitAction,
    DownloadToggleAction, KeptDownloads,
};
use super::podcasts_scroller::build_episode_scroller;
use super::podcasts_view_data::{last_updated_text, unique};
use super::podcasts_worker::{
    podcasts_response_channel, request_generation, PodcastsOperation, PodcastsPriority,
    PodcastsRequest, PodcastsRuntime, PodcastsWorkerResult,
};
use super::youtube_channel_detail::YoutubeChannelDetail;
use crate::ui::source_empty_state::SourceEmptyState;
use crate::ui::strings;

#[path = "podcasts_view_actions.rs"]
mod actions;
#[path = "podcasts_view_copy.rs"]
mod copy;
#[path = "podcasts_view_requests.rs"]
mod requests;
#[cfg(test)]
#[path = "podcasts_view_tests.rs"]
mod tests;

/// `SRC-10`: the stack page holding the shared empty-state geometry, used
/// only for "nothing subscribed yet".
const EMPTY_PAGE: &str = "empty";
/// `SRC-10` addendum (Block B2): the module-off sibling of `EMPTY_PAGE` —
/// same geometry, "Enable in Preferences" instead of Add.
const MODULE_OFF_PAGE: &str = "module-off";

type OnEpisodeActivated = Rc<dyn Fn(EpisodeRow)>;
type OnSubscriptionRemoved = Rc<dyn Fn(i64)>;
type OnSidebarRefresh = Rc<dyn Fn()>;

#[derive(Clone)]
pub(in crate::ui) struct PodcastsCallbacks {
    on_episode_activated: OnEpisodeActivated,
    on_subscription_removed: OnSubscriptionRemoved,
    on_sidebar_refresh: OnSidebarRefresh,
}

impl Default for PodcastsCallbacks {
    fn default() -> Self {
        Self {
            on_episode_activated: Rc::new(|_| {}),
            on_subscription_removed: Rc::new(|_| {}),
            on_sidebar_refresh: Rc::new(|| {}),
        }
    }
}

impl PodcastsCallbacks {
    pub(in crate::ui) fn new(
        on_episode_activated: impl Fn(EpisodeRow) + 'static,
        on_subscription_removed: impl Fn(i64) + 'static,
        on_sidebar_refresh: impl Fn() + 'static,
    ) -> Self {
        Self {
            on_episode_activated: Rc::new(on_episode_activated),
            on_subscription_removed: Rc::new(on_subscription_removed),
            on_sidebar_refresh: Rc::new(on_sidebar_refresh),
        }
    }
}

pub(in crate::ui) struct PodcastsView {
    root: gtk4::Box,
    pub(super) conn: Rc<Db>,
    runtime: Rc<PodcastsRuntime>,
    callbacks: PodcastsCallbacks,
    kind: PodcastKind,
    filter_bar: Rc<PodcastsFilterBar>,
    group_container: gtk4::Box,
    stack: gtk4::Stack,
    youtube_detail: Rc<YoutubeChannelDetail>,
    status: adw::StatusPage,
    status_button: gtk4::Button,
    empty_state: SourceEmptyState,
    /// `SRC-10` addendum (Block B2): the module-off sibling state's own
    /// page — a second `SourceEmptyState` rather than reusing `empty_state`
    /// with a swapped copy, since the two need different button actions
    /// (open the add dialog vs. open Preferences) and `SourceEmptyState`
    /// wires exactly one `connect_add` callback for its lifetime.
    module_off_state: SourceEmptyState,
    /// Set post-construction (parallel to `set_toast_overlay`) once
    /// Preferences exists — `PodcastsView` is built before it in `window.rs`.
    on_open_preferences: RefCell<Option<Rc<dyn Fn()>>>,
    footer: gtk4::Box,
    footer_status: gtk4::Label,
    footer_spinner: gtk4::Spinner,
    groups: RefCell<Vec<SourceGroup>>,
    rows: RefCell<Vec<EpisodeRow>>,
    pub(super) device_sync: PodcastDeviceSyncState,
    expanded_sources: Rc<RefCell<BTreeSet<i64>>>,
    download_states: Rc<RefCell<BTreeMap<i64, DownloadState>>>,
    download_widgets: RefCell<BTreeMap<i64, podcasts_groups::DownloadRowWidgets>>,
    playing_episode: Cell<Option<i64>>,
    generation: Cell<u64>,
    toast_overlay: glib::WeakRef<adw::ToastOverlay>,
    kept_downloads: RefCell<KeptDownloads>,
    /// `NET-3c`: explicit, injectable connectivity seam, mirroring
    /// `RadioView`'s (`NET-3b`) — defaults to `Online` and is not wired to
    /// any real OS signal yet; only [`PodcastsView::set_connectivity`] (and
    /// tests) change it. A transition from `Offline` to `Online` triggers
    /// the queued-download runner.
    connectivity: Cell<Connectivity>,
}

impl PodcastsView {
    pub(in crate::ui) fn install(
        conn: Rc<Db>,
        runtime: Rc<PodcastsRuntime>,
        callbacks: PodcastsCallbacks,
        kind: PodcastKind,
    ) -> Rc<Self> {
        let filter_bar = PodcastsFilterBar::new(conn.clone(), kind);
        let group_container = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
        group_container.set_margin_top(8);
        group_container.set_margin_bottom(8);
        group_container.set_margin_start(12);
        group_container.set_margin_end(12);
        group_container.set_hexpand(true);
        let scroller = build_episode_scroller(group_container.upcast_ref::<gtk4::Widget>());

        let status = adw::StatusPage::new();
        let status_button = gtk4::Button::new();
        status_button.add_css_class("suggested-action");
        status.set_child(Some(&status_button));
        let empty_state = SourceEmptyState::new(&copy::empty_state_copy(kind));
        let module_off_state = SourceEmptyState::new(&copy::module_off_copy(kind));
        let stack = gtk4::Stack::new();
        stack.add_named(&scroller, Some("list"));
        stack.add_named(&status, Some("status"));
        stack.add_named(empty_state.widget(), Some(EMPTY_PAGE));
        stack.add_named(module_off_state.widget(), Some(MODULE_OFF_PAGE));
        let default_hide_shorts =
            podcasts::config::load(&conn).map_or(true, |config| config.youtube_hide_shorts_default);
        let youtube_detail = YoutubeChannelDetail::new(&stack, default_hide_shorts);
        stack.add_named(youtube_detail.widget(), Some("youtube-channel"));
        stack.set_vexpand(true);

        let footer = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        footer.set_margin_top(6);
        footer.set_margin_bottom(6);
        footer.set_margin_start(12);
        footer.set_margin_end(12);
        let footer_spinner = gtk4::Spinner::new();
        footer.append(&footer_spinner);
        let footer_status = gtk4::Label::new(None);
        footer_status.add_css_class("caption");
        footer_status.add_css_class("dim-label");
        footer_status.set_hexpand(true);
        footer_status.set_xalign(0.0);
        footer.append(&footer_status);
        let refresh = gtk4::Button::with_label(&strings::text(strings::PODCAST_REFRESH_NOW));
        refresh.add_css_class("flat");
        footer.append(&refresh);

        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        root.add_css_class("reprise-podcasts-source");
        // `SRC-10` addendum (Block B2): the filter row lives at this level,
        // not inside the "list" stack page, so its visibility can be
        // decided independently of which page is showing — visible for
        // `List`/`NoEpisodes`/`NoResults`/`NoDownloads`, hidden for the two
        // whole-page-replaced states `Empty`/`ModuleOff`.
        root.append(filter_bar.widget());
        root.append(&stack);
        root.append(&footer);

        let view = Rc::new(Self {
            root,
            conn,
            runtime,
            callbacks,
            kind,
            filter_bar,
            group_container,
            stack,
            youtube_detail,
            status,
            status_button,
            empty_state,
            module_off_state,
            on_open_preferences: RefCell::new(None),
            footer,
            footer_status,
            footer_spinner,
            groups: RefCell::new(Vec::new()),
            rows: RefCell::new(Vec::new()),
            device_sync: PodcastDeviceSyncState::default(),
            expanded_sources: Rc::new(RefCell::new(BTreeSet::new())),
            download_states: Rc::new(RefCell::new(BTreeMap::new())),
            download_widgets: RefCell::new(BTreeMap::new()),
            playing_episode: Cell::new(None),
            generation: Cell::new(0),
            toast_overlay: glib::WeakRef::new(),
            kept_downloads: RefCell::new(KeptDownloads::default()),
            connectivity: Cell::new(Connectivity::default()),
        });
        view.install_actions();
        view.wire_controls(&refresh);
        let weak = Rc::downgrade(&view);
        view.empty_state.connect_add(move || {
            if let Some(view) = weak.upgrade() {
                view.open_add_dialog();
            }
        });
        let weak = Rc::downgrade(&view);
        view.module_off_state.connect_add(move || {
            if let Some(view) = weak.upgrade() {
                if let Some(callback) = view.on_open_preferences.borrow().clone() {
                    callback();
                }
            }
        });
        view.refresh();
        view
    }

    /// Wires the module-off empty state's "Enable in Preferences" button.
    /// Set post-construction because `PodcastsView` is built before the
    /// `Preferences` context exists in `window.rs` (mirrors
    /// `set_toast_overlay`).
    pub(in crate::ui) fn set_on_open_preferences(&self, callback: impl Fn() + 'static) {
        *self.on_open_preferences.borrow_mut() = Some(Rc::new(callback));
    }

    pub(in crate::ui) fn root(&self) -> &gtk4::Widget {
        self.root.upcast_ref()
    }

    pub(in crate::ui) fn set_toast_overlay(&self, overlay: &adw::ToastOverlay) {
        self.toast_overlay.set(Some(overlay));
    }

    pub(in crate::ui) fn set_playing_episode(&self, episode_id: Option<i64>) {
        self.playing_episode.set(episode_id);
        self.render();
    }

    /// `NET-3c`: sets the connectivity seam this view consults — see the
    /// `connectivity` field doc. Not wired to any real OS signal yet, same
    /// as `RadioView::set_connectivity` (`NET-3b`). A transition from
    /// `Offline` to `Online` dispatches the queued-download runner; every
    /// other transition (including staying `Online` or staying `Offline`)
    /// is a no-op so re-asserting the same state never replays anything.
    pub(in crate::ui) fn set_connectivity(self: &Rc<Self>, value: Connectivity) {
        let previous = self.connectivity.replace(value);
        if previous == Connectivity::Offline && value == Connectivity::Online {
            self.request_run_queued();
        }
    }

    /// `NET-3` point 4 (F4): the add dialog reads this once, at present
    /// time, to decide whether to disable search and to route a pasted URL
    /// through the offline path instead of a live preview fetch.
    pub(in crate::ui) fn connectivity(&self) -> Connectivity {
        self.connectivity.get()
    }

    pub(in crate::ui) fn bind_device_sync(
        self: &Rc<Self>,
        runtime: &Rc<crate::ui::device_sync_runtime::DeviceSyncRuntime>,
    ) {
        super::podcasts_device_sync::bind(self, runtime);
    }

    pub(in crate::ui) fn refresh(&self) {
        let result = {
            podcasts::query::list_source_groups(&self.conn, self.kind).and_then(|groups| {
                let selected = PodcastDeviceSyncState::selected_for_groups(&self.conn, &groups)?;
                Ok((groups, selected))
            })
        };
        match result {
            Ok((groups, selected_devices)) => {
                let mut rows = groups
                    .iter()
                    .flat_map(|group| group.episodes.iter().cloned())
                    .collect::<Vec<_>>();
                sort_newest_first(&mut rows);
                let previous = self.download_states.borrow().clone();
                self.download_states
                    .replace(refreshed_download_states(&rows, &previous));
                self.groups.replace(groups);
                self.rows.replace(rows);
                self.device_sync.replace_selected(selected_devices);
                let last_updated = last_updated_text(&self.conn);
                self.footer_status.set_text(&last_updated);
                self.render();
            }
            Err(error) => self.footer_status.set_text(&error.to_string()),
        }
    }

    fn wire_controls(self: &Rc<Self>, refresh_button: &gtk4::Button) {
        let weak = Rc::downgrade(self);
        refresh_button.connect_clicked(move |_| {
            if let Some(view) = weak.upgrade() {
                view.request_refresh(true);
            }
        });
        let weak = Rc::downgrade(self);
        self.filter_bar.set_on_changed(move |_| {
            if let Some(view) = weak.upgrade() {
                view.render();
            }
        });
        let weak = Rc::downgrade(self);
        self.status_button.connect_clicked(move |_| {
            let Some(view) = weak.upgrade() else {
                return;
            };
            // `SRC-10` moved the "nothing subscribed yet" empty state onto
            // its own page with its own button (see `open_add_dialog` wiring
            // in `install`); this button is now reachable only for
            // `NoEpisodes`/`NoResults`, both subscribed states.
            if filter_active(&view.filter_bar.filter()) {
                view.filter_bar.clear_all();
            } else {
                view.request_refresh(true);
            }
        });
    }

    pub(super) fn render(&self) {
        let rows = self.rows.borrow().clone();
        let groups = self.groups.borrow().clone();
        let download_states = self.download_states.borrow().clone();
        let connected_devices = self.device_sync.connected();
        let selected_devices = self.device_sync.selected();
        let filter = self.filter_bar.filter();
        let filtered = apply_filter(&rows, &filter);
        let total = rows.len();
        let shows = groups
            .iter()
            .map(|group| group.title.clone())
            .collect::<Vec<_>>();
        let rendered_groups = rendered_source_groups(&groups, &filter, &download_states);
        // `NET-1a` / `C1`: computed once per render pass from the live
        // module + global-gate state, then threaded down to every source
        // image entry point in this view instead of each one re-deriving it.
        let images_allowed = reprise_core::online_sources::network_allowed(
            &self.conn,
            &reprise_core::modules::SOURCE_IMAGES_MODULE,
        )
        .unwrap_or(false);
        self.youtube_detail.update(
            &rendered_groups,
            &download_states,
            &connected_devices,
            &selected_devices,
            images_allowed,
        );
        let download_widgets = podcasts_groups::replace(
            &self.group_container,
            &rendered_groups,
            self.playing_episode.get(),
            &self.expanded_sources,
            &download_states,
            &connected_devices,
            &selected_devices,
            images_allowed,
        );
        self.download_widgets.replace(download_widgets);
        // `G2` (design 6a): the header line is a projection over the
        // unfiltered `groups`, not `rendered_groups` — it stays a stable
        // library overview instead of jittering with the active filter.
        self.filter_bar
            .set_context(unique(shows), filtered.len(), library_summary(&groups));
        let subscriptions = groups.len();
        // `G1`/`NET-1a`: the same combined gate the sidebar already uses to
        // decide whether this source's row is even reachable — one
        // authority for "is this source usable" rather than a second,
        // possibly-diverging check here.
        let module = match self.kind {
            PodcastKind::Rss => &reprise_core::modules::PODCASTS_MODULE,
            PodcastKind::Youtube => &reprise_core::modules::YOUTUBE_MODULE,
        };
        let module_enabled =
            reprise_core::online_sources::network_allowed(&self.conn, module).unwrap_or(false);
        let classification = podcasts_empty_state_for(
            subscriptions,
            total,
            filtered.len(),
            filter_active(&filter),
            filter.downloaded_only,
            module_enabled,
        );
        // `SRC-10`: the two whole-page-replaced states (`Empty`/
        // `ModuleOff`) hide the footer's refresh row too — refreshing zero
        // or switched-off subscriptions has nothing to do, and a live
        // control would make an intentionally unused view look broken
        // instead. `NoEpisodes` keeps the footer (a manual refresh is still
        // meaningful there) but not the filter row — the filter row is
        // reachable only where clearing it is actually the way out:
        // `List`, `NoResults`, `NoDownloads`.
        let whole_page_replaced = matches!(
            classification,
            PodcastsEmptyState::Empty | PodcastsEmptyState::ModuleOff
        );
        self.footer.set_visible(!whole_page_replaced);
        self.filter_bar.widget().set_visible(matches!(
            classification,
            PodcastsEmptyState::List
                | PodcastsEmptyState::NoResults
                | PodcastsEmptyState::NoDownloads
        ));
        match classification {
            PodcastsEmptyState::List => self.stack.set_visible_child_name("list"),
            PodcastsEmptyState::Empty => self.stack.set_visible_child_name(EMPTY_PAGE),
            PodcastsEmptyState::ModuleOff => {
                self.stack.set_visible_child_name(MODULE_OFF_PAGE);
            }
            state => {
                let (title, description, button) = copy::status_copy(state);
                self.status.set_title(&title);
                self.status.set_description(Some(&description));
                self.status_button.set_label(&button);
                self.stack.set_visible_child_name("status");
            }
        }
        if self.youtube_detail.is_active() {
            self.stack.set_visible_child_name("youtube-channel");
        }
    }

    fn toggle_download(self: &Rc<Self>, episode_id: i64) {
        let allowed = {
            let states = self.download_states.borrow();
            download_request_allowed(states.get(&episode_id))
        };
        if !allowed {
            return;
        }
        let Ok(Some(row)) = podcasts::store::episode(&self.conn, episode_id) else {
            return;
        };
        if let Some(path) = row.downloaded_path.as_deref() {
            let file_exists = std::path::Path::new(path).is_file();
            if download_toggle_action(Some(path), file_exists) == DownloadToggleAction::Trash {
                let file = gio::File::for_path(path);
                if let Err(error) = file.trash(None::<&gio::Cancellable>) {
                    self.show_error(&error.to_string());
                    return;
                }
            }
            if let Err(error) = podcasts::store::set_downloaded_path(&self.conn, episode_id, None) {
                self.show_error(&error.to_string());
                return;
            }
            self.download_states
                .borrow_mut()
                .insert(episode_id, DownloadState::NotDownloaded);
            if file_exists {
                self.refresh();
                return;
            }
        }
        let operation = PodcastsOperation::Download { episode_id };
        let generation = request_generation(self.generation.get(), operation);
        let (response, receiver) = podcasts_response_channel();
        if !self.runtime.request(PodcastsRequest {
            generation,
            operation,
            priority: PodcastsPriority::Normal,
            response,
        }) {
            return;
        }
        self.set_download_state(episode_id, &DownloadState::Queued);
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            while let Ok(response) = receiver.recv().await {
                let Some(view) = weak.upgrade() else {
                    return;
                };
                match response.result {
                    Ok(PodcastsWorkerResult::DownloadState { episode_id, state }) => {
                        let terminal = matches!(
                            state,
                            DownloadState::Downloaded { .. } | DownloadState::Failed { .. }
                        );
                        view.set_download_state(episode_id, &state);
                        if matches!(state, DownloadState::Downloaded { .. }) {
                            view.refresh();
                        }
                        if terminal {
                            break;
                        }
                    }
                    Ok(PodcastsWorkerResult::Refreshed(_)) => {}
                    Ok(PodcastsWorkerResult::LoadedMore { .. }) => {}
                    Ok(PodcastsWorkerResult::QueueRunComplete { .. }) => {}
                    Err(error) => {
                        view.set_download_state(
                            episode_id,
                            &DownloadState::Failed {
                                message: error.clone(),
                            },
                        );
                        view.show_error(&error);
                        break;
                    }
                }
            }
        });
    }

    fn set_download_state(&self, episode_id: i64, state: &DownloadState) {
        self.download_states
            .borrow_mut()
            .insert(episode_id, state.clone());
        let widgets = self.download_widgets.borrow().get(&episode_id).cloned();
        if let Some(widgets) = widgets {
            podcasts_groups::update_download_state(&widgets, state);
        }
        self.youtube_detail.update_download_state(episode_id, state);
    }

    fn unsubscribe(self: &Rc<Self>, subscription_id: i64) {
        let Ok(Some(subscription)) = podcasts::store::subscription(&self.conn, subscription_id)
        else {
            return;
        };
        let paths = podcasts::store::downloaded_paths_for_subscription(&self.conn, subscription_id)
            .unwrap_or_default();
        if let Err(error) = podcasts::store::tombstone_subscription(
            &self.conn,
            subscription_id,
            chrono::Utc::now().timestamp(),
        ) {
            self.show_error(&error.to_string());
            return;
        }
        (self.callbacks.on_subscription_removed)(subscription_id);
        (self.callbacks.on_sidebar_refresh)();
        self.refresh();

        let Some(overlay) = self.toast_overlay.upgrade() else {
            self.kept_downloads.borrow_mut().add(subscription_id, paths);
            if let Err(error) =
                podcasts::store::commit_remove_subscription(&self.conn, subscription_id)
            {
                self.show_error(&error.to_string());
            }
            return;
        };
        let toast = adw::Toast::new(&strings::podcast_unsubscribe_from(&subscription.title));
        toast.set_button_label(Some(&strings::text(strings::PODCAST_UNDO)));
        toast.set_timeout(10);
        toast.set_priority(adw::ToastPriority::High);
        let undone = Rc::new(Cell::new(false));
        let weak = Rc::downgrade(self);
        let undo_flag = undone.clone();
        toast.connect_button_clicked(move |_| {
            undo_flag.set(true);
            if let Some(view) = weak.upgrade() {
                if let Err(error) =
                    podcasts::store::undo_remove_subscription(&view.conn, subscription_id)
                {
                    view.show_error(&error.to_string());
                }
                view.refresh();
                (view.callbacks.on_sidebar_refresh)();
            }
        });
        let weak = Rc::downgrade(self);
        toast.connect_dismissed(move |_| {
            if undone.get() {
                return;
            }
            let Some(view) = weak.upgrade() else {
                return;
            };
            view.kept_downloads
                .borrow_mut()
                .add(subscription_id, paths.clone());
            if let Err(error) =
                podcasts::store::commit_remove_subscription(&view.conn, subscription_id)
            {
                view.show_error(&error.to_string());
                return;
            }
            view.schedule_download_toast();
        });
        overlay.add_toast(toast);
    }

    fn remove_episode(self: &Rc<Self>, episode_id: i64) {
        let Ok(Some(episode)) = podcasts::store::episode(&self.conn, episode_id) else {
            return;
        };
        if let Err(error) = podcasts::store::tombstone_episode(
            &self.conn,
            episode_id,
            chrono::Utc::now().timestamp(),
        ) {
            self.show_error(&error.to_string());
            return;
        }
        self.refresh();
        (self.callbacks.on_sidebar_refresh)();

        let Some(overlay) = self.toast_overlay.upgrade() else {
            match podcasts::store::commit_remove_episode(&self.conn, episode_id) {
                Ok(Some(path)) => self
                    .kept_downloads
                    .borrow_mut()
                    .add(episode.subscription_id, vec![path]),
                Ok(None) => {}
                Err(error) => self.show_error(&error.to_string()),
            }
            self.schedule_download_toast();
            return;
        };

        let toast = adw::Toast::new(&strings::podcast_removed_episode(&episode.title));
        toast.set_button_label(Some(&strings::text(strings::PODCAST_UNDO)));
        toast.set_timeout(10);
        toast.set_priority(adw::ToastPriority::High);
        let undone = Rc::new(Cell::new(false));
        let weak = Rc::downgrade(self);
        let undo_flag = undone.clone();
        toast.connect_button_clicked(move |_| {
            undo_flag.set(true);
            if let Some(view) = weak.upgrade() {
                if let Err(error) = podcasts::store::undo_remove_episode(&view.conn, episode_id) {
                    view.show_error(&error.to_string());
                }
                view.refresh();
                (view.callbacks.on_sidebar_refresh)();
            }
        });
        let weak = Rc::downgrade(self);
        toast.connect_dismissed(move |_| {
            if undone.get() {
                return;
            }
            let Some(view) = weak.upgrade() else {
                return;
            };
            match podcasts::store::commit_remove_episode(&view.conn, episode_id) {
                Ok(Some(path)) => view
                    .kept_downloads
                    .borrow_mut()
                    .add(episode.subscription_id, vec![path]),
                Ok(None) => {}
                Err(error) => {
                    view.show_error(&error.to_string());
                    return;
                }
            }
            view.schedule_download_toast();
        });
        overlay.add_toast(toast);
    }

    fn schedule_download_toast(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        glib::idle_add_local_once(move || {
            if let Some(view) = weak.upgrade() {
                view.flush_download_toast();
            }
        });
    }

    fn flush_download_toast(&self) {
        let (shows, paths) = self.kept_downloads.borrow_mut().take();
        if paths.is_empty() {
            return;
        }
        let Some(overlay) = self.toast_overlay.upgrade() else {
            return;
        };
        let toast = adw::Toast::new(&strings::podcast_downloads_kept(shows, paths.len()));
        toast.set_button_label(Some(&strings::text(strings::PODCAST_DELETE_FILES)));
        toast.set_priority(adw::ToastPriority::High);
        toast.connect_button_clicked(move |_| {
            if download_commit_action(true) != DownloadCommitAction::Trash {
                return;
            }
            for path in &paths {
                if let Err(error) =
                    gio::File::for_path(path).trash(None::<&gio::Cancellable>)
                {
                    tracing::warn!(%error, path = %path.display(), "could not trash podcast download");
                }
            }
        });
        overlay.add_toast(toast);
    }

    pub(super) fn show_error(&self, message: &str) {
        if let Some(overlay) = self.toast_overlay.upgrade() {
            let toast = adw::Toast::new(message);
            toast.set_priority(adw::ToastPriority::High);
            overlay.add_toast(toast);
        } else {
            tracing::warn!(%message, "podcast action failed");
        }
    }
}
