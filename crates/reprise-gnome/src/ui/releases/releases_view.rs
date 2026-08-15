//! Releases full-view table, filters, status surface, and refresh footer.

#![allow(dead_code)]

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use chrono::Local;
use gtk4::prelude::*;
use libadwaita as adw;
use reprise_core::artist_news::{self, ReleaseSortDirection};
use reprise_core::artist_news_history::HistoryEntry;
use reprise_core::connectivity::Connectivity;
use reprise_core::db::Db;
use reprise_core::source_error::{FailureAction, FailureSurface, SourceError, SourceErrorKind};

use super::releases_columns;
use super::releases_empty_state::{
    releases_empty_state_for, releases_scope_is_filtered, ReleasesEmptyState,
};
use super::releases_failure_ui::{
    failure_support, releases_failure_presentation, row_is_dimmed, update_failure_for_connectivity,
};
use super::releases_filter_bar::ReleasesFilterBar;
use super::releases_model::{ReleaseObject, ReleasesModel};
use super::releases_presentation::{releases_row_action, ReleasesRowAction};
use crate::ui::external_link::{self, LaunchErrorSlot};
use crate::ui::feed_footer::{FeedFooter, FeedFooterState};
use crate::ui::source_empty_state::SourceFailureState;
use crate::ui::source_error_banner::SourceErrorBanner;
use crate::ui::{one_shot_task, strings};

const LIST_PAGE: &str = "list";
const STATUS_PAGE: &str = "status";
const FAILURE_PAGE: &str = "failure";
type Callback = Rc<dyn Fn()>;
#[cfg(test)]
type FetchOverride = std::sync::Arc<
    dyn Fn(
            &Path,
            &mut dyn FnMut(artist_news::RefreshProgress),
        ) -> Result<artist_news::RefreshReport, artist_news::NewsError>
        + Send
        + Sync,
>;

struct Shared {
    conn: Rc<Db>,
    database_path: PathBuf,
    model: Rc<ReleasesModel>,
    filter_bar: Rc<ReleasesFilterBar>,
    end_of_results: Rc<crate::ui::end_of_results::EndOfResults>,
    rows: RefCell<Vec<HistoryEntry>>,
    cached_items: Cell<usize>,
    column_view: gtk4::ColumnView,
    column_model: Rc<dyn crate::ui::table_columns::EditorModel>,
    stack: gtk4::Stack,
    status: adw::StatusPage,
    status_button: gtk4::Button,
    footer: FeedFooter,
    error_banner: SourceErrorBanner,
    failure_state: SourceFailureState,
    fetch_failure: RefCell<Option<SourceError>>,
    failure_occurred_at: RefCell<String>,
    connectivity: Cell<Connectivity>,
    fetching: Cell<bool>,
    loaded_this_visit: Cell<bool>,
    empty_state: Cell<ReleasesEmptyState>,
    on_launch_error: LaunchErrorSlot,
    on_refreshed: RefCell<Option<Callback>>,
    #[cfg(test)]
    fetch_override: RefCell<Option<FetchOverride>>,
}

pub(in crate::ui) struct ReleasesView {
    root: gtk4::Widget,
    shared: Rc<Shared>,
}

