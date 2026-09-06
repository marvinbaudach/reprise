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
use reprise_core::library::path_guard;

use crate::ui::track_list::reload_restore::ReloadAnchor;
use crate::ui::track_list::track_list_model_change::{changed_range, ModelChange};
use crate::ui::track_list::track_list_reload::{
    capture_reload_anchor, reload_with_anchor_and_viewport, ReloadViewport,
};
use crate::ui::track_list::{show_toast, Shared};
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

pub(in crate::ui) struct CatalogDeleteReloadState {
    anchor: ReloadAnchor,
    view_ids: Vec<i64>,
    selected_positions: Vec<u32>,
    generation: u64,
}

pub(in crate::ui) fn capture_catalog_delete_reload(
    shared: &Rc<Shared>,
) -> CatalogDeleteReloadState {
    CatalogDeleteReloadState {
        anchor: capture_reload_anchor(shared),
        view_ids: shared.current_view_ids(),
        selected_positions: current_selection_positions(shared),
        generation: shared.model.generation(),
    }
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
    let reload_state = capture_catalog_delete_reload(shared);
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
            start_worker(&shared, tracks, mode, reload_state);
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
    let reload_state = capture_catalog_delete_reload(shared);
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
        start_worker(&shared, tracks, mode, reload_state);
    });
}

