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
use super::concerts_model::{ConcertObject, ConcertsModel};
use super::concerts_presentation::{sort_rows, updated_ago, ConcertSortKey, SortDirection};
use super::concerts_worker::{request_allowed, ConcertsRequest, ConcertsResponse, ConcertsRuntime};
use crate::ui::external_link::{self, LaunchErrorSlot};
use crate::ui::source_empty_state::SourceFailureState;
use crate::ui::source_error_banner::SourceErrorBanner;
use crate::ui::strings;

const LIST_PAGE: &str = "list";
const STATUS_PAGE: &str = "status";
const FAILURE_PAGE: &str = "failure";
const FETCH_BUTTON_PAGE: &str = "button";
const FETCH_SPINNER_PAGE: &str = "spinner";
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
    rows: RefCell<Vec<ConcertRow>>,
    cached_items: Cell<usize>,
    column_view: gtk4::ColumnView,
    stack: gtk4::Stack,
    status: adw::StatusPage,
    status_button: gtk4::Button,
    fetch_button: gtk4::Button,
    fetch_stack: gtk4::Stack,
    spinner: gtk4::Spinner,
    updated: gtk4::Label,
    error_banner: SourceErrorBanner,
    failure_state: SourceFailureState,
    fetch_failure: RefCell<Option<ConcertFailure>>,
    failure_occurred_at: RefCell<String>,
    connectivity: Cell<Connectivity>,
    fetching: Cell<bool>,
    generation: Cell<u64>,
    refresh_timer: Cell<Option<gtk4::glib::SourceId>>,
    empty_state: Cell<ConcertsEmptyState>,
    on_fetch_now: RefCell<Option<Callback>>,
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
    pub(in crate::ui) fn new(conn: Rc<Db>, runtime: &Rc<ConcertsRuntime>) -> Self {
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
        let columns = concerts_columns::append_columns(&column_view, &on_open);

        let scrolled = gtk4::ScrolledWindow::builder()
            .child(&column_view)
            .vexpand(true)
            .hexpand(true)
            .build();
        let status = adw::StatusPage::builder().vexpand(true).build();
        let status_button = gtk4::Button::new();
        status_button.add_css_class("pill");
        status_button.set_halign(gtk4::Align::Center);
        status.set_child(Some(&status_button));
        let stack = gtk4::Stack::builder()
            .transition_type(gtk4::StackTransitionType::Crossfade)
            .vexpand(true)
            .build();
        stack.add_named(&scrolled, Some(LIST_PAGE));
        stack.add_named(&status, Some(STATUS_PAGE));
        let failure_state = SourceFailureState::new("x-office-calendar-symbolic");
        stack.add_named(failure_state.widget(), Some(FAILURE_PAGE));

        let (footer, updated, fetch_button, fetch_stack, spinner) = build_footer();
        let error_banner = SourceErrorBanner::new();
        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        root.add_css_class("reprise-concerts-view");
        root.append(filter_bar.widget());
        root.append(error_banner.widget());
        root.append(&stack);
        root.append(&footer);

        let shared = Rc::new(Shared {
            conn,
            runtime: runtime.clone(),
            model,
            filter_bar: filter_bar.clone(),
            rows: RefCell::new(Vec::new()),
            cached_items: Cell::new(0),
            column_view: column_view.clone(),
            stack,
            status,
            status_button: status_button.clone(),
            fetch_button: fetch_button.clone(),
            fetch_stack,
            spinner,
            updated,
            error_banner,
            failure_state,
            fetch_failure: RefCell::new(None),
            failure_occurred_at: RefCell::new(String::new()),
            connectivity: Cell::new(Connectivity::Online),
            fetching: Cell::new(false),
            generation: Cell::new(0),
            refresh_timer: Cell::new(None),
            empty_state: Cell::new(ConcertsEmptyState::NeverFetched),
            on_fetch_now: RefCell::new(None),
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
            let filter_bar = filter_bar.clone();
            *shared.on_clear_filters.borrow_mut() = Some(Rc::new(move || {
                filter_bar.clear_all();
            }));
        }
        {
            let shared_weak = Rc::downgrade(&shared);
            *shared.on_fetch_now.borrow_mut() = Some(Rc::new(move || {
                if let Some(shared) = shared_weak.upgrade() {
                    request_fetch(&shared, true);
                }
            }));
        }
        {
            let shared = Rc::downgrade(&shared);
            fetch_button.connect_clicked(move |_| {
                if let Some(shared) = shared.upgrade() {
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
                    ConcertsEmptyState::NeverFetched | ConcertsEmptyState::Empty => {
                        shared.on_fetch_now.borrow().clone()
                    }
                    ConcertsEmptyState::List => None,
                };
                if let Some(callback) = callback {
                    callback();
                }
            });
        }
        {
            let shared = shared.clone();
            column_view.connect_activate(move |_, position| {
                let Some(object) = shared
                    .model
                    .store()
                    .item(position)
                    .and_downcast::<ConcertObject>()
                else {
                    return;
                };
                if let Some(target) = concerts_columns::ticket_target(&object.row()) {
                    on_open(target.to_owned());
                }
            });
        }
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
        column_view.sort_by_column(Some(&columns.date), gtk4::SortType::Ascending);

        Self {
            root: root.upcast(),
            shared,
        }
    }

    pub(in crate::ui) fn root(&self) -> &gtk4::Widget {
        &self.root
    }

    pub(in crate::ui) fn refresh(&self) {
        if let Err(error) = render_cache(&self.shared) {
            tracing::warn!(%error, "could not load concerts view");
        }
        maybe_background_refresh(&self.shared);
    }

    pub(in crate::ui) fn set_on_fetch_now(&self, callback: impl Fn() + 'static) {
        *self.shared.on_fetch_now.borrow_mut() = Some(Rc::new(callback));
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
    let location = concerts::config::location(conn)?;
    let credentials = concerts::config::credentials(conn)?;
    let similar_enabled = concerts::config::similar_config(conn)?.enabled;
    let has_similar_rows = concerts::has_similar_events(conn)?;
    let rows = concerts::query_events(conn, &filter, location.as_ref(), today)?;
    let total = if filter == ConcertFilter::default() {
        rows.len()
    } else {
        concerts::count_upcoming(conn, &ConcertFilter::default(), location.as_ref(), today)?
            as usize
    };
    let latest_fetch = concerts::latest_fetch_at(conn)?;
    let never_fetched = latest_fetch.is_none();
    shared
        .filter_bar
        .set_context(location.is_some(), similar_enabled, has_similar_rows);
    shared.filter_bar.set_counts(rows.len(), total);
    shared.rows.replace(rows.clone());
    shared.model.replace(rows.clone());
    shared.cached_items.set(total);
    let state = concerts_empty_state_for(
        rows.len(),
        filter != ConcertFilter::default(),
        !credentials.is_empty(),
        never_fetched,
    );
    apply_empty_state(shared, state, total);
    shared
        .updated
        .set_label(&updated_ago(latest_fetch, chrono::Utc::now().timestamp()));
    render_current_failure(shared);
    Ok(())
}

fn apply_empty_state(shared: &Shared, state: ConcertsEmptyState, total: usize) {
    shared.empty_state.set(state);
    shared
        .fetch_stack
        .set_visible(state != ConcertsEmptyState::NoCredentials);
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

fn build_footer() -> (
    gtk4::Box,
    gtk4::Label,
    gtk4::Button,
    gtk4::Stack,
    gtk4::Spinner,
) {
    let footer = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    footer.set_margin_top(6);
    footer.set_margin_bottom(6);
    footer.set_margin_start(12);
    footer.set_margin_end(12);
    let updated = gtk4::Label::new(None);
    updated.add_css_class("dim-label");
    updated.add_css_class("caption");
    updated.set_hexpand(true);
    footer.append(&updated);
    let fetch_button = gtk4::Button::with_label(&strings::text(strings::FETCH_NOW));
    fetch_button.add_css_class("flat");
    let spinner = gtk4::Spinner::new();
    let fetch_stack = gtk4::Stack::new();
    fetch_stack.set_transition_type(gtk4::StackTransitionType::Crossfade);
    fetch_stack.add_named(&fetch_button, Some(FETCH_BUTTON_PAGE));
    fetch_stack.add_named(&spinner, Some(FETCH_SPINNER_PAGE));
    fetch_stack.set_visible_child_name(FETCH_BUTTON_PAGE);
    footer.append(&fetch_stack);
    (footer, updated, fetch_button, fetch_stack, spinner)
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
    shared.fetch_button.set_sensitive(false);
    shared
        .fetch_stack
        .set_visible_child_name(FETCH_SPINNER_PAGE);
    shared.spinner.start();

    let generation = shared.generation.get().wrapping_add(1);
    shared.generation.set(generation);
    let (sender, receiver) = async_channel::bounded(1);
    if !shared.runtime.request(ConcertsRequest {
        generation,
        force,
        response: sender,
    }) {
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
    shared.spinner.stop();
    shared.fetch_stack.set_visible_child_name(FETCH_BUTTON_PAGE);
    shared.fetch_button.set_sensitive(true);
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
    let support = failure_support(&failure, cached_items, shared.updated.text().as_str());
    let error = failure.source_error().clone();
    let occurred_at = shared.failure_occurred_at.borrow().clone();
    let weak = Rc::downgrade(shared);
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
    match presentation.surface {
        FailureSurface::Banner => {
            shared
                .error_banner
                .show(&presentation, &support, &error, &occurred_at, on_action);
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
    let key = match column.id().as_deref() {
        Some("distance") => ConcertSortKey::Distance,
        _ => ConcertSortKey::Date,
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
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn conc_3_concerts_view_exposes_six_columns_and_row_activation() {
        gtk4::init().unwrap();
        let conn = Rc::new(crate::test_db::open().unwrap());
        let runtime = ConcertsRuntime::setup(&conn);
        let view = ConcertsView::new(conn, &runtime);
        let root = view.root().clone().downcast::<gtk4::Box>().unwrap();
        let stack = root
            .first_child()
            .and_then(|child| child.next_sibling())
            .and_then(|child| child.next_sibling())
            .and_downcast::<gtk4::Stack>()
            .unwrap();
        let scrolled = stack
            .child_by_name(LIST_PAGE)
            .and_downcast::<gtk4::ScrolledWindow>()
            .unwrap();
        let table = scrolled.child().and_downcast::<gtk4::ColumnView>().unwrap();
        assert_eq!(table.columns().n_items(), 6);
        assert!(!table.enables_rubberband());
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn conc_5a_footer_keeps_fetch_progress_below_the_live_table() {
        gtk4::init().unwrap();
        let conn = Rc::new(crate::test_db::open().unwrap());
        let runtime = ConcertsRuntime::setup(&conn);
        let view = ConcertsView::new(conn, &runtime);
        let root = view.root().clone().downcast::<gtk4::Box>().unwrap();
        let footer = root.last_child().and_downcast::<gtk4::Box>().unwrap();
        let fetch_stack = footer.last_child().and_downcast::<gtk4::Stack>().unwrap();
        assert!(fetch_stack.child_by_name(FETCH_BUTTON_PAGE).is_some());
        assert!(fetch_stack.child_by_name(FETCH_SPINNER_PAGE).is_some());
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn conc_4b_settings_changes_re_evaluate_credentials_and_refresh_dependents() {
        gtk4::init().unwrap();
        let conn = Rc::new(crate::test_db::open().unwrap());
        let runtime = ConcertsRuntime::setup(&conn);
        let view = ConcertsView::new(conn.clone(), &runtime);
        let refreshes = Rc::new(Cell::new(0));
        view.set_on_refreshed({
            let refreshes = refreshes.clone();
            move || refreshes.set(refreshes.get() + 1)
        });

        view.refresh();
        assert_eq!(
            view.shared.empty_state.get(),
            ConcertsEmptyState::NoCredentials
        );
        assert!(!view.shared.fetch_stack.is_visible());

        reprise_core::library::settings::set_setting(
            &conn,
            reprise_core::concerts::config::TICKETMASTER_API_KEY,
            "stored-key",
        )
        .unwrap();
        runtime.notify_settings_changed();
        assert_eq!(
            view.shared.empty_state.get(),
            ConcertsEmptyState::NeverFetched
        );
        assert!(view.shared.fetch_stack.is_visible());
        assert_eq!(refreshes.get(), 1);

        reprise_core::library::settings::set_setting(
            &conn,
            reprise_core::concerts::config::TICKETMASTER_API_KEY,
            "",
        )
        .unwrap();
        runtime.notify_settings_changed();
        assert_eq!(
            view.shared.empty_state.get(),
            ConcertsEmptyState::NoCredentials
        );
        assert!(!view.shared.fetch_stack.is_visible());
        assert_eq!(refreshes.get(), 2);
    }

    #[test]
    fn conc_7_filter_changes_refresh_badge_dependents() {
        let conn = crate::test_db::open().unwrap();
        let runtime = ConcertsRuntime::setup(&conn);
        let refreshes = Rc::new(Cell::new(0));
        runtime.subscribe_settings(|| true, {
            let refreshes = refreshes.clone();
            move || refreshes.set(refreshes.get() + 1)
        });

        notify_filter_changed(&runtime);

        assert_eq!(refreshes.get(), 1);
    }
}