impl ReleasesView {
    pub(in crate::ui) fn new(conn: Rc<Db>, database_path: PathBuf) -> Self {
        let model = Rc::new(ReleasesModel::new());
        let filter_bar = ReleasesFilterBar::new(conn.clone());
        let column_view = gtk4::ColumnView::builder()
            .model(model.selection())
            .show_row_separators(false)
            .show_column_separators(false)
            .build();
        column_view.add_css_class("reprise-releases-table");

        let shared_target = Rc::new(RefCell::new(None::<std::rc::Weak<Shared>>));
        let visibility_target = shared_target.clone();
        let on_set_hidden: releases_columns::OnSetHidden = Rc::new(move |mbid, hidden| {
            let shared = visibility_target
                .borrow()
                .as_ref()
                .and_then(std::rc::Weak::upgrade);
            if let Some(shared) = shared {
                set_hidden(&shared, &mbid, hidden);
            }
        });
        let launch_target = shared_target.clone();
        let on_open: releases_columns::OnOpenTarget = Rc::new(move |url| {
            let shared = launch_target
                .borrow()
                .as_ref()
                .and_then(std::rc::Weak::upgrade);
            if let Some(shared) = shared {
                external_link::launch(&url, "Bandcamp purchase", Some(&shared.on_launch_error));
            }
        });
        let date_column =
            releases_columns::append_columns(&column_view, &on_set_hidden, &on_open, &filter_bar);
        let column_registry = super::releases_column_layout::registry(&column_view, conn.clone());
        let column_model = super::releases_column_layout::model(&column_registry);
        crate::ui::table_columns::header_popover::install_header_popover(
            &column_view,
            &column_model,
        );
        crate::ui::table_columns::header_dnd::install_header_drag(&column_view, &column_model);
        let scrolled = gtk4::ScrolledWindow::builder()
            .child(&column_view)
            .vexpand(true)
            .hexpand(true)
            .build();
        let list_overlay = gtk4::Overlay::new();
        list_overlay.set_child(Some(&scrolled));
        let end_of_results = crate::ui::end_of_results::EndOfResults::install(
            &list_overlay,
            &scrolled,
            &column_view,
            crate::ui::end_of_results::ResultsUnit::Gaps,
        );
        {
            let filter_bar = filter_bar.clone();
            end_of_results.connect_recover(move || filter_bar.clear_all());
        }
        let status = adw::StatusPage::builder().vexpand(true).build();
        let status_button = gtk4::Button::new();
        status_button.add_css_class("pill");
        status_button.set_halign(gtk4::Align::Center);
        status.set_child(Some(&status_button));
        let stack = gtk4::Stack::builder()
            .transition_type(gtk4::StackTransitionType::Crossfade)
            .vexpand(true)
            .build();
        stack.add_named(&list_overlay, Some(LIST_PAGE));
        stack.add_named(&status, Some(STATUS_PAGE));
        let failure_state = SourceFailureState::new("star-new-symbolic");
        stack.add_named(failure_state.widget(), Some(FAILURE_PAGE));

        let footer = FeedFooter::new();
        let error_banner = SourceErrorBanner::new();
        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        root.add_css_class("reprise-releases-view");
        root.append(filter_bar.widget());
        root.append(error_banner.widget());
        root.append(&stack);
        root.append(footer.widget());

        let shared = Rc::new(Shared {
            conn,
            database_path,
            model,
            filter_bar: filter_bar.clone(),
            end_of_results,
            rows: RefCell::new(Vec::new()),
            cached_items: Cell::new(0),
            column_view: column_view.clone(),
            column_model,
            stack,
            status,
            status_button: status_button.clone(),
            footer,
            error_banner,
            failure_state,
            fetch_failure: RefCell::new(None),
            failure_occurred_at: RefCell::new(String::new()),
            connectivity: Cell::new(Connectivity::Online),
            fetching: Cell::new(false),
            loaded_this_visit: Cell::new(false),
            empty_state: Cell::new(ReleasesEmptyState::NeverFetched),
            on_launch_error: Rc::new(RefCell::new(None)),
            on_refreshed: RefCell::new(None),
            #[cfg(test)]
            fetch_override: RefCell::new(None),
        });
        shared_target.replace(Some(Rc::downgrade(&shared)));
        {
            let shared = Rc::downgrade(&shared);
            filter_bar.set_on_changed(move |_| {
                if let Some(shared) = shared.upgrade() {
                    if let Err(error) = render_cache(&shared) {
                        tracing::warn!(%error, "could not apply Releases filter");
                    }
                    notify_refreshed(&shared);
                }
            });
        }
        {
            let shared = shared.clone();
            status_button.connect_clicked(move |_| match shared.empty_state.get() {
                ReleasesEmptyState::NoResults => shared.filter_bar.show_widest(),
                ReleasesEmptyState::NeverFetched | ReleasesEmptyState::Empty => {
                    request_fetch(&shared);
                }
                ReleasesEmptyState::List => {}
            });
        }
        {
            let weak = Rc::downgrade(&shared);
            shared.footer.connect_reload(move || {
                if let Some(shared) = weak.upgrade() {
                    request_fetch(&shared);
                }
            });
        }
        {
            let shared = shared.clone();
            column_view.connect_activate(move |_, position| activate_position(&shared, position));
        }
        wire_sorting(&column_view, &shared);
        column_view.sort_by_column(Some(&date_column), gtk4::SortType::Descending);

        Self {
            root: root.upcast(),
            shared,
        }
    }

