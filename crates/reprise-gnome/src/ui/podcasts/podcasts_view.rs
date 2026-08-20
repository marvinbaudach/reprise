//! Podcasts table, status states, actions, and refresh wiring.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use gtk4::gio;
use gtk4::glib::{self};
use gtk4::prelude::*;
use libadwaita as adw;
use reprise_core::connectivity::{self, ActionOutcome, Connectivity};
use reprise_core::db::Db;
use reprise_core::podcasts::download_state::DownloadState;
use reprise_core::podcasts::{self, EpisodeRow, PodcastKind, SourceGroup};
use reprise_core::source_error::SourceError;
use reprise_core::up_next::QueueItem;

use super::add_dialog;
use super::podcasts_callbacks::PodcastsCallbacks;
use super::podcasts_context_menu;
use super::podcasts_deferred_actions::{replay_until_refused, DeferredAction, DeferredActions};
use super::podcasts_download_presentation::refreshed_download_states;
use super::podcasts_empty_state::{podcasts_empty_state_for, PodcastsEmptyState};
use super::podcasts_filter_bar::PodcastsFilterBar;
use super::podcasts_footer::PodcastsFooter;
use super::podcasts_groups;
use super::podcasts_playback::EpisodeMark;
use super::podcasts_presentation::{
    active as filter_active, apply_filter, filter_without_hiding, filter_without_hiding_group,
    library_summary, rendered_source_groups, sort_newest_first,
};
use super::podcasts_removal::{
    download_commit_action, download_request_allowed, download_toggle_action, DownloadCommitAction,
    DownloadToggleAction, KeptDownloads,
};
use super::podcasts_rendered_order;
use super::podcasts_reveal::RevealRequest;
use super::podcasts_selection::{PodcastSelection, SelectMode};
use super::podcasts_view_data::{episode_ids_in_rendered_order, last_updated_text};
use super::podcasts_worker::{
    podcasts_response_channel, request_generation, PodcastsOperation, PodcastsRequest,
    PodcastsRuntime, PodcastsWorkerResult,
};
use super::youtube_channel_detail::YoutubeChannelDetail;
use crate::ui::source_empty_state::{SourceEmptyState, SourceFailureState};
use crate::ui::source_error_banner::SourceErrorBanner;
use crate::ui::strings;

#[path = "podcasts_view_actions.rs"]
mod actions;
#[path = "podcasts_artwork_refresh.rs"]
mod artwork_refresh;
#[cfg(test)]
#[path = "podcasts_artwork_refresh_tests.rs"]
mod artwork_refresh_tests;
#[path = "podcasts_connectivity_ui.rs"]
mod connectivity_ui;
#[path = "podcasts_view_copy.rs"]
mod copy;
#[cfg(test)]
#[path = "podcasts_end_of_results_tests.rs"]
mod end_of_results_tests;
#[path = "podcasts_failure_ui.rs"]
mod failure_ui;
#[path = "podcasts_view_marker.rs"]
mod marker;
#[cfg(test)]
#[path = "podcasts_refresh_button_tests.rs"]
mod refresh_button_tests;
#[path = "podcasts_view_requests.rs"]
mod requests;
#[path = "podcasts_view_selection.rs"]
mod selection;
#[path = "podcasts_view_shortcuts.rs"]
mod shortcuts;
#[cfg(test)]
#[path = "podcasts_view_tests.rs"]
mod tests;

/// `SRC-10`: the stack page holding the shared empty-state geometry, used
/// only for "nothing subscribed yet".
const EMPTY_PAGE: &str = "empty";
/// `SRC-10` addendum (Block B2): the module-off sibling of `EMPTY_PAGE` —
/// same geometry, "Enable in Preferences" instead of Add.
const MODULE_OFF_PAGE: &str = "module-off";
const FAILURE_PAGE: &str = "fetch-failed";

