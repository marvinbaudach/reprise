#![allow(dead_code)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use chrono::Local;
use gtk4::prelude::*;
use libadwaita as adw;
use reprise_core::concerts::{self, ConcertFailure, ConcertFilter, ConcertRow};
use reprise_core::connectivity::Connectivity;
use reprise_core::db::Db;
use reprise_core::source_error::{FailureAction, FailureSurface, SourceError, SourceErrorKind};

use super::concerts_columns::{self, OnOpenTarget};
use super::concerts_empty_state::{
    concerts_empty_state_for, concerts_empty_state_presentation, ConcertsEmptyState,
};
use super::concerts_failure_ui::{
    concerts_failure_presentation, failure_support, row_is_dimmed, update_failure_for_connectivity,
};
use super::concerts_filter_bar::ConcertsFilterBar;
use super::concerts_location_banner::ConcertsLocationBanner;
use super::concerts_location_columns::LocationColumns;
use super::concerts_model::ConcertsModel;
use super::concerts_presentation::{sort_key_for_id, sort_rows, SortDirection};
use super::concerts_search::concerts_matching;
use super::concerts_worker::{request_allowed, ConcertsRequest, ConcertsResponse, ConcertsRuntime};
use crate::ui::external_link::{self, LaunchErrorSlot};
use crate::ui::feed_footer::{FeedFooter, FeedFooterState};
use crate::ui::location_broadcast::LocationBroadcast;
use crate::ui::source_empty_state::SourceFailureState;
use crate::ui::source_error_banner::SourceErrorBanner;

const LIST_PAGE: &str = "list";
const STATUS_PAGE: &str = "status";
const FAILURE_PAGE: &str = "failure";
const REFRESH_TIMER_SECONDS: u32 = 60 * 60;

type Callback = Rc<dyn Fn()>;

fn notify_filter_changed(runtime: &ConcertsRuntime) {
    runtime.notify_settings_changed();
}

struct Shared {
    conn: Rc<Db>,
    runtime: Rc<ConcertsRuntime>,
    model: Rc<ConcertsModel>,
    filter_bar: Rc<ConcertsFilterBar>,
    end_of_results: Rc<super::concerts_end_of_results::ConcertsEndOfResults>,
    rows: RefCell<Vec<ConcertRow>>,
    cached_items: Cell<usize>,
    column_view: gtk4::ColumnView,
    column_model: Rc<dyn crate::ui::table_columns::EditorModel>,
    location_columns: LocationColumns,
    stack: gtk4::Stack,
    status: adw::StatusPage,
    status_button: gtk4::Button,
    footer: FeedFooter,
    error_banner: SourceErrorBanner,
    location_banner: ConcertsLocationBanner,
    failure_state: SourceFailureState,
    fetch_failure: RefCell<Option<ConcertFailure>>,
    failure_occurred_at: RefCell<String>,
    connectivity: Cell<Connectivity>,
    fetching: Cell<bool>,
    loaded_this_visit: Cell<bool>,
    generation: Cell<u64>,
    refresh_timer: Cell<Option<gtk4::glib::SourceId>>,
    empty_state: Cell<ConcertsEmptyState>,
    on_clear_filters: RefCell<Option<Callback>>,
    on_refreshed: RefCell<Option<Callback>>,
    on_open_preferences: RefCell<Option<Callback>>,
    on_launch_error: LaunchErrorSlot,
}

pub(in crate::ui) struct ConcertsView {
    root: gtk4::Widget,
    shared: Rc<Shared>,
}