    pub(in crate::ui) fn root(&self) -> &gtk4::Widget {
        &self.root
    }

    pub(in crate::ui) fn column_model(&self) -> Rc<dyn crate::ui::table_columns::EditorModel> {
        self.shared.column_model.clone()
    }

    pub(in crate::ui) fn refresh(&self) {
        if let Err(error) = render_cache(&self.shared) {
            tracing::warn!(%error, "could not load Releases view");
        }
    }

    /// SEARCH-8a: applies this view's query (FIL-1d: title and artist).
    pub(in crate::ui) fn set_search_query(&self, query: &str) {
        self.shared.filter_bar.set_query(query);
    }

    pub(in crate::ui) fn set_committed_search_query(&self, query: &str) {
        self.shared.filter_bar.set_committed_query(query);
    }

    /// SEARCH-8a: the bar requests or reports a query transition to the shell.
    pub(in crate::ui) fn set_on_search_query_changed(&self, callback: impl Fn(&str) + 'static) {
        self.shared.filter_bar.set_on_query_changed(callback);
    }

    /// FIL-2a: "Clear all" for this view — its query and its facets.
    ///
    /// The shell reaches for this when a search found nothing here, so it has
    /// to open the catalog as wide as it goes. The filter row's own "Clear
    /// all" is the narrower promise: back to the default.
    pub(in crate::ui) fn clear_all_filters(&self) {
        self.shared.filter_bar.show_widest();
    }

    pub(in crate::ui) fn set_on_launch_error(&self, callback: impl Fn(String) + 'static) {
        *self.shared.on_launch_error.borrow_mut() = Some(Rc::new(callback));
    }

    pub(in crate::ui) fn set_on_refreshed(&self, callback: impl Fn() + 'static) {
        *self.shared.on_refreshed.borrow_mut() = Some(Rc::new(callback));
    }

    pub(in crate::ui) fn set_connectivity(&self, value: Connectivity) {
        self.shared.connectivity.set(value);
        let previous = self.shared.fetch_failure.borrow().clone();
        update_failure_for_connectivity(&mut self.shared.fetch_failure.borrow_mut(), value);
        if self.shared.fetch_failure.borrow().as_ref() != previous.as_ref() {
            *self.shared.failure_occurred_at.borrow_mut() = chrono::Utc::now().to_rfc3339();
        }
        apply_row_connectivity(&self.shared);
        if let Err(error) = render_cache(&self.shared) {
            tracing::warn!(%error, "could not apply New Releases connectivity");
        }
    }
}

/// FIL-1d: "in title and artist" — no other column takes part, so the chip's
/// promise and the match stay one statement.
fn releases_matching(rows: Vec<HistoryEntry>, query: &str) -> Vec<HistoryEntry> {
    if query.trim().is_empty() {
        return rows;
    }
    rows.into_iter()
        .filter(|entry| {
            reprise_view::search_scope::matches_any(
                [entry.title.as_str(), entry.artist_name.as_str()],
                query,
            )
        })
        .collect()
}

