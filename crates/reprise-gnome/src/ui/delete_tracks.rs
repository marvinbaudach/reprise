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

use crate::ui::strings;
use crate::ui::track_list::{reload, show_toast, Shared};
use crate::ui::track_list_context_menu::current_selection_positions;

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

pub(super) fn append_menu_section(menu: &gio::Menu, action_group: &str) {
    let section = gio::Menu::new();
    section.append(
        Some(&strings::text(strings::REMOVE_FROM_LIBRARY)),
        Some(&format!("{action_group}.{ACTION_REMOVE}")),
    );
    section.append(
        Some(&strings::text(strings::MOVE_TO_TRASH)),
        Some(&format!("{action_group}.{ACTION_TRASH}")),
    );
    menu.append_section(None, &section);
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
    let shared = shared.clone();
    dialog.choose(Some(&window), gio::Cancellable::NONE, move |chosen| {
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
    let shared = shared.clone();
    dialog.choose(Some(&window), gio::Cancellable::NONE, move |response| {
        let mode = match response.as_str() {
            RESPONSE_REMOVE => DeleteMode::Remove,
            RESPONSE_TRASH => DeleteMode::Trash,
            _ => return,
        };
        start_worker(&shared, tracks, mode);
    });
}

fn start_worker(shared: &Rc<Shared>, tracks: Vec<(i64, PathBuf)>, mode: DeleteMode) {
    let db_path = shared.conn.borrow().path().map(PathBuf::from);
    let Some(db_path) = db_path else {
        show_toast(shared, &strings::text(strings::DELETE_DATABASE_UNAVAILABLE));
        return;
    };
    let (sender, receiver) = async_channel::bounded(1);
    let spawned = std::thread::Builder::new()
        .name("reprise-delete-tracks".into())
        .spawn(move || {
            let result = reprise_core::db::open(Some(&db_path))
                .map_err(|error| error.to_string())
                .map(|mut conn| run_delete(&mut conn, &tracks, mode));
            let _ = sender.try_send(result);
        });
    if let Err(error) = spawned {
        tracing::warn!(%error, "could not start removal worker");
        show_toast(shared, &strings::text(strings::DELETE_WORKER_FAILED));
        return;
    }
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
    conn: &mut rusqlite::Connection,
    tracks: &[(i64, PathBuf)],
    mode: DeleteMode,
) -> DeleteReport {
    match mode {
        DeleteMode::Remove => {
            let ids: Vec<_> = tracks.iter().map(|(id, _)| *id).collect();
            match reprise_core::queries::remove_tracks(conn, &ids) {
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
            let report = reprise_core::library::trash_tracks::trash_tracks(conn, tracks);
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

fn finish(shared: &Rc<Shared>, report: &DeleteReport, mode: DeleteMode) {
    let removed = report.removed_ids.len();
    let callback = shared.on_library_mutated.borrow().clone();
    if let Some(callback) = callback {
        callback(&report.removed_ids);
    }
    shared.browse_bar.refresh();
    reload(shared);
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
}
