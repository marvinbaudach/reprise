//! Confirmed, off-thread remove-from-library and move-to-trash workflows.
//! The permanent trash smoke hook refuses anything outside a temporary scan
//! root, so it can never be aimed at the user's music library accidentally.

use std::path::{Path, PathBuf};
use std::rc::Rc;

use gtk4::gdk;
use gtk4::gio;
use gtk4::gio::prelude::*;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;

use crate::ui::track_list::{reload, show_toast, Shared};
use crate::ui::track_list_context_menu::current_selection_positions;
use crate::ui::{one_shot_task, strings};

const ACTION_REMOVE: &str = "remove-selected-from-library";
const ACTION_TRASH: &str = "trash-selected-tracks";
const RESPONSE_CANCEL: &str = "cancel";
const RESPONSE_REMOVE: &str = "remove";
const RESPONSE_TRASH: &str = "trash";
const SMOKE_ENV: &str = "REPRISE_SMOKE_DELETE";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DeleteMode {
    Remove,
    Trash,
}

#[derive(Debug)]
struct DeleteReport {
    removed_ids: Vec<i64>,
    failures: usize,
}

pub(super) fn add_actions(
    group: &gio::SimpleActionGroup,
    column_view: &gtk4::ColumnView,
    shared: &Rc<Shared>,
) {
    for (name, mode) in [
        (ACTION_REMOVE, DeleteMode::Remove),
        (ACTION_TRASH, DeleteMode::Trash),
    ] {
        let action = gio::SimpleAction::new(name, None);
        let shared = shared.clone();
        action.connect_activate(move |_, _| confirm(&shared, mode));
        group.add_action(&action);
    }

    let controller = gtk4::EventControllerKey::new();
    let shared = shared.clone();
    controller.connect_key_pressed(move |_, key, _, _| {
        if key != gdk::Key::Delete {
            return glib::Propagation::Proceed;
        }
        choose(&shared);
        glib::Propagation::Stop
    });
    column_view.add_controller(controller);
}

fn selected_tracks(shared: &Rc<Shared>) -> Option<Vec<(i64, PathBuf)>> {
    let positions = current_selection_positions(shared);
    if positions.is_empty() {
        return None;
    }
    positions
        .into_iter()
        .map(|position| {
            shared
                .model
                .track_at(position)
                .map(|track| (track.id, PathBuf::from(track.path)))
        })
        .collect()
}

fn confirm(shared: &Rc<Shared>, mode: DeleteMode) {
    let Some(tracks) = selected_tracks(shared) else {
        return;
    };
    let Some(window) = shared.window.upgrade() else {
        return;
    };
    let (heading, body, label, response) = match mode {
        DeleteMode::Remove => (
            &strings::text(strings::REMOVE_FROM_LIBRARY),
            strings::remove_confirmation_body(tracks.len()),
            &strings::text(strings::DELETE_TRACKS_REMOVE),
            RESPONSE_REMOVE,
        ),
        DeleteMode::Trash => (
            &strings::text(strings::MOVE_TO_TRASH),
            strings::trash_confirmation_body(tracks.len()),
            &strings::text(strings::DELETE_TRACKS_TRASH),
            RESPONSE_TRASH,
        ),
    };
    let dialog = adw::AlertDialog::builder()
        .heading(heading)
        .body(body)
        .close_response(RESPONSE_CANCEL)
        .build();
    dialog.add_response(
        RESPONSE_CANCEL,
        &strings::text(strings::DELETE_TRACKS_CANCEL),
    );
    dialog.add_response(response, label);
    dialog.set_response_appearance(response, adw::ResponseAppearance::Destructive);
    let focus_guard = crate::ui::transient_focus::TransientFocusGuard::capture(&window);
    let shared = shared.clone();
    dialog.choose(Some(&window), gio::Cancellable::NONE, move |chosen| {
        focus_guard.restore();
        if chosen.as_str() == response {
            start_worker(&shared, tracks, mode);
        }
    });
}