fn render_cache(shared: &Rc<Shared>) -> Result<(), rusqlite::Error> {
    let today = Local::now().date_naive();
    let filter = shared.filter_bar.filter();
    let query = shared.filter_bar.query();
    // FIL-1d: the query narrows what the facets already returned, matching
    // release title and artist — the two fields the chip names.
    let scoped = artist_news::query_releases_view_scope(&shared.conn, &filter, today)?;
    let rows = releases_matching(scoped.rows, &query);
    let restricted = releases_scope_is_filtered(&filter, &query);
    let total = scoped.widest_total;
    let latest = artist_news::latest_fetched_at(&shared.conn)?;
    shared.filter_bar.set_counts(rows.len(), total);
    shared
        .end_of_results
        .update(crate::ui::end_of_results::EndOfResultsInput {
            shown: rows.len(),
            total,
            query,
            facets_restrict: release_facets_restrict(&filter),
        });
    shared.rows.replace(rows.clone());
    shared.model.replace(rows.clone());
    shared.cached_items.set(total);
    apply_empty_state(
        shared,
        releases_empty_state_for(rows.len(), restricted, latest.is_none()),
        total,
    );
    if !shared.fetching.get() {
        apply_footer(shared, current_footer_state(shared, latest));
    }
    render_current_failure(shared);
    Ok(())
}

fn release_facets_restrict(filter: &artist_news::ReleasesFilter) -> bool {
    !(filter.release_types.is_empty() || filter.release_types.is_all())
        || filter.window != artist_news::ReleaseWindow::All
}

fn apply_empty_state(shared: &Shared, state: ReleasesEmptyState, total: usize) {
    shared.empty_state.set(state);
    if state == ReleasesEmptyState::List {
        shared.stack.set_visible_child_name(LIST_PAGE);
        return;
    }
    let (icon, title, action) = match state {
        ReleasesEmptyState::NeverFetched => (
            "star-new-symbolic",
            strings::text(strings::RELEASES_NO_DATA_TITLE),
            strings::text(strings::FETCH_NOW),
        ),
        ReleasesEmptyState::NoResults => (
            "system-search-symbolic",
            strings::text(strings::NO_RESULTS_TITLE),
            strings::show_all_releases(total),
        ),
        ReleasesEmptyState::Empty => (
            crate::ui::icons::DONE,
            strings::text(strings::RELEASES_EMPTY_TITLE),
            strings::text(strings::FETCH_NOW),
        ),
        ReleasesEmptyState::List => unreachable!("list state returns before status mutation"),
    };
    shared.status.set_icon_name(Some(icon));
    shared.status.set_title(&title);
    shared.status_button.set_label(&action);
    shared.stack.set_visible_child_name(STATUS_PAGE);
}

fn set_hidden(shared: &Rc<Shared>, mbid: &str, hidden: bool) {
    if let Err(error) = artist_news::set_release_hidden(&shared.conn, mbid, hidden) {
        tracing::warn!(%error, "could not change release visibility");
        return;
    }
    if let Err(error) = render_cache(shared) {
        tracing::warn!(%error, "could not reload Releases after visibility change");
    }
    notify_refreshed(shared);
}

fn activate_position(shared: &Rc<Shared>, position: u32) {
    let Some(object) = shared
        .model
        .store()
        .item(position)
        .and_downcast::<ReleaseObject>()
    else {
        return;
    };
    let entry = object.entry();
    match releases_row_action(&entry, Local::now().date_naive()) {
        ReleasesRowAction::Restore => set_hidden(shared, &entry.release_group_mbid, false),
        ReleasesRowAction::OpenAnnouncement(url) => {
            external_link::launch(&url, "release announcement", Some(&shared.on_launch_error));
        }
    }
}

fn notify_refreshed(shared: &Shared) {
    if let Some(callback) = shared.on_refreshed.borrow().clone() {
        callback();
    }
}

