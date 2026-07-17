//! Context-menu cleanup for the sidebar's transient issue sources.
//!
//! Import errors are diagnostics and are dismissed with a current file-stat
//! fingerprint. Missing tracks are persistent library rows, so bulk removal
//! is confirmed explicitly and routed through the shared tombstone/Undo
//! service. The established vanished-source fallback selects Music when the
//! now-empty Missing row disappears.

use std::rc::Rc;

use gtk4::gio;
use gtk4::gio::prelude::*;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::queries;
use reprise_core::view_source::ViewSource;

use crate::ui::popover_lifecycle;
use crate::ui::sidebar::{rebuild, show_toast, OnRemoveMissing, Shared};
use crate::ui::sidebar_issue_strings as copy;
use crate::ui::strings;

const ACTION_DISMISS_ALL: &str = "dismiss-all";
const ACTION_REMOVE_ALL: &str = "remove-all";
const ACTION_GROUP_NAME: &str = "issuerow";
const RESPONSE_CANCEL: &str = "cancel";
const RESPONSE_REMOVE: &str = "remove";

struct IssueMenuSpec {
    action: &'static str,
    label: String,
    needs_confirmation: bool,
}

fn issue_menu_spec(source: &ViewSource) -> Option<IssueMenuSpec> {
    match source {
        ViewSource::ImportErrors => Some(IssueMenuSpec {
            action: ACTION_DISMISS_ALL,
            label: copy::text(copy::DISMISS_ALL_IMPORT_ERRORS),
            needs_confirmation: false,
        }),
        ViewSource::Missing => Some(IssueMenuSpec {
            action: ACTION_REMOVE_ALL,
            label: copy::text(copy::REMOVE_ALL_MISSING_ENTRIES),
            needs_confirmation: true,
        }),
        _ => None,
    }
}

pub(in crate::ui) fn wire_issue_context_menu(
    shared: &Rc<Shared>,
    row: &gtk4::ListBoxRow,
    source: ViewSource,
) {
    debug_assert!(issue_menu_spec(&source).is_some());
    let gesture = gtk4::GestureClick::new();
    gesture.set_button(gtk4::gdk::BUTTON_SECONDARY);

    let shared = shared.clone();
    let row_for_popover = row.clone();
    gesture.connect_pressed(move |gesture, _n_press, x, y| {
        gesture.set_state(gtk4::EventSequenceState::Claimed);
        show_context_menu(&shared, &row_for_popover, &source, x, y);
    });
    row.add_controller(gesture);
}