fn choose(shared: &Rc<Shared>) {
    let Some(tracks) = selected_tracks(shared) else {
        return;
    };
    let Some(window) = shared.window.upgrade() else {
        return;
    };
    let dialog = adw::AlertDialog::builder()
        .heading(strings::text(strings::DELETE_TRACKS_HEADING))
        .body(strings::text(strings::DELETE_TRACKS_CHOICE))
        .close_response(RESPONSE_CANCEL)
        .build();
    dialog.add_response(
        RESPONSE_CANCEL,
        &strings::text(strings::DELETE_TRACKS_CANCEL),
    );
    dialog.add_response(
        RESPONSE_REMOVE,
        &strings::text(strings::DELETE_TRACKS_REMOVE),
    );
    dialog.add_response(RESPONSE_TRASH, &strings::text(strings::DELETE_TRACKS_TRASH));
    dialog.set_response_appearance(RESPONSE_REMOVE, adw::ResponseAppearance::Destructive);
    dialog.set_response_appearance(RESPONSE_TRASH, adw::ResponseAppearance::Destructive);
    let focus_guard = crate::ui::transient_focus::TransientFocusGuard::capture(&window);
    let shared = shared.clone();
    dialog.choose(Some(&window), gio::Cancellable::NONE, move |response| {
        focus_guard.restore();
        let mode = match response.as_str() {
            RESPONSE_REMOVE => DeleteMode::Remove,
            RESPONSE_TRASH => DeleteMode::Trash,
            _ => return,
        };
        start_worker(&shared, tracks, mode);
    });
}

fn start_worker(shared: &Rc<Shared>, tracks: Vec<(i64, PathBuf)>, mode: DeleteMode) {
    let db_path = shared.conn.path();
    let Some(db_path) = db_path else {
        show_toast(shared, &strings::text(strings::DELETE_DATABASE_UNAVAILABLE));
        return;
    };
    let receiver = match one_shot_task::spawn("reprise-delete-tracks", move || {
        reprise_core::db::Db::open_migrated(Some(&db_path))
            .map_err(|error| error.to_string())
            .map(|db| run_delete(&db, &tracks, mode))
    }) {
        Ok(receiver) => receiver,
        Err(error) => {
            tracing::warn!(%error, "could not start removal worker");
            show_toast(shared, &strings::text(strings::DELETE_WORKER_FAILED));
            return;
        }
    };
    let shared_weak = Rc::downgrade(shared);
    glib::spawn_future_local(async move {
        let Ok(result) = receiver.recv().await else {
            return;
        };
        let Some(shared) = shared_weak.upgrade() else {
            return;
        };
        match result {
            Ok(report) => finish(&shared, &report, mode),
            Err(error) => {
                tracing::warn!(%error, "removal worker could not open database");
                show_toast(
                    &shared,
                    &strings::text(strings::DELETE_DATABASE_UNAVAILABLE),
                );
            }
        }
    });
}

fn run_delete(
    db: &reprise_core::db::Db,
    tracks: &[(i64, PathBuf)],
    mode: DeleteMode,
) -> DeleteReport {
    match mode {
        DeleteMode::Remove => {
            match reprise_core::queries::exclude_tracks_matching_paths(db, tracks, now_unix()) {
                Ok(removed_ids) => {
                    let failures = tracks.len().saturating_sub(removed_ids.len());
                    DeleteReport {
                        removed_ids,
                        failures,
                    }
                }
                Err(error) => {
                    tracing::error!(%error, "remove-from-library transaction failed");
                    DeleteReport {
                        removed_ids: Vec::new(),
                        failures: tracks.len(),
                    }
                }
            }
        }
        DeleteMode::Trash => {
            let report = reprise_core::library::trash_tracks::trash_tracks_with(
                db,
                tracks,
                reprise_platform_linux::trash::delete,
            );
            for failure in &report.failures {
                tracing::warn!(id = failure.id, path = %failure.path.display(), error = %failure.error, "move-to-trash failed");
            }
            DeleteReport {
                removed_ids: report.removed_ids,
                failures: report.failures.len(),
            }
        }
    }
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs() as i64)
}

fn deletion_focus_position(
    selected_before: &[u32],
    selected_after: &[u32],
    remaining: u32,
) -> Option<u32> {
    selected_after.first().copied().or_else(|| {
        let first_removed = selected_before.first().copied()?;
        (remaining > 0).then(|| first_removed.min(remaining - 1))
    })
}