fn releases_footer_state(
    module_enabled: bool,
    network_enabled: bool,
    connectivity: Connectivity,
    fetching: bool,
    failed: bool,
    latest: Option<i64>,
    loaded_this_visit: bool,
) -> FeedFooterState {
    if !module_enabled {
        FeedFooterState::ModuleOff
    } else if !network_enabled {
        FeedFooterState::NetworkOff
    } else if fetching {
        FeedFooterState::Fetching {
            checked: 0,
            total: 0,
        }
    } else if connectivity == Connectivity::Offline {
        latest.map_or(FeedFooterState::NeverFetched, |latest| {
            FeedFooterState::Offline { latest }
        })
    } else if failed {
        latest.map_or(FeedFooterState::NeverFetched, |latest| {
            FeedFooterState::Failed { latest }
        })
    } else if let Some(at) = latest {
        if loaded_this_visit {
            FeedFooterState::Loaded { at }
        } else {
            FeedFooterState::Cached { at }
        }
    } else {
        FeedFooterState::NeverFetched
    }
}

fn current_footer_state(shared: &Shared, latest: Option<i64>) -> FeedFooterState {
    releases_footer_state(
        reprise_core::modules::is_enabled(
            &shared.conn,
            &reprise_core::modules::NEW_RELEASES_MODULE,
        )
        .unwrap_or(false),
        reprise_core::online_sources::is_enabled(&shared.conn).unwrap_or(false),
        shared.connectivity.get(),
        shared.fetching.get(),
        shared.fetch_failure.borrow().is_some(),
        latest,
        shared.loaded_this_visit.get(),
    )
}

fn apply_footer(shared: &Shared, state: FeedFooterState) {
    shared
        .footer
        .apply_with_copy(state, strings::releases_feed_footer_copy());
}

fn request_fetch(shared: &Rc<Shared>) {
    if shared.fetching.replace(true) {
        return;
    }
    apply_footer(
        shared,
        FeedFooterState::Fetching {
            checked: 0,
            total: 0,
        },
    );
    let path = shared.database_path.clone();
    #[cfg(test)]
    let fetch_override = shared.fetch_override.borrow().clone();
    let result = one_shot_task::spawn_with_progress("reprise-releases", move |publish| {
        #[cfg(test)]
        if let Some(fetch_override) = fetch_override {
            return fetch_override(&path, publish);
        }
        fetch_from_database(&path, publish)
    });
    let Ok((progress, receiver)) = result else {
        finish_fetch(
            shared,
            Some(SourceError::new(
                SourceErrorKind::Unreachable,
                "Queue New Releases refresh",
                "New Releases worker refused the refresh request",
            )),
        );
        return;
    };
    let weak = Rc::downgrade(shared);
    gtk4::glib::spawn_future_local(async move {
        while let Ok(progress) = progress.recv().await {
            let Some(shared) = weak.upgrade() else {
                return;
            };
            if !shared.fetching.get() {
                return;
            }
            apply_footer(
                &shared,
                FeedFooterState::Fetching {
                    checked: progress.checked,
                    total: progress.total,
                },
            );
        }
    });
    let weak = Rc::downgrade(shared);
    gtk4::glib::spawn_future_local(async move {
        let failure = match receiver.recv().await {
            Ok(Ok(report)) => report.failures.into_iter().next(),
            Ok(Err(error)) => {
                tracing::warn!(%error, "could not refresh Releases");
                Some(error.into_source_error())
            }
            Err(error) => {
                tracing::warn!(%error, "Releases worker closed without a result");
                Some(SourceError::new(
                    SourceErrorKind::Unreachable,
                    "Refresh New Releases",
                    error.to_string(),
                ))
            }
        };
        if let Some(shared) = weak.upgrade() {
            finish_fetch(&shared, failure);
        }
    });
}

