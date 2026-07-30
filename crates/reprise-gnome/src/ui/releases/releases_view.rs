//! Releases full-view table, filters, status surface, and refresh footer.

#![allow(dead_code)]

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use chrono::Local;
use gtk4::prelude::*;
use libadwaita as adw;
use reprise_core::artist_news::{self, ReleaseSortDirection, ReleasesFilter};
use reprise_core::artist_news_history::HistoryEntry;
use reprise_core::db::Db;

use super::releases_columns;
use super::releases_empty_state::{releases_empty_state_for, ReleasesEmptyState};
use super::releases_filter_bar::ReleasesFilterBar;
use super::releases_model::{ReleaseObject, ReleasesModel};
use super::releases_presentation::{releases_row_action, ReleasesRowAction};
use crate::ui::external_link::{self, LaunchErrorSlot};
use crate::ui::{one_shot_task, strings};

const LIST_PAGE: &str = "list";
const STATUS_PAGE: &str = "status";
const FETCH_ICON_PAGE: &str = "icon";
const FETCH_SPINNER_PAGE: &str = "spinner";

type Callback = Rc<dyn Fn()>;

struct Shared {
    conn: Rc<Db>,
    database_path: PathBuf,
    model: Rc<ReleasesModel>,
    filter_bar: Rc<ReleasesFilterBar>,
    rows: RefCell<Vec<HistoryEntry>>,
    stack: gtk4::Stack,
    status: adw::StatusPage,
    status_button: gtk4::Button,
    fetch_button: gtk4::Button,
    fetch_stack: gtk4::Stack,
    spinner: gtk4::Spinner,
    updated: gtk4::Label,
    failure: gtk4::Label,
    fetching: Cell<bool>,
    empty_state: Cell<ReleasesEmptyState>,
    on_launch_error: LaunchErrorSlot,
    on_refreshed: RefCell<Option<Callback>>,
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
        let date_column = releases_columns::append_columns(&column_view, &on_set_hidden, &on_open);
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

        let (footer, fetch_button, fetch_stack, spinner, updated, failure) = build_footer();
        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        root.add_css_class("reprise-releases-view");
        root.append(filter_bar.widget());
        root.append(&stack);
        root.append(&footer);