impl ConcertsView {
    pub(in crate::ui) fn new(
        conn: Rc<Db>,
        runtime: &Rc<ConcertsRuntime>,
        location_broadcast: &Rc<LocationBroadcast>,
    ) -> Self {
        let model = Rc::new(ConcertsModel::new());
        let filter_bar = ConcertsFilterBar::new(conn.clone());
        let column_view = gtk4::ColumnView::builder()
            .model(model.selection())
            .show_row_separators(false)
            .show_column_separators(false)
            .build();
        column_view.add_css_class("reprise-concerts-table");

        let launch_error: LaunchErrorSlot = Rc::new(RefCell::new(None));
        let launch_error_for_open = launch_error.clone();
        let on_open: OnOpenTarget = Rc::new(move |target| {
            external_link::launch(&target, "concert", Some(&launch_error_for_open));
        });
        let query_source: crate::ui::search_highlight::QuerySource = {
            let filter_bar = filter_bar.clone();
            Rc::new(move || filter_bar.query())
        };
        let radius_source: super::concerts_status_cells::RadiusSource = {
            let filter_bar = filter_bar.clone();
            Rc::new(move || filter_bar.filter().radius_km)
        };
        let columns = concerts_columns::append_columns(&column_view, &query_source, &radius_source);
        let column_registry = super::concerts_column_layout::registry(&column_view, conn.clone());
        let (location_columns, column_model) =
            LocationColumns::new(column_registry, &column_view, columns);
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
        let end_of_results = super::concerts_end_of_results::ConcertsEndOfResults::install(
            &list_overlay,
            &scrolled,
            &column_view,
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
        let failure_state = SourceFailureState::new("x-office-calendar-symbolic");
        stack.add_named(failure_state.widget(), Some(FAILURE_PAGE));

        let footer = FeedFooter::new();
        let error_banner = SourceErrorBanner::new();
        let location_banner = ConcertsLocationBanner::new();
        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        root.add_css_class("reprise-concerts-view");
        root.append(filter_bar.widget());
        root.append(location_banner.widget());
        root.append(error_banner.widget());
        root.append(&stack);
        root.append(footer.widget());

        let shared = Rc::new(Shared {
            conn,
            runtime: runtime.clone(),
            model,
            filter_bar: filter_bar.clone(),
            end_of_results,
            rows: RefCell::new(Vec::new()),
            cached_items: Cell::new(0),
            column_view: column_view.clone(),
            column_model,
            location_columns,
            stack,
            status,
            status_button: status_button.clone(),
            footer,
            error_banner,
            location_banner,
            failure_state,
            fetch_failure: RefCell::new(None),
            failure_occurred_at: RefCell::new(String::new()),
            connectivity: Cell::new(Connectivity::Online),
            fetching: Cell::new(false),
            loaded_this_visit: Cell::new(false),
            generation: Cell::new(0),
            refresh_timer: Cell::new(None),
            empty_state: Cell::new(ConcertsEmptyState::NeverFetched),
            on_clear_filters: RefCell::new(None),
            on_refreshed: RefCell::new(None),
            on_open_preferences: RefCell::new(None),
            on_launch_error: launch_error,
        });
        {
            let shared = Rc::downgrade(&shared);
            filter_bar.set_on_changed(move |_| {
                if let Some(shared) = shared.upgrade() {
                    notify_filter_changed(&shared.runtime);
                }
            });
        }
        {
            let shared = Rc::downgrade(&shared);
            filter_bar.set_on_open_location(move || {
                let Some(shared) = shared.upgrade() else {
                    return;
                };
                let callback = shared.on_open_preferences.borrow().clone();
                if let Some(callback) = callback {
                    callback();
                }
            });
        }
        {
            let weak = Rc::downgrade(&shared);
            shared.location_banner.set_on_open_location(move || {
                let Some(shared) = weak.upgrade() else {
                    return;
                };
                let callback = shared.on_open_preferences.borrow().clone();
                if let Some(callback) = callback {
                    callback();
                }
            });
        }
        {
            let filter_bar = filter_bar.clone();
            *shared.on_clear_filters.borrow_mut() = Some(Rc::new(move || {
                filter_bar.clear_all();
            }));
        }
        {
            let shared_weak = Rc::downgrade(&shared);
            shared.footer.connect_reload(move || {
                if let Some(shared) = shared_weak.upgrade() {
                    request_fetch(&shared, true);
                }
            });
        }

        {
            let shared = shared.clone();
            status_button.connect_clicked(move |_| {
                let callback = match shared.empty_state.get() {
                    ConcertsEmptyState::NoCredentials => None,
                    ConcertsEmptyState::NoResults => shared.on_clear_filters.borrow().clone(),
                    ConcertsEmptyState::NeverFetched
                    | ConcertsEmptyState::Empty
                    | ConcertsEmptyState::List => None,
                };
                if let Some(callback) = callback {
                    callback();
                }
            });
        }
        super::concerts_activation::wire(&column_view, &shared.model, on_open);
        {
            let root = root.downgrade();
            let shared = Rc::downgrade(&shared);
            runtime.subscribe_enabled(
                move || root.upgrade().is_some(),
                move |enabled| {
                    if let Some(shared) = shared.upgrade() {
                        enabled_changed(&shared, enabled);
                    }
                },
            );
        }
        {
            let root = root.downgrade();
            let shared = Rc::downgrade(&shared);
            location_broadcast.subscribe(
                move || root.upgrade().is_some(),
                move || {
                    let Some(shared) = shared.upgrade() else {
                        return;
                    };
                    if let Err(error) = shared.filter_bar.reload_persisted() {
                        tracing::warn!(%error, "could not reload app location settings");
                        return;
                    }
                    if let Err(error) = render_cache(&shared) {
                        tracing::warn!(%error, "could not apply app location settings");
                        return;
                    }
                    let callback = shared.on_refreshed.borrow().clone();
                    if let Some(callback) = callback {
                        callback();
                    }
                },
            );
        }
        {
            let root = root.downgrade();
            let shared = Rc::downgrade(&shared);
            runtime.subscribe_settings(
                move || root.upgrade().is_some(),
                move || {
                    let Some(shared) = shared.upgrade() else {
                        return;
                    };
                    if let Err(error) = shared.filter_bar.reload_persisted() {
                        tracing::warn!(%error, "could not reload Concerts settings");
                        return;
                    }
                    if let Err(error) = render_cache(&shared) {
                        tracing::warn!(%error, "could not apply Concerts settings");
                        return;
                    }
                    let callback = shared.on_refreshed.borrow().clone();
                    if let Some(callback) = callback {
                        callback();
                    }
                },
            );
        }
        wire_sorting(&column_view, &shared);
        shared.location_columns.sort_by_date();

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

    /// SEARCH-8a: applies this view's query (FIL-1d: artist and venue).
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
    pub(in crate::ui) fn clear_all_filters(&self) {
        self.shared.filter_bar.clear_all();
    }

    pub(in crate::ui) fn refresh(&self) {
        self.shared.loaded_this_visit.set(false);
        if let Err(error) = render_cache(&self.shared) {
            tracing::warn!(%error, "could not load concerts view");
        }
        maybe_background_refresh(&self.shared);
    }

    pub(in crate::ui) fn set_on_clear_filters(&self, callback: impl Fn() + 'static) {
        *self.shared.on_clear_filters.borrow_mut() = Some(Rc::new(callback));
    }

    pub(in crate::ui) fn set_on_launch_error(&self, callback: impl Fn(String) + 'static) {
        *self.shared.on_launch_error.borrow_mut() = Some(Rc::new(callback));
    }

    pub(in crate::ui) fn set_on_refreshed(&self, callback: impl Fn() + 'static) {
        *self.shared.on_refreshed.borrow_mut() = Some(Rc::new(callback));
    }

    pub(in crate::ui) fn set_on_open_preferences(&self, callback: impl Fn() + 'static) {
        *self.shared.on_open_preferences.borrow_mut() = Some(Rc::new(callback));
    }

    pub(in crate::ui) fn set_connectivity(&self, value: Connectivity) {
        self.shared.connectivity.set(value);
        let can_fetch = concerts::config::credentials(&self.shared.conn)
            .is_ok_and(|credentials| !credentials.is_empty());
        let previous = self.shared.fetch_failure.borrow().clone();
        update_failure_for_connectivity(
            &mut self.shared.fetch_failure.borrow_mut(),
            value,
            can_fetch,
        );
        if self.shared.fetch_failure.borrow().as_ref() != previous.as_ref() {
            *self.shared.failure_occurred_at.borrow_mut() = chrono::Utc::now().to_rfc3339();
        }
        apply_row_connectivity(&self.shared);
        if let Err(error) = render_cache(&self.shared) {
            tracing::warn!(%error, "could not apply Concerts connectivity");
        }
    }
}

fn render_cache(shared: &Rc<Shared>) -> Result<(), rusqlite::Error> {
    let today = Local::now().date_naive();
    let conn = &shared.conn;
    let filter = shared.filter_bar.filter();
    let app_location = reprise_core::location::app_location(conn)?;
    let credentials = concerts::config::credentials(conn)?;
    let similar_enabled = concerts::config::similar_config(conn)?.enabled;
    let has_similar_rows = concerts::has_similar_events(conn)?;
    let query = shared.filter_bar.query();
    // FIL-1d: the query narrows what the facets returned, matching artist and
    // venue — the two fields the chip names.
    let rows = concerts_matching(
        concerts::query_events(conn, &filter, app_location.as_ref(), today)?,
        &query,
    );
    let facets_restrict = filter.country.is_some()
        || filter.horizon != reprise_core::concerts::DateHorizon::AllUpcoming
        || (app_location.is_some() && filter.radius_km.is_some());
    let restricted = filter != ConcertFilter::default() || !query.is_empty();
    let total = if restricted {
        concerts::count_upcoming(
            conn,
            &ConcertFilter::default(),
            app_location.as_ref(),
            today,
        )? as usize
    } else {
        rows.len()
    };
    let latest_fetch = concerts::latest_fetch_at(conn)?;
    let never_fetched = latest_fetch.is_none();
    shared
        .filter_bar
        .set_context(app_location.as_ref(), similar_enabled, has_similar_rows);
    shared.filter_bar.set_counts(rows.len(), total);
    shared.location_columns.apply(app_location.is_some());
    if app_location.is_some() {
        shared.location_banner.hide();
    } else {
        shared.location_banner.show(total);
    }
    shared
        .end_of_results
        .update(super::concerts_end_of_results::Input {
            shown: rows.len(),
            total,
            query,
            facets_restrict,
            radius_km: app_location.as_ref().and(filter.radius_km),
            city: app_location.map(|location| location.name),
        });
    shared.rows.replace(rows.clone());
    shared.model.replace(rows.clone());
    shared.cached_items.set(total);
    let state = concerts_empty_state_for(
        rows.len(),
        restricted,
        !credentials.is_empty(),
        never_fetched,
    );
    apply_empty_state(shared, state, total);
    apply_footer(shared, latest_fetch);
    render_current_failure(shared);
    Ok(())
}

fn apply_empty_state(shared: &Shared, state: ConcertsEmptyState, total: usize) {
    shared.empty_state.set(state);
    if state == ConcertsEmptyState::List {
        shared.stack.set_visible_child_name(LIST_PAGE);
        return;
    }

    let presentation = concerts_empty_state_presentation(state, total);
    shared.status.set_icon_name(Some(presentation.icon));
    shared.status.set_title(&presentation.title);
    shared
        .status
        .set_description(Some(&presentation.description));
    shared
        .status_button
        .set_visible(presentation.action.is_some());
    if let Some(action) = presentation.action {
        shared.status_button.set_label(&action);
    }
    shared.stack.set_visible_child_name(STATUS_PAGE);
}

fn maybe_background_refresh(shared: &Rc<Shared>) {
    let latest = concerts::latest_fetch_at(&shared.conn).ok().flatten();
    let due = concerts::refresh_due(
        latest,
        chrono::Utc::now().timestamp(),
        shared.runtime.jitter_seconds(),
    );
    if request_allowed(shared.runtime.enabled.get(), shared.fetching.get(), due) {
        request_fetch(shared, false);
    }
}

fn request_fetch(shared: &Rc<Shared>, force: bool) {
    let has_credentials = {
        let conn = &shared.conn;
        concerts::config::credentials(conn).is_ok_and(|credentials| !credentials.is_empty())
    };
    if !has_credentials
        || !request_allowed(shared.runtime.enabled.get(), shared.fetching.get(), true)
    {
        return;
    }
    if shared.fetching.replace(true) {
        return;
    }
    apply_footer(
        shared,
        concerts::latest_fetch_at(&shared.conn).ok().flatten(),
    );

    let generation = shared.generation.get().wrapping_add(1);
    shared.generation.set(generation);
    let (sender, receiver) = async_channel::bounded(1);
    let (progress_sender, progress_receiver) = async_channel::unbounded();
    if !shared.runtime.request_with_progress(
        ConcertsRequest {
            generation,
            force,
            response: sender,
        },
        progress_sender,
    ) {
        finish_fetch(
            shared,
            Some(ConcertFailure::Source(SourceError::new(
                SourceErrorKind::Unreachable,
                "Queue Concerts refresh",
                "Concerts worker refused the refresh request",
            ))),
        );
        return;
    }
    let progress_weak = Rc::downgrade(shared);
    gtk4::glib::spawn_future_local(async move {
        while let Ok(progress) = progress_receiver.recv().await {
            let Some(shared) = progress_weak.upgrade() else {
                return;
            };
            if !shared.fetching.get() || shared.generation.get() != generation {
                return;
            }
            shared.footer.apply(FeedFooterState::Fetching {
                checked: progress.checked,
                total: progress.total,
            });
        }
    });
    let weak = Rc::downgrade(shared);
    gtk4::glib::spawn_future_local(async move {
        let response = receiver.recv().await;
        let Some(shared) = weak.upgrade() else {
            return;
        };
        let failure = match response {
            Ok(ConcertsResponse {
                generation: response_generation,
                result,
            }) if response_generation == shared.generation.get() => match result {
                Ok(summary) => summary.failures.into_iter().next(),
                Err(error) => {
                    tracing::warn!(%error, "could not refresh Concerts");
                    Some(error.into_source_failure())
                }
            },
            Ok(_) => return,
            Err(error) => {
                tracing::warn!(%error, "Concerts worker closed without a result");
                Some(ConcertFailure::Source(SourceError::new(
                    SourceErrorKind::Unreachable,
                    "Refresh Concerts",
                    error.to_string(),
                )))
            }
        };
        finish_fetch(&shared, failure);
    });
}

fn finish_fetch(shared: &Rc<Shared>, failure: Option<ConcertFailure>) {
    shared.fetching.set(false);
    if failure.is_none() {
        shared.loaded_this_visit.set(true);
    }
    shared.fetch_failure.replace(failure);
    if shared.fetch_failure.borrow().is_some() {
        *shared.failure_occurred_at.borrow_mut() = chrono::Utc::now().to_rfc3339();
    }
    if let Err(error) = render_cache(shared) {
        tracing::warn!(%error, "could not reload Concerts after fetch");
    }
    let callback = shared.on_refreshed.borrow().clone();
    if let Some(callback) = callback {
        callback();
    }
}

fn render_current_failure(shared: &Rc<Shared>) {
    let Some(failure) = shared.fetch_failure.borrow().clone() else {
        shared.error_banner.hide();
        return;
    };
    let cached_items = shared.cached_items.get();
    let presentation = concerts_failure_presentation(&failure, cached_items);
    let support = failure_support(&failure, cached_items);
    let error = failure.source_error().clone();
    let occurred_at = shared.failure_occurred_at.borrow().clone();
    let weak = Rc::downgrade(shared);
    let dismiss_weak = weak.clone();
    let on_action = move |action| {
        let Some(shared) = weak.upgrade() else {
            return;
        };
        match action {
            FailureAction::TryAgain => request_fetch(&shared, true),
            FailureAction::OpenPreferences => {
                let callback = shared.on_open_preferences.borrow().clone();
                if let Some(callback) = callback {
                    callback();
                }
            }
            FailureAction::CheckSubscription
            | FailureAction::Unsubscribe
            | FailureAction::FindNewUrl => {}
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

fn enabled_changed(shared: &Rc<Shared>, enabled: bool) {
    if enabled {
        start_refresh_timer(shared);
    } else {
        stop_refresh_timer(shared);
    }
    if let Err(error) = render_cache(shared) {
        tracing::warn!(%error, "could not apply Concerts module state");
    }
}

fn apply_footer(shared: &Shared, latest_fetch: Option<i64>) {
    let module_enabled =
        reprise_core::modules::is_enabled(&shared.conn, &reprise_core::modules::CONCERTS_MODULE)
            .unwrap_or(false);
    let network_enabled = reprise_core::online_sources::is_enabled(&shared.conn).unwrap_or(false);
    let has_credentials = concerts::config::credentials(&shared.conn)
        .is_ok_and(|credentials| !credentials.is_empty());
    let state = if !module_enabled {
        FeedFooterState::ModuleOff
    } else if !network_enabled {
        FeedFooterState::NetworkOff
    } else if !has_credentials {
        FeedFooterState::NoCredentials
    } else if shared.fetching.get() {
        FeedFooterState::Fetching {
            checked: 0,
            total: 0,
        }
    } else if shared.connectivity.get() == Connectivity::Offline {
        latest_fetch.map_or(FeedFooterState::NeverFetched, |latest| {
            FeedFooterState::Offline { latest }
        })
    } else if shared.fetch_failure.borrow().is_some() {
        latest_fetch.map_or(FeedFooterState::NeverFetched, |latest| {
            FeedFooterState::Failed { latest }
        })
    } else if let Some(at) = latest_fetch {
        if shared.loaded_this_visit.get() {
            FeedFooterState::Loaded { at }
        } else {
            FeedFooterState::Cached { at }
        }
    } else {
        FeedFooterState::NeverFetched
    };
    shared.footer.apply(state);
}

fn start_refresh_timer(shared: &Rc<Shared>) {
    let existing = shared.refresh_timer.take();
    if existing.is_some() {
        shared.refresh_timer.set(existing);
        return;
    }
    let weak = Rc::downgrade(shared);
    let source = gtk4::glib::timeout_add_seconds_local(REFRESH_TIMER_SECONDS, move || {
        let Some(shared) = weak.upgrade() else {
            return gtk4::glib::ControlFlow::Break;
        };
        maybe_background_refresh(&shared);
        gtk4::glib::ControlFlow::Continue
    });
    shared.refresh_timer.set(Some(source));
}

fn stop_refresh_timer(shared: &Shared) {
    if let Some(source) = shared.refresh_timer.take() {
        source.remove();
    }
}

fn wire_sorting(column_view: &gtk4::ColumnView, shared: &Rc<Shared>) {
    let Some(sorter) = column_view
        .sorter()
        .and_downcast::<gtk4::ColumnViewSorter>()
    else {
        tracing::warn!("concerts table has no ColumnViewSorter");
        return;
    };
    {
        let shared = shared.clone();
        sorter.connect_primary_sort_column_notify(move |sorter| apply_sort(&shared, sorter));
    }
    {
        let shared = shared.clone();
        sorter.connect_primary_sort_order_notify(move |sorter| apply_sort(&shared, sorter));
    }
}

fn apply_sort(shared: &Shared, sorter: &gtk4::ColumnViewSorter) {
    let Some(column) = sorter.primary_sort_column() else {
        return;
    };
    let Some(key) = sort_key_for_id(column.id().as_deref()) else {
        return;
    };
    let direction = if sorter.primary_sort_order() == gtk4::SortType::Descending {
        SortDirection::Descending
    } else {
        SortDirection::Ascending
    };
    let mut rows = shared.rows.borrow().clone();
    sort_rows(&mut rows, key, direction);
    shared.model.replace(rows);
}

#[cfg(test)]
#[path = "concerts_view_tests.rs"]
mod tests;
