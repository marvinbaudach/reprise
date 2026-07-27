//! Podcasts table, status states, actions, and refresh wiring.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::rc::Rc;

use gtk4::gio;
use gtk4::glib::{self};
use gtk4::prelude::*;
use libadwaita as adw;
use reprise_core::podcasts::{self, EpisodeRow};
use rusqlite::Connection;

use super::add_dialog;
use super::podcasts_columns::{self, IsPlaying, OnUnsubscribe};
use super::podcasts_context_menu;
use super::podcasts_empty_state::{podcasts_empty_state_for, PodcastsEmptyState};
use super::podcasts_filter_bar::PodcastsFilterBar;
use super::podcasts_model::{PodcastEpisodeObject, PodcastsModel};
use super::podcasts_presentation::{active as filter_active, apply_filter, sort_newest_first};
use super::podcasts_worker::{
    PodcastsOperation, PodcastsRequest, PodcastsResponse, PodcastsRuntime,
};
use crate::ui::strings;

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

#[derive(Default)]
struct KeptDownloads {
    shows: BTreeMap<i64, Vec<PathBuf>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DownloadCommitAction {
    Keep,
    Trash,
}

fn download_commit_action(delete_requested: bool) -> DownloadCommitAction {
    if delete_requested {
        DownloadCommitAction::Trash
    } else {
        DownloadCommitAction::Keep
    }
}

impl KeptDownloads {
    fn add(&mut self, subscription_id: i64, paths: Vec<String>) {
        if paths.is_empty() {
            return;
        }
        self.shows
            .entry(subscription_id)
            .or_default()
            .extend(paths.into_iter().map(PathBuf::from));
    }