fn start_worker(
    shared: &Rc<Shared>,
    tracks: Vec<(i64, PathBuf)>,
    mode: DeleteMode,
    reload_state: CatalogDeleteReloadState,
) {
    let worker_started = std::time::Instant::now();
    tracing::info!(
        tracks = tracks.len(),
        mode = ?mode,
        "delete confirmed"
    );
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
            Ok(report) => finish(
                &shared,
                &report,
                mode,
                reload_state,
                worker_started.elapsed().as_millis(),
            ),
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
            let report = match reprise_platform_linux::trash::Session::open() {
                Ok(session) => {
                    reprise_core::library::trash_tracks::trash_tracks_with(db, tracks, |path| {
                        session.delete(path)
                    })
                }
                Err(error) => {
                    reprise_core::library::trash_tracks::trash_tracks_with(db, tracks, |_| {
                        Err(error.clone())
                    })
                }
            };
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

fn deletion_model_change(
    before_ids: &[i64],
    after_ids: &[i64],
    removed_ids: &[i64],
    generation: u64,
) -> Option<ModelChange> {
    changed_range(before_ids, after_ids, removed_ids, generation)
}

fn surviving_delete_anchor(
    mut anchor: ReloadAnchor,
    before_ids: &[i64],
    after_ids: &[i64],
) -> ReloadAnchor {
    let Some((anchor_id, offset)) = anchor.anchor else {
        return anchor;
    };
    if after_ids.contains(&anchor_id) {
        return anchor;
    }
    anchor.anchor = before_ids
        .iter()
        .position(|id| *id == anchor_id)
        .and_then(|position| after_ids.get(position.min(after_ids.len().saturating_sub(1))))
        .map(|id| (*id, offset));
    anchor
}

pub(in crate::ui) fn reload_after_catalog_delete(
    shared: &Rc<Shared>,
    removed_ids: &[i64],
    reload_state: CatalogDeleteReloadState,
) {
    let after_ids = shared.current_view_ids();
    let model_change = deletion_model_change(
        &reload_state.view_ids,
        &after_ids,
        removed_ids,
        reload_state.generation,
    );
    // One question, asked once, before the reload below bumps the counter
    // itself: is everything captured before the dialog still describing this
    // model? Both the viewport and the focus fallback further down are frozen
    // pre-dialog state and go stale together.
    let captured_state_still_applies = shared.model.generation() == reload_state.generation;
    let mut anchor = if captured_state_still_applies {
        surviving_delete_anchor(reload_state.anchor, &reload_state.view_ids, &after_ids)
    } else {
        // The model changed while the dialog or worker was alive. The stale
        // ModelChange will deliberately fall back to a full invalidation; its
        // equally stale viewport must not overwrite the newer live view.
        capture_reload_anchor(shared)
    };
    // BROWSE-11/NAV-10b: deleting the loaded track requests an automatic
    // reveal before this reload. Its destination is newer than the dialog's
    // frozen viewport, so let it replace only the scroll anchor; the frozen
    // selected IDs still decide the post-delete selection and focus.
    if shared.track_reveal_pending.get() || shared.scroll_glide.destination().is_some() {
        anchor.anchor = capture_reload_anchor(shared).anchor;
    }
    reload_with_anchor_and_viewport(
        shared,
        &anchor,
        ReloadViewport::PreserveAnchor,
        model_change,
        Some(after_ids),
    );
    // Selection restore itself is keyed on track ids and is immune to row
    // shift, but this fallback is positional: it places focus where the first
    // deleted row used to sit. Against a model that something else reshaped
    // while the dialog was open, that position means nothing, and following it
    // would throw focus onto an unrelated track. Leaving focus where GTK put it
    // is the lesser evil.
    if reload_state.selected_positions.is_empty() || !captured_state_still_applies {
        shared.column_view.grab_focus();
        return;
    }
    let selected_after = current_selection_positions(shared);
    if let Some(position) = deletion_focus_position(
        &reload_state.selected_positions,
        &selected_after,
        shared.model.n_items(),
    ) {
        if selected_after.is_empty() {
            shared.selection.select_item(position, true);
        }
    }
    shared.column_view.grab_focus();
}

fn finish(
    shared: &Rc<Shared>,
    report: &DeleteReport,
    mode: DeleteMode,
    reload_state: CatalogDeleteReloadState,
    worker_ms: u128,
) {
    let removed = report.removed_ids.len();
    let callback = shared.on_library_mutated.borrow().clone();
    let player = shared.player.borrow().upgrade();
    let advance_player = player.clone();
    let timings = finish_steps(
        || {
            if let Some(player) = player {
                player.purge_queue_ids(&report.removed_ids);
            }
        },
        || {
            if let Some(player) = advance_player {
                player.advance_after_user_catalog_delete(&report.removed_ids);
            }
        },
        || reload_after_catalog_delete(shared, &report.removed_ids, reload_state),
        || {
            show_toast(
                shared,
                &strings::delete_result_toast(removed, report.failures, mode == DeleteMode::Trash),
            );
        },
    );
    let browse_bar = shared.browse_bar.clone();
    defer_secondary_refresh(move || {
        if let Some(callback) = callback {
            callback(&[]);
        }
        let browse_bar_started = std::time::Instant::now();
        browse_bar.refresh();
        browse_bar_started.elapsed().as_millis()
    });
    tracing::info!(
        worker_ms,
        mutated_ms = timings.mutated_ms,
        advance_ms = timings.advance_ms,
        reload_ms = timings.reload_ms,
        main_thread_ms = timings.mutated_ms + timings.advance_ms + timings.reload_ms,
        removed,
        failed = report.failures,
        trashed = mode == DeleteMode::Trash,
        "delete batch completed"
    );
}

fn defer_secondary_refresh(refresh: impl FnOnce() -> u128 + 'static) {
    glib::idle_add_local_once(move || {
        let browse_bar_ms = refresh();
        tracing::info!(
            stage = "secondary_surfaces",
            browse_bar_ms,
            "delete batch completed"
        );
    });
}

#[derive(Debug, PartialEq, Eq)]
struct FinishTimings {
    mutated_ms: u128,
    advance_ms: u128,
    reload_ms: u128,
}

fn finish_steps(
    purge: impl FnOnce(),
    advance: impl FnOnce(),
    reload: impl FnOnce(),
    toast: impl FnOnce(),
) -> FinishTimings {
    let mutated_started = std::time::Instant::now();
    purge();
    let mutated_ms = mutated_started.elapsed().as_millis();

    let advance_started = std::time::Instant::now();
    advance();
    let advance_ms = advance_started.elapsed().as_millis();

    let reload_started = std::time::Instant::now();
    reload();
    let reload_ms = reload_started.elapsed().as_millis();

    toast();
    FinishTimings {
        mutated_ms,
        advance_ms,
        reload_ms,
    }
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
        let reload_state = capture_catalog_delete_reload(&shared);
        start_worker(&shared, tracks, mode, reload_state);
    });
}

