//! Context-menu orchestration for batch tag editing: resolve the complete
//! selection into a [`TagEditSession`], present the dialog, stream the save
//! batch's progress while the dialog stays open (F2), then reconcile GTK
//! state and show the FB-3 failure path once the write has completed.
//!
//! F0 note: the selection is built straight into `SessionTrack`s (with a
//! parallel `bitrate_kbps` list for the header subtitle) instead of the
//! three parallel `Vec`s (`tracks`/`tags`/`ratings`) the pre-session editor
//! needed — `TagEditSession` is the single state truth from the moment the
//! dialog opens.
//!
//! F2 note: the write no longer runs as a single opaque one-shot batch
//! (`one_shot_task`, bounded(1), fire-and-once by design — wrong shape for
//! a progress *stream*). `spawn_save` opens its own connection on a worker
//! thread, same as `scan_worker.rs`'s scan does, and streams `(done, total)`
//! over a `bounded(1)` "latest wins" channel the same way
//! `scan_worker::publish_latest_progress` does (reimplemented generically
//! here as `publish_latest` — `ScanProgress` isn't the type flowing through
//! this one).

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use gtk4::gio;
use gtk4::gio::prelude::*;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::library::tag_edit::{
    apply_track_writes, EditableTags, TagBatchReport, TagWriteFailure, TrackWrite,
};
use reprise_core::library::tag_edit_session::SessionTrack;
use rusqlite::Connection;

use crate::ui::player_controller::PlayerController;
use crate::ui::sidebar::Sidebar;
use crate::ui::strings;
use crate::ui::tag_editor;
use crate::ui::tag_editor_failures;
use crate::ui::track_list::{reload, show_toast, Shared, TrackList};
use crate::ui::track_list_context_menu::current_selection_positions;

pub(in crate::ui) const ACTION_EDIT_TAGS: &str = "edit-tags";
const SMOKE_TAG_EDIT_ENV_VAR: &str = "REPRISE_SMOKE_TAG_EDIT";

/// FB-1: a failure toast carries an action ("Details") and is therefore
/// unverdrängbar for its full run, unlike the plain 4 s success toast
/// `toasts::show` covers.
const FAILURE_TOAST_TIMEOUT_S: u32 = 10;

pub(in crate::ui) fn wire_refresh(
    track_list: &TrackList,
    sidebar: &Rc<Sidebar>,
    player: &Option<Rc<PlayerController>>,
) {
    let sidebar_weak = Rc::downgrade(sidebar);
    let player = player.clone();
    track_list.set_on_tags_mutated(move |paths| {
        match sidebar_weak.upgrade() {
            Some(sidebar) => sidebar.refresh("track tags edited"),
            None => tracing::warn!("sidebar is gone; skipping refresh after tag editing"),
        }
        if let Some(player) = &player {
            player.refresh_edited_cover(paths);
        }
    });
}

pub(in crate::ui) fn add_action(group: &gio::SimpleActionGroup, shared: &Rc<Shared>) {
    let action = gio::SimpleAction::new(ACTION_EDIT_TAGS, None);
    {
        let shared = shared.clone();
        action.connect_activate(move |_, _| begin(&shared));
    }
    group.add_action(&action);
}

/// Builds one `SessionTrack` plus its bitrate from a `models::Track` row —
/// the in-memory data the visible list already has, no disk re-read needed
/// for the normal open-from-selection path.
fn session_track_from_model(track: &reprise_core::models::Track) -> (SessionTrack, Option<u32>) {
    let tags = EditableTags {
        title: track.title.clone(),
        artist: track.artist.clone(),
        album: track.album.clone(),
        album_artist: track.album_artist.clone(),
        year: track.year.and_then(|value| u32::try_from(value).ok()),
        track_no: track.track_no.and_then(|value| u32::try_from(value).ok()),
        genre: track.genre.clone(),
    };
    let session_track = SessionTrack {
        id: track.id,
        path: PathBuf::from(&track.path),
        tags,
        rating: track.rating,
    };
    let bitrate = track
        .bitrate_kbps
        .and_then(|value| u32::try_from(value).ok());
    (session_track, bitrate)
}