    fn take(&mut self) -> (usize, Vec<PathBuf>) {
        let shows = self.shows.len();
        let paths = std::mem::take(&mut self.shows)
            .into_values()
            .flatten()
            .collect();
        (shows, paths)
    }
}

pub(in crate::ui) struct PodcastsView {
    root: gtk4::Box,
    conn: Rc<RefCell<Connection>>,
    runtime: Rc<PodcastsRuntime>,
    callbacks: PodcastsCallbacks,
    model: PodcastsModel,
    filter_bar: Rc<PodcastsFilterBar>,
    stack: gtk4::Stack,
    status: adw::StatusPage,
    status_button: gtk4::Button,
    footer_status: gtk4::Label,
    footer_spinner: gtk4::Spinner,
    rows: RefCell<Vec<EpisodeRow>>,
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
    ) -> Rc<Self> {
        let model = PodcastsModel::new();
        let column_view = gtk4::ColumnView::new(Some(model.selection().clone()));
        column_view.add_css_class("reprise-podcasts-table");
        column_view.set_hexpand(true);
        column_view.set_vexpand(true);

        let filter_bar = PodcastsFilterBar::new(conn.clone());
        let list_box = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        list_box.append(filter_bar.widget());
        list_box.append(&column_view);

        let status = adw::StatusPage::new();
        let status_button = gtk4::Button::new();
        status_button.add_css_class("suggested-action");
        status.set_child(Some(&status_button));
        let stack = gtk4::Stack::new();
        stack.add_named(&list_box, Some("list"));
        stack.add_named(&status, Some("status"));
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
            model,
            filter_bar,
            stack,
            status,
            status_button,
            footer_status,
            footer_spinner,
            rows: RefCell::new(Vec::new()),
            playing_episode: Cell::new(None),
            generation: Cell::new(0),
            toast_overlay: glib::WeakRef::new(),
            kept_downloads: RefCell::new(KeptDownloads::default()),
        });
        view.install_actions();
        view.wire_columns(&column_view);
        view.wire_controls(&column_view, &refresh);
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

    pub(in crate::ui) fn refresh(&self) {
        match podcasts::query::list_episodes(&self.conn.borrow()) {
            Ok(mut rows) => {
                sort_newest_first(&mut rows);
                self.rows.replace(rows);
                self.footer_status
                    .set_text(&last_updated_text(&self.conn.borrow()));
                self.render();
            }
            Err(error) => self.footer_status.set_text(&error.to_string()),
        }
    }

    pub(in crate::ui) fn request_refresh(self: &Rc<Self>, force: bool) -> bool {
        let generation = self.generation.get().wrapping_add(1);
        self.generation.set(generation);
        let (sender, receiver) = async_channel::bounded::<PodcastsResponse>(1);
        let queued = self.runtime.request(PodcastsRequest {
            generation,
            operation: PodcastsOperation::Refresh { force },
            response: sender,
        });
        if !queued {
            return false;
        }
        self.footer_spinner.start();
        self.footer_status
            .set_text(&strings::text(strings::PODCAST_REFRESHING));
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let Ok(response) = receiver.recv().await else {
                return;
            };
            let Some(view) = weak.upgrade() else {
                return;
            };
            if view.generation.get() != response.generation {
                return;
            }
            view.footer_spinner.stop();
            match response.result {
                Ok(_) => {
                    view.refresh();
                    (view.callbacks.on_sidebar_refresh)();
                }
                Err(error) => view.footer_status.set_text(&format!(
                    "{} · {error}",
                    strings::text(strings::PODCAST_REFRESH_FAILED)
                )),
            }
        });
        true
    }

    fn wire_columns(self: &Rc<Self>, column_view: &gtk4::ColumnView) {
        let weak = Rc::downgrade(self);
        let unsubscribe: OnUnsubscribe = Rc::new(move |subscription_id| {
            if let Some(view) = weak.upgrade() {
                view.unsubscribe(subscription_id);
            }
        });
        let weak = Rc::downgrade(self);
        let playing: IsPlaying = Rc::new(move |episode_id| {
            weak.upgrade()
                .is_some_and(|view| view.playing_episode.get() == Some(episode_id))
        });
        let columns = podcasts_columns::append_columns(column_view, &unsubscribe, &playing);
        columns.date.set_resizable(false);
        self.model.enable_sorting(column_view.sorter());
    }

    fn wire_controls(
        self: &Rc<Self>,
        column_view: &gtk4::ColumnView,
        refresh_button: &gtk4::Button,
    ) {
        let weak = Rc::downgrade(self);
        column_view.connect_activate(move |view, position| {
            let Some(podcasts) = weak.upgrade() else {
                return;
            };
            let Some(model) = view.model() else {
                return;
            };
            let Some(row) = model.item(position).and_downcast::<PodcastEpisodeObject>() else {
                return;
            };
            (podcasts.callbacks.on_episode_activated)(row.row());
        });
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
            let subscriptions =
                podcasts::store::count_subscriptions(&view.conn.borrow()).unwrap_or_default();
            if subscriptions == 0 {
                view.open_add_dialog();
            } else if filter_active(&view.filter_bar.filter()) {
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
        add_dialog::present(&self.root, &self.conn, move |import_latest| {
            if let Some(view) = weak.upgrade() {
                view.refresh();
                if import_latest {
                    view.request_refresh(true);
                }
                (view.callbacks.on_sidebar_refresh)();
            }
        });
    }

    fn render(&self) {
        let rows = self.rows.borrow();
        let filter = self.filter_bar.filter();
        let filtered = apply_filter(&rows, &filter);
        let total = rows.len();
        let shows = rows.iter().map(|row| row.show.clone()).collect::<Vec<_>>();
        self.model.replace(filtered.clone());
        self.filter_bar
            .set_context(unique(shows), filtered.len(), total);
        let subscriptions =
            podcasts::store::count_subscriptions(&self.conn.borrow()).unwrap_or_default();
        match podcasts_empty_state_for(subscriptions, total, filtered.len(), filter_active(&filter))
        {
            PodcastsEmptyState::List => self.stack.set_visible_child_name("list"),
            state => {
                let (title, description, button) = match state {
                    PodcastsEmptyState::Empty => (
                        strings::PODCAST_NO_PODCASTS,
                        strings::PODCAST_NO_PODCASTS_DESCRIPTION,
                        strings::text(strings::PODCAST_ADD),
                    ),
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
                    PodcastsEmptyState::List => unreachable!(),
                };
                self.status.set_title(&strings::text(title));
                self.status
                    .set_description(Some(&strings::text(description)));
                self.status_button.set_label(&button);
                self.stack.set_visible_child_name("status");
            }
        }
    }

    fn toggle_download(self: &Rc<Self>, episode_id: i64) {
        let Ok(Some(row)) = podcasts::store::episode(&self.conn.borrow(), episode_id) else {
            return;
        };
        if let Some(path) = row.downloaded_path {
            let file = gio::File::for_path(path);
            if let Err(error) = file.trash(None::<&gio::Cancellable>) {
                self.show_error(&error.to_string());
                return;
            }
            if let Err(error) =
                podcasts::store::set_downloaded_path(&self.conn.borrow(), episode_id, None)
            {
                self.show_error(&error.to_string());
            }
            self.refresh();
            return;
        }
        let generation = self.generation.get().wrapping_add(1);
        self.generation.set(generation);
        let (sender, receiver) = async_channel::bounded(1);
        if !self.runtime.request(PodcastsRequest {
            generation,
            operation: PodcastsOperation::Download { episode_id },
            response: sender,
        }) {
            return;
        }
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let Ok(response) = receiver.recv().await else {
                return;
            };
            let Some(view) = weak.upgrade() else {
                return;
            };
            match response.result {
                Ok(_) => view.refresh(),
                Err(error) => view.show_error(&error),
            }
        });
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

    fn show_error(&self, message: &str) {
        if let Some(overlay) = self.toast_overlay.upgrade() {
            let toast = adw::Toast::new(message);
            toast.set_priority(adw::ToastPriority::High);
            overlay.add_toast(toast);
        } else {
            tracing::warn!(%message, "podcast action failed");
        }
    }
}

fn unique(mut values: Vec<String>) -> Vec<String> {
    values.sort_by_key(|value| value.to_lowercase());
    values.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
    values
}

fn last_updated_text(conn: &Connection) -> String {
    let last = podcasts::store::active_subscriptions(conn)
        .ok()
        .and_then(|rows| rows.into_iter().filter_map(|row| row.last_fetch_at).max());
    super::podcasts_presentation::updated_ago(last, chrono::Utc::now().timestamp())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsubscribe_aggregation_skips_empty_download_sets_and_coalesces_shows() {
        let mut aggregate = KeptDownloads::default();
        aggregate.add(1, Vec::new());
        aggregate.add(2, vec!["a.mp3".into(), "b.mp3".into()]);
        aggregate.add(3, vec!["c.mp3".into()]);
        let (shows, paths) = aggregate.take();
        assert_eq!(shows, 2);
        assert_eq!(paths.len(), 3);
    }

    #[test]
    fn src_4_unsubscribe_commit_toast_trashes_never_hard_deletes() {
        assert_eq!(download_commit_action(false), DownloadCommitAction::Keep);
        assert_eq!(download_commit_action(true), DownloadCommitAction::Trash);
    }
}
