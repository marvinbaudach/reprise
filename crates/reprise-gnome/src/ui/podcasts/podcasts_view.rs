//! Podcasts table, status states, actions, and refresh wiring.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use gtk4::gio;
use gtk4::glib::{self};
use gtk4::prelude::*;
use libadwaita as adw;
use reprise_core::podcasts::download_state::DownloadState;
use reprise_core::podcasts::{self, EpisodeRow, PodcastKind, SourceGroup};
use rusqlite::Connection;

use super::add_dialog;
use super::podcasts_context_menu;
use super::podcasts_device_sync::PodcastDeviceSyncState;
use super::podcasts_download_presentation::refreshed_download_states;
use super::podcasts_empty_state::{podcasts_empty_state_for, PodcastsEmptyState};
use super::podcasts_filter_bar::PodcastsFilterBar;
use super::podcasts_groups;
use super::podcasts_presentation::{
    active as filter_active, apply_filter, rendered_source_groups, sort_newest_first,
};
use super::podcasts_removal::{
    download_commit_action, download_request_allowed, download_toggle_action, DownloadCommitAction,
    DownloadToggleAction, KeptDownloads,
};
use super::podcasts_scroller::build_episode_scroller;
use super::podcasts_view_data::{last_updated_text, unique};
use super::podcasts_worker::{
    podcasts_response_channel, request_generation, PodcastsOperation, PodcastsRequest,
    PodcastsRuntime, PodcastsWorkerResult,
};
use super::youtube_channel_detail::YoutubeChannelDetail;
use crate::ui::sidebar::sidebar_presentation::NavIcon;
use crate::ui::source_empty_state::{SourceEmptyState, SourceEmptyStateCopy};
use crate::ui::strings;

#[path = "podcasts_view_requests.rs"]
mod requests;

/// `SRC-10`: the stack page holding the shared empty-state geometry, used
/// only for "nothing subscribed yet" — distinct from the existing `"status"`
/// page, which still carries `NoEpisodes`/`NoResults` (Block B2, unchanged).
const EMPTY_PAGE: &str = "empty";

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
    pub(super) conn: Rc<RefCell<Connection>>,
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
}

