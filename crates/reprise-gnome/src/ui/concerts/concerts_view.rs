#![allow(dead_code)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use chrono::Local;
use gtk4::gio;
use gtk4::prelude::*;
use libadwaita as adw;
use reprise_core::concerts::{self, ConcertFilter, ConcertRow};
use rusqlite::Connection;

use super::concerts_columns::{self, OnOpenTarget};
use super::concerts_empty_state::{concerts_empty_state_for, ConcertsEmptyState};
use super::concerts_filter_bar::ConcertsFilterBar;
use super::concerts_model::{ConcertObject, ConcertsModel};
use super::concerts_presentation::{sort_rows, updated_ago, ConcertSortKey, SortDirection};
use super::concerts_worker::{request_allowed, ConcertsRequest, ConcertsResponse, ConcertsRuntime};
use crate::ui::strings;

const LIST_PAGE: &str = "list";
const STATUS_PAGE: &str = "status";
const FETCH_BUTTON_PAGE: &str = "button";
const FETCH_SPINNER_PAGE: &str = "spinner";
const REFRESH_TIMER_SECONDS: u32 = 60 * 60;

type Callback = Rc<dyn Fn()>;
type ErrorCallback = Rc<dyn Fn(String)>;

struct Shared {
    conn: Rc<RefCell<Connection>>,
    runtime: Rc<ConcertsRuntime>,
    model: Rc<ConcertsModel>,
    filter_bar: Rc<ConcertsFilterBar>,
    rows: RefCell<Vec<ConcertRow>>,
    stack: gtk4::Stack,
    status: adw::StatusPage,
    status_button: gtk4::Button,
    fetch_button: gtk4::Button,
    fetch_stack: gtk4::Stack,
    spinner: gtk4::Spinner,
    updated: gtk4::Label,
    failure: gtk4::Label,
    fetching: Cell<bool>,
    generation: Cell<u64>,
    refresh_timer: Cell<Option<gtk4::glib::SourceId>>,
    empty_state: Cell<ConcertsEmptyState>,
    on_fetch_now: RefCell<Option<Callback>>,
    on_clear_filters: RefCell<Option<Callback>>,
    on_open_preferences: RefCell<Option<Callback>>,
    on_refreshed: RefCell<Option<Callback>>,
    on_launch_error: Rc<RefCell<Option<ErrorCallback>>>,
}

pub(in crate::ui) struct ConcertsView {
    root: gtk4::Widget,
    shared: Rc<Shared>,
}