pub(in crate::ui) struct PodcastsView {
    root: gtk4::Box,
    pub(super) conn: Rc<Db>,
    runtime: Rc<PodcastsRuntime>,
    callbacks: PodcastsCallbacks,
    kind: PodcastKind,
    filter_bar: Rc<PodcastsFilterBar>,
    end_of_results: Rc<crate::ui::end_of_results::EndOfResults>,
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
    error_banner: SourceErrorBanner,
    failure_state: SourceFailureState,
    fetch_failure: RefCell<Option<SourceError>>,
    /// Set post-construction (parallel to `set_toast_overlay`) once
    /// Preferences exists — `PodcastsView` is built before it in `window.rs`.
    on_open_preferences: RefCell<Option<Rc<dyn Fn()>>>,
    on_open_youtube_preferences: RefCell<Option<Rc<dyn Fn()>>>,
    footer: gtk4::Box,
    footer_add: gtk4::Button,
    footer_status: gtk4::Label,
    footer_spinner: gtk4::Spinner,
    refresh_button: gtk4::Button,
    refresh_stack: gtk4::Stack,
    refresh_spinner: gtk4::Spinner,
    /// Number of refresh requests currently running for this view. This is a
    /// counter rather than a boolean because scheduler, button, and tab-open
    /// requests can overlap; the oldest completion must not release the
    /// button while a newer request is still fetching.
    refresh_in_flight: Cell<usize>,
    fill_in_flight: Cell<bool>,
    groups: RefCell<Vec<SourceGroup>>,
    rows: RefCell<Vec<EpisodeRow>>,
    expanded_sources: Rc<RefCell<BTreeSet<i64>>>,
    expanded_episode_sources: Rc<RefCell<BTreeSet<i64>>>,
    selection: Rc<RefCell<PodcastSelection>>,
    download_states: Rc<RefCell<BTreeMap<i64, DownloadState>>>,
    download_widgets: RefCell<BTreeMap<i64, podcasts_groups::DownloadRowWidgets>>,
    selection_widgets: RefCell<BTreeMap<i64, podcasts_groups::SelectionRowWidgets>>,
    channel_widgets: RefCell<BTreeMap<i64, podcasts_groups::ChannelRowWidgets>>,
    artwork_rebinds: RefCell<Vec<podcasts_groups::ArtworkRebind>>,
    scroller: gtk4::ScrolledWindow,
    last_scroll_activity: Cell<Option<std::time::Instant>>,
    reveal_animation: Rc<RefCell<Option<adw::TimedAnimation>>>,
    activating_here: Cell<bool>,
    playing_episode: Cell<Option<EpisodeMark>>,
    pending_reveal: RefCell<Option<RevealRequest>>,
    unavailable_episode: Cell<Option<i64>>,
    generation: Cell<u64>,
    toast_overlay: glib::WeakRef<adw::ToastOverlay>,
    kept_downloads: RefCell<KeptDownloads>,
    /// `NET-3c`: explicit connectivity seam, mirroring `RadioView`'s
    /// (`NET-3b`). The window projects `gio::NetworkMonitor` changes into
    /// this value through [`PodcastsView::set_connectivity`]. A transition
    /// from `Offline` to `Online` triggers the queued-download runner.
    connectivity: Cell<Connectivity>,
    deferred_actions: RefCell<DeferredActions>,
}