impl PodcastsView {
    pub(in crate::ui) fn install(
        conn: Rc<RefCell<Connection>>,
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
        let list_box = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        list_box.append(filter_bar.widget());
        list_box.append(&build_episode_scroller(
            group_container.upcast_ref::<gtk4::Widget>(),
        ));

        let status = adw::StatusPage::new();
        let status_button = gtk4::Button::new();
        status_button.add_css_class("suggested-action");
        status.set_child(Some(&status_button));
        let empty_state = SourceEmptyState::new(&empty_state_copy(kind));
        let stack = gtk4::Stack::new();
        stack.add_named(&list_box, Some("list"));
        stack.add_named(&status, Some("status"));
        stack.add_named(empty_state.widget(), Some(EMPTY_PAGE));
        let default_hide_shorts = podcasts::config::load(&conn.borrow())
            .map_or(true, |config| config.youtube_hide_shorts_default);
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
        });
        view.install_actions();
        view.wire_controls(&refresh);
        let weak = Rc::downgrade(&view);
        view.empty_state.connect_add(move || {
            if let Some(view) = weak.upgrade() {
                view.open_add_dialog();
            }
        });
        view.refresh();
        view
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

    pub(in crate::ui) fn bind_device_sync(
        self: &Rc<Self>,
        runtime: &Rc<crate::ui::device_sync_runtime::DeviceSyncRuntime>,
    ) {
        super::podcasts_device_sync::bind(self, runtime);
    }

    pub(in crate::ui) fn refresh(&self) {
        let result = {
            let conn = self.conn.borrow();
            podcasts::query::list_source_groups(&conn, self.kind).and_then(|groups| {
                let selected = PodcastDeviceSyncState::selected_for_groups(&conn, &groups)?;
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
                let last_updated = {
                    let conn = self.conn.borrow();
                    last_updated_text(&conn)
                };
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

    fn install_actions(self: &Rc<Self>) {
        let group = gio::SimpleActionGroup::new();
        self.add_target_action(&group, podcasts_context_menu::ACTION_PLAY, |view, id| {
            if let Ok(Some(row)) = podcasts::store::episode(&view.conn.borrow(), id) {
                (view.callbacks.on_episode_activated)(row);
            }
        });
        self.add_target_action(
            &group,
            podcasts_context_menu::ACTION_COPY_URL,
            |view, id| {
                if let Ok(Some(row)) = podcasts::store::episode(&view.conn.borrow(), id) {
                    view.root.clipboard().set_text(&row.audio_url);
                }
            },
        );
        self.add_target_action(
            &group,
            podcasts_context_menu::ACTION_TOGGLE_PLAYED,
            |view, id| {
                if let Ok(Some(row)) = podcasts::store::episode(&view.conn.borrow(), id) {
                    let result = if row.played_at.is_some() {
                        podcasts::store::mark_unplayed(&view.conn.borrow(), id)
                    } else {
                        podcasts::store::mark_played(
                            &view.conn.borrow(),
                            id,
                            chrono::Utc::now().timestamp(),
                        )
                    };
                    if let Err(error) = result {
                        tracing::warn!(%error, "could not update podcast episode status");
                    }
                    view.refresh();
                    (view.callbacks.on_sidebar_refresh)();
                }
            },
        );
        self.add_target_action(
            &group,
            podcasts_context_menu::ACTION_TOGGLE_DOWNLOAD,
            PodcastsView::toggle_download,
        );
        self.add_target_action(
            &group,
            podcasts_context_menu::ACTION_REMOVE_EPISODE,
            PodcastsView::remove_episode,
        );
        self.add_target_action(
            &group,
            podcasts_context_menu::ACTION_UNSUBSCRIBE,
            PodcastsView::unsubscribe,
        );
        super::podcasts_device_sync::install_action(self, &group);
        let load_more =
            gio::SimpleAction::new("load-more", Some(&<(i64, u32)>::static_variant_type()));
        let weak = Rc::downgrade(self);
        load_more.connect_activate(move |_, target| {
            let Some(view) = weak.upgrade() else { return };
            let Some((subscription_id, end)) =
                target.and_then(gtk4::glib::Variant::get::<(i64, u32)>)
            else {
                return;
            };
            view.request_load_more(subscription_id, end as usize);
        });
        group.add_action(&load_more);
        self.youtube_detail.install_actions(&group);
        let add = gio::SimpleAction::new("open-add", None);
        let weak = Rc::downgrade(self);
        add.connect_activate(move |_, _| {
            if let Some(view) = weak.upgrade() {
                view.open_add_dialog();
            }
        });
        group.add_action(&add);
        self.root.insert_action_group("podcasts", Some(&group));
    }

    fn add_target_action(
        self: &Rc<Self>,
        group: &gio::SimpleActionGroup,
        name: &str,
        callback: impl Fn(&Rc<Self>, i64) + 'static,
    ) {
        let action = gio::SimpleAction::new(name, Some(&i64::static_variant_type()));
        let weak = Rc::downgrade(self);
        action.connect_activate(move |_, target| {
            let Some(view) = weak.upgrade() else {
                return;
            };
            let Some(id) = target.and_then(glib::Variant::get::<i64>) else {
                return;
            };
            callback(&view, id);
        });
        group.add_action(&action);
    }

    fn open_add_dialog(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        add_dialog::present(&self.root, &self.conn, self.kind, move |import_latest| {
            if let Some(view) = weak.upgrade() {
                view.refresh();
                if import_latest {
                    view.request_refresh(true);
                }
                (view.callbacks.on_sidebar_refresh)();
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
        self.youtube_detail
            .update(&rendered_groups, &download_states);
        let download_widgets = podcasts_groups::replace(
            &self.group_container,
            &rendered_groups,
            self.playing_episode.get(),
            &self.expanded_sources,
            &download_states,
            &connected_devices,
            &selected_devices,
        );
        self.download_widgets.replace(download_widgets);
        self.filter_bar
            .set_context(unique(shows), filtered.len(), total);
        let subscriptions = groups.len();
        let classification =
            podcasts_empty_state_for(subscriptions, total, filtered.len(), filter_active(&filter));
        // `SRC-10`: the true "nothing subscribed yet" empty state hides the
        // footer's refresh row too — refreshing zero subscriptions has
        // nothing to do, and a live "Refresh now" control would make an
        // intentionally unused view look broken instead.
        self.footer
            .set_visible(classification != PodcastsEmptyState::Empty);
        match classification {
            PodcastsEmptyState::List => self.stack.set_visible_child_name("list"),
            PodcastsEmptyState::Empty => self.stack.set_visible_child_name(EMPTY_PAGE),
            state => {
                let (title, description, button) = match state {
                    PodcastsEmptyState::NoEpisodes => (
                        strings::PODCAST_NO_EPISODES,
                        strings::PODCAST_NO_EPISODES_DESCRIPTION,
                        strings::text(strings::PODCAST_REFRESH_NOW),
                    ),
                    PodcastsEmptyState::NoResults => (
                        strings::PODCAST_NO_EPISODES,
                        strings::PODCAST_NO_EPISODES_DESCRIPTION,
                        strings::podcast_show_all_count(total),
                    ),
                    PodcastsEmptyState::List | PodcastsEmptyState::Empty => unreachable!(),
                };
                self.status.set_title(&strings::text(title));
                self.status
                    .set_description(Some(&strings::text(description)));
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
        let Ok(Some(row)) = podcasts::store::episode(&self.conn.borrow(), episode_id) else {
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
            if let Err(error) =
                podcasts::store::set_downloaded_path(&self.conn.borrow(), episode_id, None)
            {
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
        let Ok(Some(subscription)) =
            podcasts::store::subscription(&self.conn.borrow(), subscription_id)
        else {
            return;
        };
        let paths = podcasts::store::downloaded_paths_for_subscription(
            &self.conn.borrow(),
            subscription_id,
        )
        .unwrap_or_default();
        if let Err(error) = podcasts::store::tombstone_subscription(
            &self.conn.borrow(),
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
                podcasts::store::commit_remove_subscription(&self.conn.borrow(), subscription_id)
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
                    podcasts::store::undo_remove_subscription(&view.conn.borrow(), subscription_id)
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
                podcasts::store::commit_remove_subscription(&view.conn.borrow(), subscription_id)
            {
                view.show_error(&error.to_string());
                return;
            }
            view.schedule_download_toast();
        });
        overlay.add_toast(toast);
    }

    fn remove_episode(self: &Rc<Self>, episode_id: i64) {
        let Ok(Some(episode)) = podcasts::store::episode(&self.conn.borrow(), episode_id) else {
            return;
        };
        if let Err(error) = podcasts::store::tombstone_episode(
            &self.conn.borrow(),
            episode_id,
            chrono::Utc::now().timestamp(),
        ) {
            self.show_error(&error.to_string());
            return;
        }
        self.refresh();
        (self.callbacks.on_sidebar_refresh)();

        let Some(overlay) = self.toast_overlay.upgrade() else {
            match podcasts::store::commit_remove_episode(&self.conn.borrow(), episode_id) {
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
                if let Err(error) =
                    podcasts::store::undo_remove_episode(&view.conn.borrow(), episode_id)
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
            match podcasts::store::commit_remove_episode(&view.conn.borrow(), episode_id) {
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

/// `SRC-10`: the shared empty-state geometry's copy for this source's kind —
/// Podcasts (RSS) or YouTube. The glyph matches the source's own sidebar
/// entry (`NavIcon`), so the page visually continues from where the user
/// navigated.
fn empty_state_copy(kind: PodcastKind) -> SourceEmptyStateCopy {
    let (icon, title, body, button, secondary) = match kind {
        PodcastKind::Rss => (
            NavIcon::Podcasts.icon_name(),
            strings::PODCAST_NO_PODCASTS,
            strings::PODCAST_NO_PODCASTS_DESCRIPTION,
            strings::PODCAST_ADD,
            strings::PODCAST_NO_PODCASTS_SECONDARY,
        ),
        PodcastKind::Youtube => (
            NavIcon::Youtube.icon_name(),
            strings::YOUTUBE_NO_CHANNELS,
            strings::YOUTUBE_NO_CHANNELS_DESCRIPTION,
            strings::YOUTUBE_NO_CHANNELS_ADD,
            strings::YOUTUBE_NO_CHANNELS_SECONDARY,
        ),
    };
    SourceEmptyStateCopy {
        icon_name: icon,
        title: strings::text(title),
        body: strings::text(body),
        button_label: strings::text(button),
        secondary_line: Some(strings::text(secondary)),
    }
}