fn fetch_from_database(
    path: &Path,
    on_progress: &mut dyn FnMut(artist_news::RefreshProgress),
) -> Result<artist_news::RefreshReport, artist_news::NewsError> {
    let conn = reprise_core::db::Db::open_migrated(Some(path))
        .map_err(|error| artist_news::NewsError::Database(error.to_string()))?;
    if !reprise_core::modules::is_enabled(&conn, &reprise_core::modules::NEW_RELEASES_MODULE)
        .map_err(|error| artist_news::NewsError::Database(error.to_string()))?
    {
        return Ok(artist_news::RefreshReport::default());
    }
    let today = Local::now().date_naive();
    let scope = artist_news::configured_fetch_scope(&conn)
        .map_err(|error| artist_news::NewsError::Database(error.to_string()))?;
    artist_news::refresh_with_progress(&conn, today, scope, true, on_progress)
}

fn finish_fetch(shared: &Rc<Shared>, failure: Option<SourceError>) {
    if failure.is_none() {
        if let Err(error) =
            reprise_core::library::settings::set_new_releases_fetch_completed(&shared.conn, true)
        {
            tracing::warn!(%error, "could not persist Releases fetch completion");
        }
    }
    shared.fetching.set(false);
    shared.loaded_this_visit.set(failure.is_none());
    shared.fetch_failure.replace(failure);
    if shared.fetch_failure.borrow().is_some() {
        *shared.failure_occurred_at.borrow_mut() = chrono::Utc::now().to_rfc3339();
    }
    if let Err(error) = render_cache(shared) {
        apply_footer(shared, current_footer_state(shared, None));
        tracing::warn!(%error, "could not reload Releases after fetch");
    }
    notify_refreshed(shared);
}

fn render_current_failure(shared: &Rc<Shared>) {
    let Some(error) = shared.fetch_failure.borrow().clone() else {
        shared.error_banner.hide();
        return;
    };
    let cached_items = shared.cached_items.get();
    let presentation = releases_failure_presentation(&error, cached_items);
    let updated = artist_news::latest_fetched_at(&shared.conn)
        .ok()
        .flatten()
        .map(strings::news_timestamp_date);
    let support = failure_support(cached_items, updated.as_deref());
    let occurred_at = shared.failure_occurred_at.borrow().clone();
    let weak = Rc::downgrade(shared);
    let dismiss_weak = weak.clone();
    let on_action = move |action| {
        let Some(shared) = weak.upgrade() else {
            return;
        };
        if action == FailureAction::TryAgain {
            request_fetch(&shared);
        }
    };
    let on_dismiss = move || {
        let Some(shared) = dismiss_weak.upgrade() else {
            return;
        };
        shared.fetch_failure.replace(None);
        shared.error_banner.hide();
    };
    match presentation.surface {
        FailureSurface::Banner => {
            shared.error_banner.show(
                &presentation,
                &support,
                &error,
                &occurred_at,
                on_action,
                on_dismiss,
            );
        }
        FailureSurface::FullArea => {
            shared.error_banner.hide();
            shared
                .failure_state
                .show(&presentation, &support, &error, &occurred_at, on_action);
            shared.stack.set_visible_child_name(FAILURE_PAGE);
        }
    }
}

fn apply_row_connectivity(shared: &Shared) {
    shared
        .column_view
        .set_opacity(if row_is_dimmed(shared.connectivity.get()) {
            0.55
        } else {
            1.0
        });
}

fn wire_sorting(column_view: &gtk4::ColumnView, shared: &Rc<Shared>) {
    let Some(sorter) = column_view
        .sorter()
        .and_downcast::<gtk4::ColumnViewSorter>()
    else {
        tracing::warn!("Releases table has no ColumnViewSorter");
        return;
    };
    let shared = shared.clone();
    sorter.connect_primary_sort_order_notify(move |sorter| {
        let direction = if sorter.primary_sort_order() == gtk4::SortType::Ascending {
            ReleaseSortDirection::Ascending
        } else {
            ReleaseSortDirection::Descending
        };
        let rows = shared.rows.borrow().clone();
        shared.model.replace(artist_news::sort_release_rows(
            rows,
            artist_news::ReleaseSortKey::Date,
            direction,
        ));
    });
}

#[cfg(test)]
#[path = "releases_view_tests.rs"]
mod tests;