impl PodcastsView {
    pub(in crate::ui) fn install(
        conn: Rc<Db>,
        runtime: Rc<PodcastsRuntime>,
        callbacks: PodcastsCallbacks,
        kind: PodcastKind,
    ) -> Rc<Self> {
        let filter_bar = PodcastsFilterBar::new(conn.clone(), kind);
        let (group_container, scroller, list_overlay, end_of_results) =
            super::podcasts_list_surface::build(kind, &filter_bar);

        let status = adw::StatusPage::new();
        let status_button = gtk4::Button::new();
        status_button.add_css_class("suggested-action");
        status.set_child(Some(&status_button));
        let empty_state = SourceEmptyState::new(&copy::empty_state_copy(kind));
        let module_off_state = SourceEmptyState::new(&copy::module_off_copy(kind));
        let error_banner = SourceErrorBanner::new();
        let failure_state = SourceFailureState::new(copy::empty_state_copy(kind).icon_name);
        let stack = gtk4::Stack::new();
        stack.add_named(&list_overlay, Some("list"));
        stack.add_named(&status, Some("status"));
        stack.add_named(empty_state.widget(), Some(EMPTY_PAGE));
        stack.add_named(module_off_state.widget(), Some(MODULE_OFF_PAGE));
        stack.add_named(failure_state.widget(), Some(FAILURE_PAGE));
        let default_hide_shorts =
            podcasts::config::load(&conn).map_or(true, |config| config.youtube_hide_shorts_default);
        let youtube_detail = YoutubeChannelDetail::new(&stack, default_hide_shorts);
        stack.add_named(youtube_detail.widget(), Some("youtube-channel"));
        stack.set_vexpand(true);

        let PodcastsFooter {
            root: footer,
            add: footer_add,
            status: footer_status,
            spinner: footer_spinner,
            refresh_button,
            refresh_stack,
            refresh_spinner,
        } = super::podcasts_footer::build(kind);

        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        root.add_css_class("reprise-podcasts-source");
        // `SRC-10` addendum (Block B2): the filter row lives at this level,
        // not inside the "list" stack page, so its visibility can be
        // decided independently of which page is showing — visible for
        // `List`/`NoEpisodes`/`NoResults`/`NoDownloads`, hidden for the two
        // whole-page-replaced states `Empty`/`ModuleOff`.
        root.append(filter_bar.widget());
        root.append(error_banner.widget());
        root.append(&stack);
        root.append(&footer);

        let view = Rc::new(Self {
            root,
            conn,
            runtime,
            callbacks,
            kind,
            filter_bar,
            end_of_results,
            group_container,
            stack,
            youtube_detail,
            status,
            status_button,
            empty_state,
            module_off_state,
            error_banner,
            failure_state,
            fetch_failure: RefCell::new(None),
            on_open_preferences: RefCell::new(None),
            on_open_youtube_preferences: RefCell::new(None),
            footer,
            footer_add,
            footer_status,
            footer_spinner,
            refresh_button,
            refresh_stack,
            refresh_spinner,
            refresh_in_flight: Cell::new(0),
            fill_in_flight: Cell::new(false),
            groups: RefCell::new(Vec::new()),
            rows: RefCell::new(Vec::new()),
            expanded_sources: Rc::new(RefCell::new(BTreeSet::new())),
            expanded_episode_sources: Rc::new(RefCell::new(BTreeSet::new())),
            selection: Rc::new(RefCell::new(PodcastSelection::default())),
            download_states: Rc::new(RefCell::new(BTreeMap::new())),
            download_widgets: RefCell::new(BTreeMap::new()),
            selection_widgets: RefCell::new(BTreeMap::new()),
            channel_widgets: RefCell::new(BTreeMap::new()),
            artwork_rebinds: RefCell::new(Vec::new()),
            scroller,
            last_scroll_activity: Cell::new(None),
            reveal_animation: Rc::new(RefCell::new(None)),
            activating_here: Cell::new(false),
            playing_episode: Cell::new(None),
            pending_reveal: RefCell::new(None),
            unavailable_episode: Cell::new(None),
            generation: Cell::new(0),
            toast_overlay: glib::WeakRef::new(),
            kept_downloads: RefCell::new(KeptDownloads::default()),
            connectivity: Cell::new(Connectivity::default()),
            deferred_actions: RefCell::new(DeferredActions::default()),
        });
        view.install_actions();
        view.wire_controls();
        view.install_selection_shortcuts();
        view.install_reveal_tracking();
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

    /// SEARCH-8a: applies this view's query. The shell calls it as the
    /// header entry changes and once more when the section becomes visible
    /// again, so a query typed here is exactly what is applied here.
    pub(in crate::ui) fn set_search_query(&self, query: &str) {
        self.filter_bar.set_query(query);
    }

    pub(in crate::ui) fn set_committed_search_query(&self, query: &str) {
        self.filter_bar.set_committed_query(query);
    }

    /// SEARCH-8a: the reverse direction — the bar requests its × transition or
    /// reports a local jump that had to relax search.
    pub(in crate::ui) fn set_on_search_query_changed(&self, callback: impl Fn(&str) + 'static) {
        self.filter_bar.set_on_query_changed(callback);
    }

    /// FIL-2a: "Clear all" for this section — its query and its facets.
    pub(in crate::ui) fn clear_all_filters(&self) {
        self.filter_bar.clear_all();
    }

    /// Wires the module-off empty state's "Enable in Preferences" button.
    /// Set post-construction because `PodcastsView` is built before the
    /// `Preferences` context exists in `window.rs` (mirrors
    /// `set_toast_overlay`).
    pub(in crate::ui) fn set_on_open_preferences(&self, callback: impl Fn() + 'static) {
        *self.on_open_preferences.borrow_mut() = Some(Rc::new(callback));
    }

    pub(in crate::ui) fn set_on_open_youtube_preferences(&self, callback: impl Fn() + 'static) {
        *self.on_open_youtube_preferences.borrow_mut() = Some(Rc::new(callback));
    }

    pub(in crate::ui) fn root(&self) -> &gtk4::Widget {
        self.root.upcast_ref()
    }

    pub(in crate::ui) fn set_toast_overlay(&self, overlay: &adw::ToastOverlay) {
        self.toast_overlay.set(Some(overlay));
    }

    pub(in crate::ui) fn refresh(&self) {
        let result = podcasts::query::list_source_groups(&self.conn, self.kind);
        match result {
            Ok(groups) => {
                let mut rows = groups
                    .iter()
                    .flat_map(|group| group.episodes.iter().cloned())
                    .collect::<Vec<_>>();
                sort_newest_first(&mut rows);
                self.selection
                    .borrow_mut()
                    .retain_available(rows.iter().map(|row| row.id));
                let previous = self.download_states.borrow().clone();
                self.download_states
                    .replace(refreshed_download_states(&rows, &previous));
                self.groups.replace(groups);
                self.rows.replace(rows);
                let last_updated = last_updated_text(&self.conn);
                self.footer_status.set_text(&last_updated);
                self.render();
            }
            // `POD-17`: the detail belongs in the log, not in the footer.
            // `DbError`'s `Display` carries rusqlite's whole failing statement
            // and a byte offset, which is neither readable nor actionable.
            Err(error) => {
                tracing::warn!(%error, "could not read podcast subscriptions");
                self.footer_status
                    .set_text(&strings::text(strings::PODCAST_LIBRARY_UNREADABLE));
            }
        }
    }

    fn wire_controls(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        self.refresh_button.connect_clicked(move |_| {
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
        let selected_ids = self.selection.borrow().selected_ids();
        let filter = self.filter_bar.filter();
        let filtered = apply_filter(&rows, &filter);
        let total = rows.len();
        super::podcasts_list_surface::update(&self.end_of_results, &filter, filtered.len(), total);
        let rendered_groups = rendered_source_groups(&groups, &filter, &download_states);
        // `NET-1a` / `C1`: computed once per render pass from the live
        // module + global-gate state, then threaded down to every source
        // image entry point in this view instead of each one re-deriving it.
        let images_allowed = reprise_core::online_sources::network_allowed(
            &self.conn,
            &reprise_core::modules::ARTWORK_MODULE,
        )
        .unwrap_or(false);
        self.youtube_detail.update(
            &rendered_groups,
            &download_states,
            images_allowed,
            self.connectivity.get(),
            self.unavailable_episode.get(),
            self.playing_episode.get(),
        );
        let rendered_widgets = podcasts_groups::replace(
            &self.group_container,
            &rendered_groups,
            self.playing_episode.get(),
            &self.expanded_sources,
            &self.expanded_episode_sources,
            &download_states,
            images_allowed,
            self.connectivity.get(),
            self.unavailable_episode.get(),
            &self.selection,
            &filter.query,
        );
        self.download_widgets.replace(rendered_widgets.downloads);
        self.selection_widgets.replace(rendered_widgets.selection);
        self.channel_widgets.replace(rendered_widgets.channels);
        self.artwork_rebinds.replace(rendered_widgets.artwork);
        // `G2` (design 6a): the header line is a projection over the
        // unfiltered `groups`, not `rendered_groups` — it stays a stable
        // library overview instead of jittering with the active filter.
        self.filter_bar
            .set_context(filtered.len(), library_summary(&groups), selected_ids.len());
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
            self.fetch_failure.borrow().is_some(),
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
            PodcastsEmptyState::Empty
                | PodcastsEmptyState::ModuleOff
                | PodcastsEmptyState::FetchFailed
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
            PodcastsEmptyState::FetchFailed => {
                self.stack.set_visible_child_name(FAILURE_PAGE);
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
        if connectivity::deferrable_action_outcome(
            self.connectivity.get(),
            DownloadState::NotDownloaded.local_availability(),
        ) == ActionOutcome::QueuedOffline
        {
            self.deferred_actions
                .borrow_mut()
                .push(DeferredAction::Download(episode_id));
            self.set_download_state(episode_id, &DownloadState::Queued);
            self.footer_status
                .set_text(&strings::text(strings::PODCAST_QUEUED_OFFLINE));
            return;
        }
        self.dispatch_download(episode_id);
    }

    fn dispatch_download(self: &Rc<Self>, episode_id: i64) -> bool {
        let operation = PodcastsOperation::Download { episode_id };
        let generation = request_generation(self.generation.get(), operation);
        let (response, receiver) = podcasts_response_channel();
        if !self.runtime.request(PodcastsRequest {
            generation,
            operation,
            response,
        }) {
            return false;
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
                    Ok(PodcastsWorkerResult::Filled(_)) => {}
                    Err(error) => {
                        tracing::warn!(%error, episode_id, "podcast download failed");
                        view.set_download_state(
                            episode_id,
                            &DownloadState::Failed {
                                message: strings::text(strings::PODCAST_DOWNLOAD_FAILED),
                            },
                        );
                        view.show_error(&strings::text(strings::PODCAST_DOWNLOAD_FAILED));
                        break;
                    }
                }
            }
        });
        true
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
}