fn tracks_and_bitrates_from_selection(
    shared: &Rc<Shared>,
) -> Option<(Vec<SessionTrack>, Vec<Option<u32>>)> {
    let positions = current_selection_positions(shared);
    if positions.is_empty() {
        return None;
    }
    let mut tracks = Vec::with_capacity(positions.len());
    let mut bitrates = Vec::with_capacity(positions.len());
    for position in positions {
        let track = shared.model.track_at(position)?;
        let (session_track, bitrate) = session_track_from_model(&track);
        tracks.push(session_track);
        bitrates.push(bitrate);
    }
    Some((tracks, bitrates))
}

/// Fresh, pending-free `SessionTrack`s for an explicit id list (FB-3's
/// "Edit failed tracks…" retry path) — re-reads path/rating from the DB and
/// tags straight from the file, since these ids may not even be in the
/// currently visible/filtered list anymore.
fn tracks_and_bitrates_for_ids(conn: &Connection, ids: &[i64]) -> Vec<(SessionTrack, Option<u32>)> {
    ids.iter()
        .filter_map(|&id| {
            let (path, rating, bitrate): (String, i32, Option<i64>) = conn
                .query_row(
                    "SELECT path, rating, bitrate_kbps FROM tracks WHERE id=?1",
                    [id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .ok()?;
            let path = PathBuf::from(path);
            let tags = reprise_core::library::tag_edit::read_editable_tags(&path).ok()?;
            let bitrate = bitrate.and_then(|value| u32::try_from(value).ok());
            Some((
                SessionTrack {
                    id,
                    path,
                    tags,
                    rating,
                },
                bitrate,
            ))
        })
        .collect()
}

fn begin(shared: &Rc<Shared>) {
    let Some((tracks, bitrates)) = tracks_and_bitrates_from_selection(shared) else {
        tracing::debug!("tag editor requested without a fully resolvable selection");
        return;
    };
    open_editor(shared, tracks, &bitrates);
}

fn begin_for_ids(shared: &Rc<Shared>, ids: &[i64]) {
    let entries = {
        let conn = shared.conn.borrow();
        tracks_and_bitrates_for_ids(&conn, ids)
    };
    if entries.is_empty() {
        tracing::warn!("tag editor retry: none of the failed tracks could be re-read");
        return;
    }
    let (tracks, bitrates): (Vec<_>, Vec<_>) = entries.into_iter().unzip();
    open_editor(shared, tracks, &bitrates);
}

fn open_editor(shared: &Rc<Shared>, tracks: Vec<SessionTrack>, bitrates: &[Option<u32>]) {
    let Some(window) = shared.window.upgrade() else {
        tracing::warn!("tag editor: window is gone");
        return;
    };
    let conn = shared.conn.clone();
    let shared_for_saved = shared.clone();
    tag_editor::present(
        &window,
        &conn,
        tracks,
        bitrates,
        move |writes, report| finish_apply(&shared_for_saved, &writes, &report),
        |_direction| false,
    );
    tracing::debug!("tag editor presented");
}

/// Widget handles [`spawn_save`] disables for the write's duration and
/// re-enables only on a (rare) catastrophic failure — grouped into one
/// struct purely to stay under clippy's argument-count lint, not because
/// they're conceptually one thing (`tag_editor.rs`, the actual widget
/// owner, builds this from its `TagEditorForm` clones).
#[derive(Clone)]
pub(in crate::ui) struct SaveProgressWidgets {
    pub(in crate::ui) dialog: adw::Dialog,
    pub(in crate::ui) save_button: gtk4::Button,
    pub(in crate::ui) cancel_button: gtk4::Button,
    pub(in crate::ui) content: gtk4::Box,
    pub(in crate::ui) error_label: gtk4::Label,
}

/// F2: streams `apply_track_writes`'s progress into the still-open dialog,
/// disabling everything but the (now progress-labelled) Save button and
/// re-enabling nothing on success — the dialog closes instead. `on_finished`
/// only runs after that close, carrying the batch back alongside the
/// report so the caller can tell which succeeded tracks actually touched
/// tags (as opposed to rating-only) without re-deriving it.
pub(in crate::ui) fn spawn_save(
    conn: &Rc<RefCell<Connection>>,
    widgets: SaveProgressWidgets,
    writes: Vec<TrackWrite>,
    on_finished: impl Fn(Vec<TrackWrite>, TagBatchReport) + 'static,
) {
    let SaveProgressWidgets {
        dialog,
        save_button,
        cancel_button,
        content,
        error_label,
    } = widgets;

    let total = writes.len();
    save_button.set_sensitive(false);
    cancel_button.set_sensitive(false);
    content.set_sensitive(false);
    save_button.set_label(&strings::tag_saving_progress(0, total));

    let db_path = conn.borrow().path().map(PathBuf::from);
    let Some(db_path) = db_path else {
        tracing::warn!("tag-edit save: database has no path; aborting save");
        save_button.set_sensitive(true);
        cancel_button.set_sensitive(true);
        content.set_sensitive(true);
        error_label.set_label(&strings::text(strings::TAG_EDIT_DATABASE_UNAVAILABLE));
        error_label.set_visible(true);
        return;
    };

    let (progress_tx, progress_rx) = async_channel::bounded::<(usize, usize)>(1);
    let (result_tx, result_rx) = async_channel::bounded(1);
    let writes_for_result = writes.clone();
    let stale_rx = progress_rx.clone();
    std::thread::spawn(move || {
        let result = reprise_core::db::open_migrated(Some(&db_path)).map(|mut worker_conn| {
            apply_track_writes(&mut worker_conn, &writes, &mut |done, done_total| {
                publish_latest(&progress_tx, &stale_rx, (done, done_total));
            })
        });
        drop(progress_tx);
        drop(stale_rx);
        let _ = result_tx.send_blocking(result);
    });

    let progress_button = save_button.clone();
    glib::spawn_future_local(async move {
        while let Ok((done, done_total)) = progress_rx.recv().await {
            progress_button.set_label(&strings::tag_saving_progress(done, done_total));
        }
    });

    glib::spawn_future_local(async move {
        match result_rx.recv().await {
            Ok(Ok(report)) => {
                dialog.close();
                on_finished(writes_for_result, report);
            }
            Ok(Err(error)) => {
                tracing::error!(%error, "tag-edit save worker could not open the database");
                save_button.set_sensitive(true);
                cancel_button.set_sensitive(true);
                content.set_sensitive(true);
                error_label.set_label(&strings::text(strings::TAG_EDIT_DATABASE_UNAVAILABLE));
                error_label.set_visible(true);
            }
            Err(error) => {
                tracing::error!(%error, "tag-edit save worker channel closed unexpectedly");
                save_button.set_sensitive(true);
                cancel_button.set_sensitive(true);
                content.set_sensitive(true);
                error_label.set_label(&strings::text(strings::TAG_EDIT_WORKER_FAILED));
                error_label.set_visible(true);
            }
        }
    });
}

/// The same "latest progress wins, never blocks the writer" pattern as
/// `scan_worker::publish_latest_progress`, generalized over the payload
/// type instead of hardcoding `ScanProgress`.
fn publish_latest<T: Send + 'static>(
    sender: &async_channel::Sender<T>,
    receiver: &async_channel::Receiver<T>,
    value: T,
) {
    match sender.try_send(value) {
        Ok(()) => {}
        Err(async_channel::TrySendError::Full(value)) => {
            let _ = receiver.try_recv();
            if let Err(error) = sender.try_send(value) {
                tracing::warn!(%error, "tag-edit save progress dropped: UI receiver is gone");
            }
        }
        Err(async_channel::TrySendError::Closed(_)) => {
            tracing::warn!("tag-edit save progress dropped: UI receiver is gone");
        }
    }
}

fn finish_apply(shared: &Rc<Shared>, writes: &[TrackWrite], report: &TagBatchReport) {
    let updated = report.updated_ids.len();
    let failed = report.failures.len();
    if updated > 0 {
        let tag_changed_paths: Vec<PathBuf> = writes
            .iter()
            .filter(|write| !write.patch.tags.is_empty() && report.updated_ids.contains(&write.id))
            .map(|write| write.path.clone())
            .collect();
        if !tag_changed_paths.is_empty() {
            shared.cover_loader.invalidate_paths(&tag_changed_paths);
            shared.browse_bar.refresh();
        }
        reload(shared);
        if !tag_changed_paths.is_empty() {
            let tag_changed_ids: Vec<i64> = writes
                .iter()
                .filter(|write| {
                    !write.patch.tags.is_empty() && report.updated_ids.contains(&write.id)
                })
                .map(|write| write.id)
                .collect();
            if let Some(player) = shared.player.borrow().upgrade() {
                player.refresh_edited_metadata(&tag_changed_ids);
            }
            let on_tags_mutated = shared.on_tags_mutated.borrow().clone();
            if let Some(on_tags_mutated) = on_tags_mutated {
                on_tags_mutated(&tag_changed_paths);
            }
        }
    }
    tracing::info!(updated, failed, "tag-edit batch completed");

    if report.failures.is_empty() {
        show_toast(shared, &strings::tag_save_result_toast(updated));
    } else {
        show_failure_toast(shared, updated, report.failures.clone());
    }
}

/// FB-3: one unverdrängbar toast for the whole batch's failures — never one
/// per failure — with a "Details" action that opens
/// `tag_editor_failures::present`.
fn show_failure_toast(shared: &Rc<Shared>, updated: usize, failures: Vec<TagWriteFailure>) {
    let Some(overlay) = shared.toast_overlay.upgrade() else {
        tracing::warn!("toast overlay is gone; degrading to log-only for tag-edit failures");
        return;
    };
    let toast = adw::Toast::new(&strings::tag_save_result_toast_with_failures(
        updated,
        failures.len(),
    ));
    toast.set_timeout(FAILURE_TOAST_TIMEOUT_S);
    toast.set_priority(adw::ToastPriority::High);
    toast.set_button_label(Some(&strings::text(strings::TAG_SAVE_FAILURE_DETAILS)));

    let shared_weak = Rc::downgrade(shared);
    toast.connect_button_clicked(move |_| {
        let Some(shared) = shared_weak.upgrade() else {
            return;
        };
        let Some(window) = shared.window.upgrade() else {
            return;
        };
        let shared_for_retry = shared.clone();
        tag_editor_failures::present(&window, &failures, move |ids| {
            begin_for_ids(&shared_for_retry, &ids);
        });
    });
    overlay.add_toast(toast);
}

pub(in crate::ui) fn arm_smoke(shared: &Rc<Shared>) {
    let Ok(value) = std::env::var(SMOKE_TAG_EDIT_ENV_VAR) else {
        return;
    };
    let Some(title) = value.strip_prefix("title:").map(str::to_string) else {
        tracing::warn!(%value, "{SMOKE_TAG_EDIT_ENV_VAR} ignored; expected title:<value>");
        return;
    };
    let shared_weak = Rc::downgrade(shared);
    glib::idle_add_local_once(move || {
        let Some(shared) = shared_weak.upgrade() else {
            return;
        };
        let count = shared.model.n_items().min(2);
        if count == 0 {
            tracing::warn!("tag-edit smoke: list is empty");
            return;
        }
        shared.selection.select_range(0, count, true);
        let Some((tracks, _bitrates)) = tracks_and_bitrates_from_selection(&shared) else {
            return;
        };
        let writes: Vec<TrackWrite> = tracks
            .into_iter()
            .map(|track| TrackWrite {
                id: track.id,
                path: track.path,
                patch: reprise_core::library::tag_edit::TrackEditPatch {
                    tags: reprise_core::library::tag_edit::TagPatch {
                        title: Some(title.clone()),
                        ..Default::default()
                    },
                    ..Default::default()
                },
            })
            .collect();
        let db_path = {
            let conn = shared.conn.borrow();
            conn.path().map(PathBuf::from)
        };
        let Some(db_path) = db_path else {
            tracing::warn!("tag-edit smoke: database has no path");
            return;
        };
        let report = reprise_core::db::open_migrated(Some(&db_path))
            .map(|mut worker_conn| apply_track_writes(&mut worker_conn, &writes, &mut |_, _| {}));
        match report {
            Ok(report) => finish_apply(&shared, &writes, &report),
            Err(error) => tracing::warn!(%error, "tag-edit smoke: could not open database"),
        }
    });
}