pub(in crate::ui) fn reload_after_catalog_delete(shared: &Rc<Shared>) {
    let selected_before = current_selection_positions(shared);
    reload(shared);
    if selected_before.is_empty() {
        return;
    }
    let selected_after = current_selection_positions(shared);
    if let Some(position) =
        deletion_focus_position(&selected_before, &selected_after, shared.model.n_items())
    {
        if selected_after.is_empty() {
            shared.selection.select_item(position, true);
        }
    }
    shared.column_view.grab_focus();
}

fn finish(shared: &Rc<Shared>, report: &DeleteReport, mode: DeleteMode) {
    let removed = report.removed_ids.len();
    let callback = shared.on_library_mutated.borrow().clone();
    if let Some(callback) = callback {
        callback(&report.removed_ids);
    }
    shared.browse_bar.refresh();
    reload_after_catalog_delete(shared);
    tracing::info!(
        removed,
        failed = report.failures,
        trashed = mode == DeleteMode::Trash,
        "delete batch completed"
    );
    show_toast(
        shared,
        &strings::delete_result_toast(removed, report.failures, mode == DeleteMode::Trash),
    );
}

pub(super) fn arm_smoke(shared: &Rc<Shared>) {
    let Ok(value) = std::env::var(SMOKE_ENV) else {
        return;
    };
    let mode = match value.as_str() {
        "db-only" => DeleteMode::Remove,
        "trash" => DeleteMode::Trash,
        _ => {
            tracing::warn!(%value, "{SMOKE_ENV} ignored; expected db-only or trash");
            return;
        }
    };
    let shared_weak = Rc::downgrade(shared);
    glib::idle_add_local_once(move || {
        let Some(shared) = shared_weak.upgrade() else {
            return;
        };
        shared
            .selection
            .select_range(0, shared.model.n_items().min(2), true);
        let Some(tracks) = selected_tracks(&shared) else {
            return;
        };
        if mode == DeleteMode::Trash && !safe_scratch_tracks(&tracks) {
            tracing::error!(
                "trash smoke refused: selection is not inside a temporary REPRISE_SCAN_DIR"
            );
            return;
        }
        start_worker(&shared, tracks, mode);
    });
}

fn safe_scratch_tracks(tracks: &[(i64, PathBuf)]) -> bool {
    let Ok(scan_root) = std::env::var("REPRISE_SCAN_DIR") else {
        return false;
    };
    paths_within_temp_root(
        Path::new(&scan_root),
        &tracks
            .iter()
            .map(|(_, path)| path.clone())
            .collect::<Vec<_>>(),
    )
}

