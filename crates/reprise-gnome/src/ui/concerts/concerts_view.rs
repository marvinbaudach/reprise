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
use super::concerts_model::{ConcertObject, ConcertsModel};
use super::concerts_presentation::{sort_rows, ConcertSortKey, SortDirection};
use crate::ui::strings;

const LIST_PAGE: &str = "list";
const STATUS_PAGE: &str = "status";

type Callback = Rc<dyn Fn()>;
type ErrorCallback = Rc<dyn Fn(String)>;

struct Shared {
    conn: Rc<RefCell<Connection>>,
    model: Rc<ConcertsModel>,
    rows: RefCell<Vec<ConcertRow>>,
    stack: gtk4::Stack,
    status: adw::StatusPage,
    status_button: gtk4::Button,
    empty_state: Cell<ConcertsEmptyState>,
    on_fetch_now: RefCell<Option<Callback>>,
    on_clear_filters: RefCell<Option<Callback>>,
    on_open_preferences: RefCell<Option<Callback>>,
    on_launch_error: Rc<RefCell<Option<ErrorCallback>>>,
}

pub(in crate::ui) struct ConcertsView {
    root: gtk4::Widget,
    shared: Rc<Shared>,
}

impl ConcertsView {
    pub(in crate::ui) fn new(conn: Rc<RefCell<Connection>>) -> Self {
        let model = Rc::new(ConcertsModel::new());
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

        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        root.add_css_class("reprise-concerts-view");
        root.append(&stack);

        let shared = Rc::new(Shared {
            conn,
            model,
            rows: RefCell::new(Vec::new()),
            stack,
            status,
            status_button: status_button.clone(),
            empty_state: Cell::new(ConcertsEmptyState::NeverFetched),
            on_fetch_now: RefCell::new(None),
            on_clear_filters: RefCell::new(None),
            on_open_preferences: RefCell::new(None),
            on_launch_error: launch_error,
        });

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
        if let Err(error) = refresh(&self.shared) {
            tracing::warn!(%error, "could not load concerts view");
        }
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
}

fn refresh(shared: &Rc<Shared>) -> Result<(), rusqlite::Error> {
    let today = Local::now().date_naive();
    let conn = shared.conn.borrow();
    let filter = concerts::config::persisted_filter(&conn)?;
    let location = concerts::config::location(&conn)?;
    let credentials = concerts::config::credentials(&conn)?;
    let rows = concerts::query_events(&conn, &filter, location.as_ref(), today)?;
    let total = if filter == ConcertFilter::default() {
        rows.len()
    } else {
        concerts::count_upcoming(&conn, &ConcertFilter::default(), location.as_ref(), today)?
            as usize
    };
    let never_fetched = concerts::latest_fetch_at(&conn)?.is_none();
    drop(conn);

    shared.rows.replace(rows.clone());
    shared.model.replace(rows.clone());
    let state = concerts_empty_state_for(
        rows.len(),
        filter != ConcertFilter::default(),
        !credentials.is_empty(),
        never_fetched,
    );
    apply_empty_state(shared, state, total);
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
        let conn = Rc::new(RefCell::new(Connection::open_in_memory().unwrap()));
        reprise_core::db::migrate(&conn.borrow()).unwrap();
        let view = ConcertsView::new(conn);
        let root = view.root().clone().downcast::<gtk4::Box>().unwrap();
        let stack = root.first_child().and_downcast::<gtk4::Stack>().unwrap();
        let scrolled = stack
            .child_by_name(LIST_PAGE)
            .and_downcast::<gtk4::ScrolledWindow>()
            .unwrap();
        let table = scrolled.child().and_downcast::<gtk4::ColumnView>().unwrap();
        assert_eq!(table.columns().n_items(), 6);
        assert!(!table.enables_rubberband());
    }
}