fn show_context_menu(
    shared: &Rc<Shared>,
    row: &gtk4::ListBoxRow,
    source: &ViewSource,
    x: f64,
    y: f64,
) {
    let Some(spec) = issue_menu_spec(source) else {
        return;
    };
    let action_group = gio::SimpleActionGroup::new();
    let action = gio::SimpleAction::new(spec.action, None);
    let shared_for_action = shared.clone();
    let needs_confirmation = spec.needs_confirmation;
    action.connect_activate(move |_, _| {
        if needs_confirmation {
            confirm_remove_all_missing(&shared_for_action);
        } else {
            dismiss_all_import_errors(&shared_for_action);
        }
    });
    action_group.add_action(&action);
    row.insert_action_group(ACTION_GROUP_NAME, Some(&action_group));

    let menu = gio::Menu::new();
    menu.append(
        Some(&spec.label),
        Some(&format!("{ACTION_GROUP_NAME}.{}", spec.action)),
    );
    let popover = gtk4::PopoverMenu::from_model(Some(&menu));
    popover.set_parent(row);
    popover.set_has_arrow(false);
    popover.set_pointing_to(Some(&gtk4::gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
    popover_lifecycle::unparent_after_actions(popover.upcast_ref());
    popover.popup();
}

fn dismiss_all_import_errors(shared: &Rc<Shared>) {
    let result = {
        let conn = shared.conn.borrow();
        dismiss_import_errors_with_stat(&conn, &crate::ui::import_errors_view::file_stat)
    };
    match result {
        Ok(dismissed) => {
            tracing::info!(dismissed, "all import errors dismissed");
            rebuild(shared, None, "all import errors dismissed");
            show_toast(
                shared,
                &copy::import_errors_dismissed(dismissed.try_into().unwrap_or(usize::MAX)),
            );
        }
        Err(error) => {
            tracing::error!(%error, "failed to dismiss all import errors");
            show_toast(shared, &copy::text(copy::IMPORT_ERRORS_DISMISS_FAILED));
        }
    }
}

fn dismiss_import_errors_with_stat(
    conn: &rusqlite::Connection,
    stat: &dyn Fn(&str) -> Option<(i64, i64)>,
) -> Result<u32, rusqlite::Error> {
    queries::dismiss_all_import_errors(conn, stat)
}

fn confirm_remove_all_missing(shared: &Rc<Shared>) {
    let ids = {
        let conn = shared.conn.borrow();
        match missing_ids_for_cleanup(&conn) {
            Ok(ids) => ids,
            Err(error) => {
                tracing::error!(%error, "failed to load missing ids for bulk removal");
                show_toast(shared, &copy::text(copy::MISSING_ENTRIES_REMOVE_FAILED));
                return;
            }
        }
    };
    if ids.is_empty() {
        rebuild(shared, None, "missing cleanup found no rows");
        return;
    }
    let Some(window) = shared.window.upgrade() else {
        tracing::warn!("sidebar window is gone; cannot confirm missing-entry cleanup");
        return;
    };
    let dialog = adw::AlertDialog::builder()
        .heading(copy::text(copy::REMOVE_MISSING_HEADING))
        .body(strings::missing_remove_body(ids.len()))
        .close_response(RESPONSE_CANCEL)
        .build();
    dialog.add_response(RESPONSE_CANCEL, &copy::text(copy::CANCEL));
    dialog.add_response(RESPONSE_REMOVE, &copy::text(copy::REMOVE));
    dialog.set_response_appearance(RESPONSE_REMOVE, adw::ResponseAppearance::Destructive);

    let shared = shared.clone();
    dialog.choose(Some(&window), gio::Cancellable::NONE, move |response| {
        if response.as_str() == RESPONSE_REMOVE {
            remove_all_missing(&shared, &ids);
        }
    });
}

fn missing_ids_for_cleanup(conn: &rusqlite::Connection) -> Result<Vec<i64>, rusqlite::Error> {
    queries::query_track_ids(conn, &ViewSource::Missing, "title", "asc", "", &[])
}

fn dispatch_missing_cleanup(callback: Option<OnRemoveMissing>, ids: &[i64]) -> bool {
    let Some(callback) = callback else {
        return false;
    };
    callback(ids);
    true
}

fn remove_all_missing(shared: &Rc<Shared>, ids: &[i64]) {
    let callback = shared.on_remove_missing.borrow().clone();
    if dispatch_missing_cleanup(callback, ids) {
        tracing::info!(
            tombstoned = ids.len(),
            "all missing library entries tombstoned"
        );
    } else {
        tracing::error!("missing tombstone route is not wired");
        show_toast(shared, &copy::text(copy::MISSING_ENTRIES_REMOVE_FAILED));
    }
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};

    use super::*;
    use crate::ui::sidebar::{find_row, Sidebar};

    #[test]
    fn only_issue_sources_offer_their_matching_bulk_cleanup_action() {
        let import_errors = issue_menu_spec(&ViewSource::ImportErrors).unwrap();
        assert_eq!(import_errors.action, ACTION_DISMISS_ALL);
        assert!(!import_errors.needs_confirmation);

        let missing = issue_menu_spec(&ViewSource::Missing).unwrap();
        assert_eq!(missing.action, ACTION_REMOVE_ALL);
        assert!(missing.needs_confirmation);

        assert!(issue_menu_spec(&ViewSource::Library).is_none());
        assert!(issue_menu_spec(&ViewSource::Queue).is_none());
    }

    #[test]
    fn bulk_cleanup_routes_keep_issue_rows_reversible() {
        let conn = reprise_core::db::open_migrated(None).unwrap();
        for path in ["/x/stat-ok.flac", "/x/stat-fails.flac"] {
            conn.execute(
                "INSERT INTO import_errors \
                 (path,reason_kind,reason_detail,first_seen,last_seen) \
                 VALUES (?1,'io','broken',1,1)",
                [path],
            )
            .unwrap();
        }
        let dismissed = dismiss_import_errors_with_stat(&conn, &|path| {
            (path == "/x/stat-ok.flac").then_some((11, 22))
        })
        .unwrap();
        assert_eq!(dismissed, 1);
        assert_eq!(queries::query_import_error_count(&conn).unwrap(), 2);
        assert_eq!(queries::count_import_errors_active(&conn).unwrap(), 1);
        assert_eq!(queries::count_dismissed_import_errors(&conn).unwrap(), 1);

        conn.execute(
            "INSERT INTO tracks \
             (id,path,title,artist,added_at,missing_since,missing_reason) \
             VALUES (7,'/x/gone.flac','Gone','',0,1,'deleted')",
            [],
        )
        .unwrap();
        let ids = missing_ids_for_cleanup(&conn).unwrap();
        assert_eq!(ids, vec![7]);

        let conn = Rc::new(RefCell::new(conn));
        let invoked = Rc::new(Cell::new(false));
        let route: OnRemoveMissing = {
            let conn = conn.clone();
            let invoked = invoked.clone();
            Rc::new(move |ids| {
                invoked.set(true);
                queries::tombstone_tracks(&conn.borrow(), ids, 100).unwrap();
            })
        };
        assert!(dispatch_missing_cleanup(Some(route), &ids));
        assert!(invoked.get());
        assert_eq!(queries::count_missing(&conn.borrow()).unwrap(), 0);
        let removed_at: Option<i64> = conn
            .borrow()
            .query_row("SELECT removed_at FROM tracks WHERE id=7", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(removed_at, Some(100), "the row is retained for Undo");
        queries::undo_tombstone(&conn.borrow(), &ids).unwrap();
        assert_eq!(queries::count_missing(&conn.borrow()).unwrap(), 1);
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn issue_rows_install_context_gestures_and_missing_cleanup_falls_back() {
        gtk4::init().unwrap();
        let conn = reprise_core::db::open(None).unwrap();
        reprise_core::db::migrate(&conn).unwrap();
        let bad = tempfile::NamedTempFile::new().unwrap();
        let bad_path = bad.path().to_string_lossy().to_string();
        conn.execute(
            "INSERT INTO import_errors \
             (path, reason_kind, reason_detail, first_seen, last_seen) \
             VALUES (?1, 'io', 'bad tag', 1, 1)",
            [&bad_path],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO tracks (id,path,title,artist,added_at,missing_since) \
             VALUES (7,'/x/gone.flac','Gone','',0,1)",
            [],
        )
        .unwrap();
        let conn = Rc::new(RefCell::new(conn));
        let window = adw::ApplicationWindow::builder().build();
        let sidebar = Sidebar::new(conn.clone(), &window, || 0);
        let removed = Rc::new(RefCell::new(Vec::new()));
        let removed_for_callback = removed.clone();
        let conn_for_callback = conn.clone();
        sidebar.set_on_remove_missing(move |ids| {
            removed_for_callback.borrow_mut().extend_from_slice(ids);
            queries::tombstone_tracks(&conn_for_callback.borrow(), ids, 100).unwrap();
        });

        for source in [ViewSource::ImportErrors, ViewSource::Missing] {
            let row = find_row(sidebar.test_shared(), &source).unwrap();
            let controllers = row.observe_controllers();
            assert!((0..controllers.n_items()).any(|index| {
                controllers.item(index).is_some_and(|controller| {
                    controller
                        .downcast_ref::<gtk4::GestureClick>()
                        .is_some_and(|gesture| gesture.button() == gtk4::gdk::BUTTON_SECONDARY)
                })
            }));
        }

        dismiss_all_import_errors(sidebar.test_shared());
        assert_eq!(
            queries::query_import_error_count(&conn.borrow()).unwrap(),
            1
        );
        assert_eq!(
            queries::count_import_errors_active(&conn.borrow()).unwrap(),
            0
        );
        assert_eq!(
            queries::count_dismissed_import_errors(&conn.borrow()).unwrap(),
            1
        );
        assert!(find_row(sidebar.test_shared(), &ViewSource::ImportErrors).is_some());

        sidebar.refresh_and_select(ViewSource::Missing, "test missing cleanup");
        let ids = missing_ids_for_cleanup(&conn.borrow()).unwrap();
        remove_all_missing(sidebar.test_shared(), &ids);
        sidebar.refresh("test tombstone refresh");
        assert_eq!(*removed.borrow(), vec![7]);
        assert!(find_row(sidebar.test_shared(), &ViewSource::Missing).is_none());
        assert_eq!(
            *sidebar.test_shared().current_source.borrow(),
            ViewSource::Library
        );
    }
}