fn paths_within_temp_root(root: &Path, paths: &[PathBuf]) -> bool {
    let Ok(root) = std::fs::canonicalize(root) else {
        return false;
    };
    let Ok(temp) = std::fs::canonicalize(std::env::temp_dir()) else {
        return false;
    };
    root.starts_with(temp)
        && paths.iter().all(|path| {
            std::fs::canonicalize(path).is_ok_and(|canonical| canonical.starts_with(&root))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn insert_track(db: &reprise_core::db::Db, id: i64, path: &Path, title: &str) {
        crate::test_db::connection(db)
            .execute(
                "INSERT INTO tracks (id,path,title,artist,added_at) VALUES (?1,?2,?3,'',0)",
                rusqlite::params![id, path.to_string_lossy(), title],
            )
            .unwrap();
    }

    #[test]
    fn smoke_guard_accepts_only_existing_files_inside_temporary_root() {
        let root = tempfile::tempdir().unwrap();
        let inside = root.path().join("inside.flac");
        std::fs::write(&inside, b"scratch").unwrap();
        let outside_root = tempfile::tempdir().unwrap();
        let outside = outside_root.path().join("outside.flac");
        std::fs::write(&outside, b"scratch").unwrap();

        assert!(paths_within_temp_root(
            root.path(),
            std::slice::from_ref(&inside),
        ));
        assert!(!paths_within_temp_root(root.path(), &[inside, outside]));
        assert!(!paths_within_temp_root(
            root.path(),
            &[root.path().join("missing.flac")],
        ));
    }

    #[test]
    fn stale_track_identity_survives_remove_dialog_and_trash_cleanup() {
        let temp = tempfile::tempdir().unwrap();
        let db_path = temp.path().join("delete-race.sqlite");
        let conn = reprise_core::db::Db::open_migrated(Some(&db_path)).unwrap();

        let old_remove_path = temp.path().join("old-remove.flac");
        let replacement_remove_path = temp.path().join("replacement-remove.flac");
        insert_track(&conn, 1, &old_remove_path, "Old remove row");
        let stale_remove = vec![(1, old_remove_path)];
        crate::test_db::connection(&conn)
            .execute("DELETE FROM tracks WHERE id=1", [])
            .unwrap();
        insert_track(&conn, 1, &replacement_remove_path, "Replacement remove row");

        let remove_report = run_delete(&conn, &stale_remove, DeleteMode::Remove);
        let remove_path_after = crate::test_db::connection(&conn)
            .query_row("SELECT path FROM tracks WHERE id=1", [], |row| {
                row.get::<_, String>(0)
            })
            .ok();

        let old_trash_path = temp.path().join("old-trash.flac");
        let replacement_trash_path = temp.path().join("replacement-trash.flac");
        std::fs::write(&old_trash_path, b"scratch").unwrap();
        insert_track(&conn, 2, &old_trash_path, "Old trash row");
        let stale_trash = vec![(2, old_trash_path.clone())];
        let race_conn = reprise_core::db::Db::open_migrated(Some(&db_path)).unwrap();

        let trash_report =
            reprise_core::library::trash_tracks::trash_tracks_with(&conn, &stale_trash, |path| {
                std::fs::remove_file(path).map_err(|error| error.to_string())?;
                crate::test_db::connection(&race_conn)
                    .execute("DELETE FROM tracks WHERE id=2", [])
                    .map_err(|error| error.to_string())?;
                insert_track(
                    &race_conn,
                    2,
                    &replacement_trash_path,
                    "Replacement trash row",
                );
                Ok(())
            });

        let trash_path_after = crate::test_db::connection(&conn)
            .query_row("SELECT path FROM tracks WHERE id=2", [], |row| {
                row.get::<_, String>(0)
            })
            .ok();

        assert!(trash_report.removed_ids.is_empty());
        assert_eq!(
            trash_path_after.as_deref(),
            Some(replacement_trash_path.to_string_lossy().as_ref())
        );
        assert!(remove_report.removed_ids.is_empty());
        assert_eq!(
            remove_path_after.as_deref(),
            Some(replacement_remove_path.to_string_lossy().as_ref())
        );
    }

    #[test]
    fn browse_7_remove_creates_an_exclusion_but_trash_does_not() {
        let temp = tempfile::tempdir().unwrap();
        let remove_path = temp.path().join("remove.flac");
        let trash_path = temp.path().join("trash.flac");
        std::fs::write(&remove_path, b"remove").unwrap();
        std::fs::write(&trash_path, b"trash").unwrap();
        let conn = crate::test_db::open().unwrap();
        insert_track(&conn, 1, &remove_path, "Remove");
        insert_track(&conn, 2, &trash_path, "Trash");

        let removed = run_delete(&conn, &[(1, remove_path.clone())], DeleteMode::Remove);
        let trashed = reprise_core::library::trash_tracks::trash_tracks_with(
            &conn,
            &[(2, trash_path)],
            |_| Ok(()),
        );

        assert_eq!(removed.removed_ids, vec![1]);
        assert_eq!(trashed.removed_ids, vec![2]);
        assert_eq!(reprise_core::library::exclusions::count(&conn).unwrap(), 1);
    }

    #[test]
    fn browse_8_deletion_focus_uses_survivor_then_next_then_previous() {
        assert_eq!(deletion_focus_position(&[4, 5], &[4], 8), Some(4));
        assert_eq!(deletion_focus_position(&[4, 5], &[], 6), Some(4));
        assert_eq!(deletion_focus_position(&[6, 7], &[], 6), Some(5));
        assert_eq!(deletion_focus_position(&[0], &[], 0), None);
    }
}
