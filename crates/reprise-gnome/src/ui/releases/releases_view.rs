//! Releases full-view table and status surface.

#![allow(dead_code)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use chrono::Local;
use gtk4::gio;
use gtk4::prelude::*;
use libadwaita as adw;
use reprise_core::artist_news::{self, ReleaseSortDirection, ReleasesFilter};
use reprise_core::artist_news_history::HistoryEntry;
use rusqlite::Connection;

use super::releases_columns;
use super::releases_empty_state::{releases_empty_state_for, ReleasesEmptyState};
use super::releases_model::{ReleaseObject, ReleasesModel};
use super::releases_presentation::{releases_row_action, ReleasesRowAction};
use super::OnShowAlbum;
use crate::ui::strings;

const LIST_PAGE: &str = "list";
const STATUS_PAGE: &str = "status";

type Callback = Rc<dyn Fn()>;
type ErrorCallback = Rc<dyn Fn(String)>;

struct Shared {
    conn: Rc<RefCell<Connection>>,
    model: Rc<ReleasesModel>,
    rows: RefCell<Vec<HistoryEntry>>,
    stack: gtk4::Stack,
    status: adw::StatusPage,
    status_button: gtk4::Button,
    empty_state: Cell<ReleasesEmptyState>,
    on_show_album: OnShowAlbum,
    on_fetch_now: RefCell<Option<Callback>>,
    on_launch_error: Rc<RefCell<Option<ErrorCallback>>>,
    on_refreshed: RefCell<Option<Callback>>,
}

pub(in crate::ui) struct ReleasesView {
    root: gtk4::Widget,
    shared: Rc<Shared>,
}

impl ReleasesView {
    pub(in crate::ui) fn new(conn: Rc<RefCell<Connection>>, on_show_album: OnShowAlbum) -> Self {
        let model = Rc::new(ReleasesModel::new());
        let column_view = gtk4::ColumnView::builder()
            .model(model.selection())
            .show_row_separators(false)
            .show_column_separators(false)
            .build();
        column_view.add_css_class("reprise-releases-table");
        let date_column = releases_columns::append_columns(&column_view);

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
        root.add_css_class("reprise-releases-view");
        root.append(&stack);

        let shared = Rc::new(Shared {
            conn,
            model,
            rows: RefCell::new(Vec::new()),
            stack,
            status,
            status_button: status_button.clone(),
            empty_state: Cell::new(ReleasesEmptyState::NeverFetched),
            on_show_album,
            on_fetch_now: RefCell::new(None),
            on_launch_error: Rc::new(RefCell::new(None)),
            on_refreshed: RefCell::new(None),
        });
        {
            let shared = shared.clone();
            status_button.connect_clicked(move |_| {
                if matches!(
                    shared.empty_state.get(),
                    ReleasesEmptyState::NeverFetched | ReleasesEmptyState::Empty
                ) {
                    let callback = shared.on_fetch_now.borrow().clone();
                    if let Some(callback) = callback {
                        callback();
                    }
                }
            });
        }
        {
            let shared = shared.clone();
            column_view.connect_activate(move |_, position| {
                activate_position(&shared, position);
            });
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

    pub(in crate::ui) fn refresh(&self) {
        if let Err(error) = render_cache(&self.shared) {
            tracing::warn!(%error, "could not load Releases view");
        }
    }

    pub(in crate::ui) fn set_on_fetch_now(&self, callback: impl Fn() + 'static) {
        *self.shared.on_fetch_now.borrow_mut() = Some(Rc::new(callback));
    }

    pub(in crate::ui) fn set_on_launch_error(&self, callback: impl Fn(String) + 'static) {
        *self.shared.on_launch_error.borrow_mut() = Some(Rc::new(callback));
    }

    pub(in crate::ui) fn set_on_refreshed(&self, callback: impl Fn() + 'static) {
        *self.shared.on_refreshed.borrow_mut() = Some(Rc::new(callback));
    }
}

fn render_cache(shared: &Shared) -> Result<(), rusqlite::Error> {
    let today = Local::now().date_naive();
    let filter = ReleasesFilter::default();
    let rows = artist_news::query_releases_view(&shared.conn.borrow(), &filter, today)?;
    let never_fetched = artist_news::latest_fetched_at(&shared.conn.borrow())?.is_none();
    shared.rows.replace(rows.clone());
    shared.model.replace(rows.clone());
    apply_empty_state(
        shared,
        releases_empty_state_for(rows.len(), false, never_fetched),
        rows.len(),
    );
    Ok(())
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
            "emblem-ok-symbolic",
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
        ReleasesRowAction::Restore => {
            if let Err(error) = reprise_core::artist_news_history::restore_release(
                &shared.conn.borrow(),
                &entry.release_group_mbid,
            ) {
                tracing::warn!(%error, "could not restore release");
                return;
            }
            if let Err(error) = render_cache(shared) {
                tracing::warn!(%error, "could not reload Releases after restore");
            }
            let callback = shared.on_refreshed.borrow().clone();
            if let Some(callback) = callback {
                callback();
            }
        }
        ReleasesRowAction::ShowInLibrary => {
            (shared.on_show_album)(&entry.title, &entry.artist_name);
        }
        ReleasesRowAction::OpenAnnouncement(url) => {
            let launch_error = shared.on_launch_error.clone();
            gtk4::UriLauncher::new(&url).launch(
                None::<&gtk4::Window>,
                gio::Cancellable::NONE,
                move |result| {
                    if let Err(error) = result {
                        tracing::warn!(%error, "could not open release announcement");
                        if let Some(callback) = launch_error.borrow().as_ref() {
                            callback(error.to_string());
                        }
                    }
                },
            );
        }
    }
}

fn wire_sorting(column_view: &gtk4::ColumnView, shared: &Rc<Shared>) {
    let Some(sorter) = column_view
        .sorter()
        .and_downcast::<gtk4::ColumnViewSorter>()
    else {
        tracing::warn!("Releases table has no ColumnViewSorter");
        return;
    };
    {
        let shared = shared.clone();
        sorter.connect_primary_sort_order_notify(move |sorter| {
            let direction = if sorter.primary_sort_order() == gtk4::SortType::Ascending {
                ReleaseSortDirection::Ascending
            } else {
                ReleaseSortDirection::Descending
            };
            let rows = shared.rows.borrow().clone();
            shared
                .model
                .replace(artist_news::sort_release_rows(rows, direction));
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn nr_14_releases_view_exposes_five_columns_and_date_sorting() {
        let conn = Rc::new(RefCell::new(Connection::open_in_memory().unwrap()));
        reprise_core::db::migrate(&conn.borrow()).unwrap();
        let view = ReleasesView::new(conn, Rc::new(|_, _| {}));
        let root = view.root().clone().downcast::<gtk4::Box>().unwrap();
        let stack = root.first_child().and_downcast::<gtk4::Stack>().unwrap();
        let scrolled = stack
            .child_by_name(LIST_PAGE)
            .and_downcast::<gtk4::ScrolledWindow>()
            .unwrap();
        let table = scrolled.child().and_downcast::<gtk4::ColumnView>().unwrap();
        assert_eq!(table.columns().n_items(), 5);
        assert_eq!(
            table
                .columns()
                .item(0)
                .and_downcast::<gtk4::ColumnViewColumn>()
                .unwrap()
                .id()
                .as_deref(),
            Some("date")
        );
    }
}