        let shared = Rc::new(Shared {
            conn,
            database_path,
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
            empty_state: Cell::new(ReleasesEmptyState::NeverFetched),
            on_launch_error: Rc::new(RefCell::new(None)),
            on_refreshed: RefCell::new(None),
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
                ReleasesEmptyState::NoResults => shared.filter_bar.clear_all(),
                ReleasesEmptyState::NeverFetched | ReleasesEmptyState::Empty => {
                    request_fetch(&shared);
                }
                ReleasesEmptyState::List => {}
            });
        }
        {
            let shared = shared.clone();
            fetch_button.connect_clicked(move |_| request_fetch(&shared));
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

    pub(in crate::ui) fn refresh(&self) {
        if let Err(error) = render_cache(&self.shared) {
            tracing::warn!(%error, "could not load Releases view");
        }
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
    let filter = shared.filter_bar.filter();
    let rows = artist_news::query_releases_view(&shared.conn, &filter, today)?;
    let total = if filter == ReleasesFilter::default() {
        rows.len()
    } else {
        artist_news::count_releases_view(&shared.conn, &ReleasesFilter::default(), today)? as usize
    };
    let latest = artist_news::latest_fetched_at(&shared.conn)?;
    shared.filter_bar.set_counts(rows.len(), total);
    shared.rows.replace(rows.clone());
    shared.model.replace(rows.clone());
    apply_empty_state(
        shared,
        releases_empty_state_for(
            rows.len(),
            filter != ReleasesFilter::default(),
            latest.is_none(),
        ),
        total,
    );
    shared
        .updated
        .set_label(&latest.map_or_else(String::new, |timestamp| {
            strings::new_releases_updated_ago(timestamp, chrono::Utc::now().timestamp())
        }));
    shared.updated.set_visible(latest.is_some());
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

fn build_footer() -> (
    gtk4::Box,
    gtk4::Button,
    gtk4::Stack,
    gtk4::Spinner,
    gtk4::Label,
    gtk4::Label,
) {
    let footer = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    for (property, value) in [
        ("margin-top", 6),
        ("margin-bottom", 6),
        ("margin-start", 12),
        ("margin-end", 12),
    ] {
        footer.set_property(property, value);
    }
    let updated = gtk4::Label::new(None);
    updated.add_css_class("dim-label");
    updated.add_css_class("caption");
    updated.set_hexpand(true);
    footer.append(&updated);
    let failure = gtk4::Label::new(Some(&strings::text(strings::FETCH_FAILED_INLINE)));
    failure.add_css_class("error");
    failure.add_css_class("caption");
    failure.set_visible(false);
    footer.append(&failure);
    let icon = gtk4::Image::from_icon_name("view-refresh-symbolic");
    let spinner = gtk4::Spinner::new();
    let fetch_stack = gtk4::Stack::new();
    fetch_stack.add_named(&icon, Some(FETCH_ICON_PAGE));
    fetch_stack.add_named(&spinner, Some(FETCH_SPINNER_PAGE));
    fetch_stack.set_visible_child_name(FETCH_ICON_PAGE);
    let label = gtk4::Label::new(Some(&strings::text(strings::FETCH_NOW)));
    let content = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    content.append(&fetch_stack);
    content.append(&label);
    let fetch_button = gtk4::Button::builder()
        .child(&content)
        .css_classes(["flat", "new-release-ghost"])
        .build();
    footer.append(&fetch_button);
    (footer, fetch_button, fetch_stack, spinner, updated, failure)
}

fn request_fetch(shared: &Rc<Shared>) {
    if shared.fetching.replace(true) {
        return;
    }
    shared.failure.set_visible(false);
    shared.fetch_button.set_sensitive(false);
    shared
        .fetch_stack
        .set_visible_child_name(FETCH_SPINNER_PAGE);
    shared.spinner.start();
    let path = shared.database_path.clone();
    let result = one_shot_task::spawn("reprise-releases", move || fetch_from_database(&path));
    let Ok(receiver) = result else {
        finish_fetch(shared, true);
        return;
    };
    let weak = Rc::downgrade(shared);
    gtk4::glib::spawn_future_local(async move {
        let failed = match receiver.recv().await {
            Ok(Ok(report)) => report.failed > 0,
            Ok(Err(error)) => {
                tracing::warn!(%error, "could not refresh Releases");
                true
            }
            Err(error) => {
                tracing::warn!(%error, "Releases worker closed without a result");
                true
            }
        };
        if let Some(shared) = weak.upgrade() {
            finish_fetch(&shared, failed);
        }
    });
}

fn fetch_from_database(path: &Path) -> Result<artist_news::RefreshReport, artist_news::NewsError> {
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
    artist_news::refresh(
        &conn,
        today,
        scope,
        true,
        crate::ui::updates::release_cover::fallback_accent_for_artist,
    )
}

fn finish_fetch(shared: &Rc<Shared>, failed: bool) {
    if !failed {
        if let Err(error) =
            reprise_core::library::settings::set_new_releases_fetch_completed(&shared.conn, true)
        {
            tracing::warn!(%error, "could not persist Releases fetch completion");
        }
    }
    shared.fetching.set(false);
    shared.spinner.stop();
    shared.fetch_stack.set_visible_child_name(FETCH_ICON_PAGE);
    shared.fetch_button.set_sensitive(true);
    shared.failure.set_visible(failed);
    if let Err(error) = render_cache(shared) {
        tracing::warn!(%error, "could not reload Releases after fetch");
    }
    notify_refreshed(shared);
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
        shared
            .model
            .replace(artist_news::sort_release_rows(rows, direction));
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn nr_20_releases_view_exposes_filters_six_columns_and_footer() {
        gtk4::init().unwrap();
        let conn = Rc::new(crate::test_db::open().unwrap());
        let view = ReleasesView::new(conn, PathBuf::new());
        let root = view.root().clone().downcast::<gtk4::Box>().unwrap();
        assert_eq!(root.observe_children().n_items(), 3);
        let stack = root
            .first_child()
            .and_then(|child| child.next_sibling())
            .and_downcast::<gtk4::Stack>()
            .unwrap();
        let table = stack
            .child_by_name(LIST_PAGE)
            .and_downcast::<gtk4::ScrolledWindow>()
            .and_then(|scrolled| scrolled.child())
            .and_downcast::<gtk4::ColumnView>()
            .unwrap();
        assert_eq!(table.columns().n_items(), 6);
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