impl ConcertsView {
    pub(in crate::ui) fn new(conn: Rc<RefCell<Connection>>, runtime: &Rc<ConcertsRuntime>) -> Self {
        let model = Rc::new(ConcertsModel::new());
        let filter_bar = ConcertsFilterBar::new(conn.clone());
        let column_view = gtk4::ColumnView::builder()
            .model(model.selection())
            .show_row_separators(false)
            .show_column_separators(false)
            .build();
        column_view.add_css_class("reprise-concerts-table");

        let launch_error = Rc::new(RefCell::new(None::<ErrorCallback>));
        let launch_error_for_open = launch_error.clone();
        let on_open: OnOpenTarget = Rc::new(move |target| {
            let error_callback = launch_error_for_open.clone();
            let target_for_log = target.clone();
            gtk4::UriLauncher::new(&target).launch(
                None::<&gtk4::Window>,
                gio::Cancellable::NONE,
                move |result| {
                    if let Err(error) = result {
                        tracing::warn!(
                            %error,
                            target = target_for_log,
                            "could not open concert URL"
                        );
                        if let Some(callback) = error_callback.borrow().as_ref() {
                            callback(error.to_string());
                        }
                    }
                },
            );
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

        let (footer, updated, failure, fetch_button, fetch_stack, spinner) = build_footer();
        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        root.add_css_class("reprise-concerts-view");
        root.append(filter_bar.widget());
        root.append(&stack);
        root.append(&footer);

        let shared = Rc::new(Shared {
            conn,
            runtime: runtime.clone(),
            model,
            filter_bar: filter_bar.clone(),
            rows: RefCell::new(Vec::new()),
            stack,
            status,
            status_button: status_button.clone(),
            fetch_button: fetch_button.clone(),
            fetch_stack,
            spinner,
            updated,
            failure,
            fetching: Cell::new(false),
            generation: Cell::new(0),
            refresh_timer: Cell::new(None),
            empty_state: Cell::new(ConcertsEmptyState::NeverFetched),
            on_fetch_now: RefCell::new(None),
            on_clear_filters: RefCell::new(None),
            on_open_preferences: RefCell::new(None),
            on_refreshed: RefCell::new(None),
            on_launch_error: launch_error,
        });
        {
            let shared = Rc::downgrade(&shared);
            filter_bar.set_on_changed(move |_| {
                if let Some(shared) = shared.upgrade() {
                    if let Err(error) = render_cache(&shared) {
                        tracing::warn!(%error, "could not apply concerts filter");
                    }
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
                    ConcertsEmptyState::NoCredentials => {
                        shared.on_open_preferences.borrow().clone()
                    }
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

    pub(in crate::ui) fn set_on_open_preferences(&self, callback: impl Fn() + 'static) {
        *self.shared.on_open_preferences.borrow_mut() = Some(Rc::new(callback));
    }

    pub(in crate::ui) fn set_on_launch_error(&self, callback: impl Fn(String) + 'static) {
        *self.shared.on_launch_error.borrow_mut() = Some(Rc::new(callback));
    }

    pub(in crate::ui) fn set_on_refreshed(&self, callback: impl Fn() + 'static) {
        *self.shared.on_refreshed.borrow_mut() = Some(Rc::new(callback));
    }
}

fn render_cache(shared: &Rc<Shared>) -> Result<(), rusqlite::Error> {
    let today = Local::now().date_naive();
    let conn = shared.conn.borrow();
    let filter = shared.filter_bar.filter();
    let location = concerts::config::location(&conn)?;
    let credentials = concerts::config::credentials(&conn)?;
    let similar_enabled = concerts::config::similar_config(&conn)?.enabled;
    let has_similar_rows = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM concert_events WHERE is_similar = 1)",
        [],
        |row| row.get::<_, bool>(0),
    )?;
    let rows = concerts::query_events(&conn, &filter, location.as_ref(), today)?;
    let total = if filter == ConcertFilter::default() {
        rows.len()
    } else {
        concerts::count_upcoming(&conn, &ConcertFilter::default(), location.as_ref(), today)?
            as usize
    };
    let latest_fetch = concerts::latest_fetch_at(&conn)?;
    let never_fetched = latest_fetch.is_none();
    drop(conn);

    shared
        .filter_bar
        .set_context(location.is_some(), similar_enabled, has_similar_rows);
    shared.filter_bar.set_counts(rows.len(), total);
    shared.rows.replace(rows.clone());
    shared.model.replace(rows.clone());
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
    Ok(())
}

fn apply_empty_state(shared: &Shared, state: ConcertsEmptyState, total: usize) {
    shared.empty_state.set(state);
    if state == ConcertsEmptyState::List {
        shared.stack.set_visible_child_name(LIST_PAGE);
        return;
    }

    let (icon, title, description, action) = match state {
        ConcertsEmptyState::NoCredentials => (
            "dialog-password-symbolic",
            strings::text(strings::CONCERTS_API_KEY_TITLE),
            strings::text(strings::CONCERTS_API_KEY_DESCRIPTION),
            strings::text(strings::CONCERTS_OPEN_PREFERENCES),
        ),
        ConcertsEmptyState::NeverFetched => (
            "x-office-calendar-symbolic",
            strings::text(strings::CONCERTS_NO_DATA_TITLE),
            String::new(),
            strings::text(strings::FETCH_NOW),
        ),
        ConcertsEmptyState::NoResults => (
            "system-search-symbolic",
            strings::text(strings::NO_RESULTS_TITLE),
            strings::text(strings::NO_RESULTS_DESCRIPTION),
            strings::show_all_concerts(total),
        ),
        ConcertsEmptyState::Empty => (
            "emblem-ok-symbolic",
            strings::text(strings::CONCERTS_NO_UPCOMING_TITLE),
            String::new(),
            strings::text(strings::FETCH_NOW),
        ),
        ConcertsEmptyState::List => unreachable!("list state returns before status mutation"),
    };
    shared.status.set_icon_name(Some(icon));
    shared.status.set_title(&title);
    shared.status.set_description(Some(&description));
    shared.status_button.set_label(&action);
    shared.stack.set_visible_child_name(STATUS_PAGE);
}

fn build_footer() -> (
    gtk4::Box,
    gtk4::Label,
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
    footer.append(&updated);
    let failure = gtk4::Label::new(Some(&strings::text(strings::CONCERTS_FETCH_FAILED)));
    failure.add_css_class("error");
    failure.add_css_class("caption");
    failure.set_hexpand(true);
    failure.set_halign(gtk4::Align::End);
    failure.set_visible(false);
    footer.append(&failure);
    let fetch_button = gtk4::Button::with_label(&strings::text(strings::FETCH_NOW));
    fetch_button.add_css_class("flat");
    let spinner = gtk4::Spinner::new();
    let fetch_stack = gtk4::Stack::new();
    fetch_stack.set_transition_type(gtk4::StackTransitionType::Crossfade);
    fetch_stack.add_named(&fetch_button, Some(FETCH_BUTTON_PAGE));
    fetch_stack.add_named(&spinner, Some(FETCH_SPINNER_PAGE));
    fetch_stack.set_visible_child_name(FETCH_BUTTON_PAGE);
    footer.append(&fetch_stack);
    (footer, updated, failure, fetch_button, fetch_stack, spinner)
}

fn maybe_background_refresh(shared: &Rc<Shared>) {
    let latest = concerts::latest_fetch_at(&shared.conn.borrow())
        .ok()
        .flatten();
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
        let conn = shared.conn.borrow();
        concerts::config::credentials(&conn).is_ok_and(|credentials| !credentials.is_empty())
    };
    if !has_credentials
        || !request_allowed(shared.runtime.enabled.get(), shared.fetching.get(), true)
    {
        return;
    }
    if shared.fetching.replace(true) {
        return;
    }
    shared.failure.set_visible(false);
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
        finish_fetch(shared, true);
        return;
    }
    let weak = Rc::downgrade(shared);
    gtk4::glib::spawn_future_local(async move {
        let response = receiver.recv().await;
        let Some(shared) = weak.upgrade() else {
            return;
        };
        let failed = match response {
            Ok(ConcertsResponse {
                generation: response_generation,
                result,
            }) if response_generation == shared.generation.get() => match result {
                Ok(summary) => summary.failed > 0,
                Err(error) => {
                    tracing::warn!(%error, "could not refresh Concerts");
                    true
                }
            },
            Ok(_) => return,
            Err(error) => {
                tracing::warn!(%error, "Concerts worker closed without a result");
                true
            }
        };
        finish_fetch(&shared, failed);
    });
}

fn finish_fetch(shared: &Rc<Shared>, failed: bool) {
    shared.fetching.set(false);
    shared.spinner.stop();
    shared.fetch_stack.set_visible_child_name(FETCH_BUTTON_PAGE);
    shared.fetch_button.set_sensitive(true);
    shared.failure.set_visible(failed);
    if let Err(error) = render_cache(shared) {
        tracing::warn!(%error, "could not reload Concerts after fetch");
    }
    let callback = shared.on_refreshed.borrow().clone();
    if let Some(callback) = callback {
        callback();
    }
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
        let conn = Rc::new(RefCell::new(Connection::open_in_memory().unwrap()));
        reprise_core::db::migrate(&conn.borrow()).unwrap();
        let runtime = ConcertsRuntime::setup(&conn.borrow());
        let view = ConcertsView::new(conn, &runtime);
        let root = view.root().clone().downcast::<gtk4::Box>().unwrap();
        let stack = root
            .first_child()
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
    fn conc_5_footer_keeps_fetch_progress_below_the_live_table() {
        gtk4::init().unwrap();
        let conn = Rc::new(RefCell::new(Connection::open_in_memory().unwrap()));
        reprise_core::db::migrate(&conn.borrow()).unwrap();
        let runtime = ConcertsRuntime::setup(&conn.borrow());
        let view = ConcertsView::new(conn, &runtime);
        let root = view.root().clone().downcast::<gtk4::Box>().unwrap();
        let footer = root.last_child().and_downcast::<gtk4::Box>().unwrap();
        let fetch_stack = footer.last_child().and_downcast::<gtk4::Stack>().unwrap();
        assert!(fetch_stack.child_by_name(FETCH_BUTTON_PAGE).is_some());
        assert!(fetch_stack.child_by_name(FETCH_SPINNER_PAGE).is_some());
    }
}