fn safe_scratch_tracks(tracks: &[(i64, PathBuf)]) -> bool {
    let Ok(scan_root) = std::env::var("REPRISE_SCAN_DIR") else {
        return false;
    };
    path_guard::paths_within_temp_root(
        Path::new(&scan_root),
        &tracks
            .iter()
            .map(|(_, path)| path.clone())
            .collect::<Vec<_>>(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Default)]
    struct CapturedLogs(Arc<Mutex<Vec<u8>>>);

    struct LogWriter(Arc<Mutex<Vec<u8>>>);

    impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for CapturedLogs {
        type Writer = LogWriter;

        fn make_writer(&'a self) -> Self::Writer {
            LogWriter(Arc::clone(&self.0))
        }
    }

    impl Write for LogWriter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn capture_info(operation: impl FnOnce()) -> String {
        let output = CapturedLogs::default();
        let subscriber = tracing_subscriber::fmt()
            .without_time()
            .with_ansi(false)
            .with_target(false)
            .with_max_level(tracing::Level::INFO)
            .with_writer(output.clone())
            .finish();
        tracing::subscriber::with_default(subscriber, operation);
        let bytes = output.0.lock().unwrap().clone();
        String::from_utf8(bytes).unwrap()
    }

    fn insert_track(db: &reprise_core::db::Db, id: i64, path: &Path, title: &str) {
        crate::test_db::connection(db)
            .execute(
                "INSERT INTO tracks (id,path,title,artist,added_at) VALUES (?1,?2,?3,'',0)",
                rusqlite::params![id, path.to_string_lossy(), title],
            )
            .unwrap();
    }

    // The smoke guard itself is proved in
    // `reprise_core::library::path_guard`, which now owns it.

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
    fn browse_11_deletion_focus_uses_survivor_then_next_then_previous() {
        assert_eq!(deletion_focus_position(&[4, 5], &[4], 8), Some(4));
        assert_eq!(deletion_focus_position(&[4, 5], &[], 6), Some(4));
        assert_eq!(deletion_focus_position(&[6, 7], &[], 6), Some(5));
        assert_eq!(deletion_focus_position(&[0], &[], 0), None);
    }

    #[test]
    fn catalog_delete_delta_removes_only_the_changed_middle_span() {
        let change = deletion_model_change(&[1, 2, 3, 4, 5], &[1, 4, 5], &[2, 3], 7)
            .expect("two deleted rows must produce a model delta");

        assert_eq!(change.position, 1);
        assert_eq!(change.removed, 2);
        assert_eq!(change.added, 0);
        assert_eq!(change.before_total, 5);
        assert_eq!(change.after_total, 3);
        assert_eq!(change.generation, 7);
    }

    #[test]
    fn a_deleted_scroll_anchor_moves_to_the_row_that_took_its_place() {
        let anchor = crate::ui::track_list::reload_restore::ReloadAnchor {
            selected_ids: vec![3],
            anchor: Some((3, 7.5)),
            row_height: None,
        };

        let restored = surviving_delete_anchor(anchor, &[1, 2, 3, 4, 5], &[1, 2, 4, 5]);

        assert_eq!(restored.anchor, Some((4, 7.5)));
    }

    #[test]
    #[ignore = "uses the global GLib main context; run alone"]
    fn finish_orders_reload_before_the_deferred_refreshes() {
        use std::cell::RefCell;

        let _main_context = crate::ui::test_main_context::lock_main_context();
        let calls = Rc::new(RefCell::new(Vec::new()));
        let record = |name| {
            let calls = calls.clone();
            move || calls.borrow_mut().push(name)
        };

        finish_steps(
            record("purge"),
            record("advance"),
            record("reload"),
            record("toast"),
        );
        defer_secondary_refresh({
            let sidebar = record("sidebar_refresh");
            let browse_bar = record("browse_bar_refresh");
            move || {
                sidebar();
                browse_bar();
                0
            }
        });

        assert_eq!(&*calls.borrow(), &["purge", "advance", "reload", "toast"]);
        while gtk4::glib::MainContext::default().iteration(false) {}
        assert_eq!(
            &*calls.borrow(),
            &[
                "purge",
                "advance",
                "reload",
                "toast",
                "sidebar_refresh",
                "browse_bar_refresh",
            ]
        );
    }

    #[test]
    #[ignore = "uses the global GLib main context; run alone"]
    fn deferred_secondary_refresh_logs_measured_browse_bar_field_for_batch_event() {
        let _main_context = crate::ui::test_main_context::lock_main_context();

        let logs = capture_info(|| {
            defer_secondary_refresh(|| 17_u128);
            while gtk4::glib::MainContext::default().iteration(false) {}
        });

        assert!(logs.contains("delete batch completed"), "{logs}");
        assert!(logs.contains("stage=\"secondary_surfaces\""), "{logs}");
        assert!(logs.contains("browse_bar_ms=17"), "{logs}");
    }
}

#[cfg(test)]
#[path = "delete_tracks_display_tests.rs"]
mod display_tests;

#[cfg(test)]
#[path = "delete_tracks_large_block_display_tests.rs"]
mod large_block_display_tests;
