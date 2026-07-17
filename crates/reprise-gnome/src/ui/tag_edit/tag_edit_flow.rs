//! Context-menu orchestration for batch tag editing: resolve the complete
//! selection into a `TagEditSession`, present the dialog, stream the save
//! batch's progress while the dialog stays open (F2), then reconcile GTK
//! state and show the FB-3 failure path once the write has completed.
//!
//! F0 note: the selection is built straight into `SessionTrack`s (with a
//! parallel `bitrate_kbps` list for the header subtitle) instead of the
//! three parallel `Vec`s (`tracks`/`tags`/`ratings`) the pre-session editor
//! needed — `TagEditSession` is the single state truth from the moment the
//! dialog opens.
//!
//! F2 note: the write no longer runs as a single opaque batch. `spawn_save`
//! opens its own connection on a worker thread and streams `(done, total)`
//! into the open dialog via `one_shot_task::spawn_with_progress` — the
//! shared helper grew that variant for this, rather than this module
//! hand-rolling the thread and the "latest wins" eviction a third time
//! (`scan_worker.rs` had the second copy). `check-architecture.sh` lists
//! this file among the modules that must route background work through that
//! helper, which is what caught the hand-rolled version.

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
use reprise_core::view_source::ViewSource;
use rusqlite::{Connection, OptionalExtension};

use crate::ui::one_shot_task;
use crate::ui::player_controller::PlayerController;
use crate::ui::sidebar::Sidebar;
use crate::ui::strings;
use crate::ui::tag_editor;
use crate::ui::tag_editor_failures;
use crate::ui::track_list::track_list_activation::current_queue_ids;
use crate::ui::track_list::{reload, reload_restore, show_toast, Shared, TrackList};
use crate::ui::track_list_context_menu::current_selection_positions;

pub(in crate::ui) const ACTION_EDIT_TAGS: &str = "edit-tags";
const SMOKE_TAG_EDIT_ENV_VAR: &str = "REPRISE_SMOKE_TAG_EDIT";

#[derive(Debug, Clone, PartialEq, Eq)]
enum SmokeTagEditMode {
    SaveTitle(String),
    Open(usize),
}

fn parse_smoke_tag_edit_mode(value: &str) -> Option<SmokeTagEditMode> {
    if let Some(title) = value.strip_prefix("title:") {
        return Some(SmokeTagEditMode::SaveTitle(title.to_string()));
    }
    value
        .strip_prefix("open:")
        .and_then(|count| count.parse::<usize>().ok())
        .filter(|count| *count > 0)
        .map(SmokeTagEditMode::Open)
}

/// FB-1: a failure toast carries an action ("Details") and is therefore
/// unverdrängbar for its full run, unlike the plain 4 s success toast
/// `toasts::show` covers.
const FAILURE_TOAST_TIMEOUT_S: u32 = 10;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ApplyOrigin {
    TrackList,
    ImportHint,
}

// Superseded by Task F2's FB-3 toast split in `finish_apply` below
// (`tag_save_result_toast` / `show_failure_toast`), same as the
// `strings::track_edit_result_toast` it wraps — kept for its
// `ApplyOrigin::ImportHint` suppression rule, which `finish_apply` still
// applies inline, and pinned by its own unit test.
#[allow(dead_code)]
fn completion_toast(origin: ApplyOrigin, updated: usize, failed: usize) -> Option<String> {
    if origin == ApplyOrigin::ImportHint && updated > 0 && failed == 0 {
        None
    } else {
        Some(strings::track_edit_result_toast(updated, failed))
    }
}

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
        // CTX-8: tags are edited on present files only — missing rows are
        // skipped, so a mixed selection edits the present subset and the
        // editor title counts only those. An all-missing selection yields
        // None (no editor), matching the menu's disabled edit-tags state.
        if track.is_missing() {
            continue;
        }
        let (session_track, bitrate) = session_track_from_model(&track);
        tracks.push(session_track);
        bitrates.push(bitrate);
    }
    (!tracks.is_empty()).then_some((tracks, bitrates))
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

/// G1 (TAG-4): the browse snapshot for a single-track open — the visible
/// list's ids (`Shared::current_view_ids()`, the same source `reload()`'s
/// TAG-1 restore uses) plus one bulk tag-data query for them, never through
/// `TrackListModel::track_at`. That call windows/caches rows for the
/// *visible* table itself; walking it for every snapshot track here would
/// load or evict cache windows nobody asked to see just because a tag-edit
/// dialog opened — exactly the kind of "the dialog touches the library"
/// TAG-1 rules out. `tag_editor::present` decides for itself whether the
/// result is even usable (single-track mode, more than one id, the opened
/// track actually present in it); `None` here already covers the cheap
/// "nothing to browse" case (0 or 1 visible tracks) without paying for the
/// tag-data query at all.
fn browsable_snapshot(shared: &Rc<Shared>) -> Option<tag_editor::BrowseSnapshot> {
    let ids = shared.current_view_ids();
    if ids.len() <= 1 {
        return None;
    }
    let sort = shared.sort.borrow().clone();
    let filter = shared.filter.borrow().clone();
    let source = shared.source.borrow().clone();
    let browse_filter = shared.browse_filter.borrow().clone();
    let queue_ids = if matches!(source, ViewSource::Queue) {
        current_queue_ids(shared)
    } else {
        Vec::new()
    };
    let total = match i64::try_from(ids.len()) {
        Ok(total) => total,
        Err(error) => {
            tracing::warn!(%error, "tag editor: browse snapshot too large to query");
            return None;
        }
    };
    let rows = {
        let mut conn = shared.conn.borrow_mut();
        reprise_core::queries::query_track_window_browsed(
            &mut conn,
            &source,
            &sort.field,
            &sort.dir,
            &filter,
            &browse_filter,
            0,
            total,
            &queue_ids,
        )
    };
    let by_id: std::collections::HashMap<i64, reprise_core::models::Track> = match rows {
        Ok(tracks) => tracks.into_iter().map(|track| (track.id, track)).collect(),
        Err(error) => {
            tracing::warn!(%error, "tag editor: failed to load the browse snapshot's tag data");
            return None;
        }
    };
    let mut tracks = Vec::with_capacity(ids.len());
    let mut bitrates = Vec::with_capacity(ids.len());
    for id in &ids {
        if let Some(track) = by_id.get(id) {
            let (session_track, bitrate) = session_track_from_model(track);
            tracks.push(session_track);
            bitrates.push(bitrate);
        }
    }
    if tracks.len() <= 1 {
        return None;
    }
    Some(tag_editor::BrowseSnapshot { tracks, bitrates })
}

fn open_editor(shared: &Rc<Shared>, tracks: Vec<SessionTrack>, bitrates: &[Option<u32>]) {
    let Some(window) = shared.window.upgrade() else {
        tracing::warn!("tag editor: window is gone");
        return;
    };
    let conn = shared.conn.clone();
    let shared_for_saved = shared.clone();
    let browse = browsable_snapshot(shared);
    tag_editor::present(
        &window,
        &conn,
        tracks,
        bitrates,
        browse,
        move |writes, report| {
            finish_apply(&shared_for_saved, &writes, &report, ApplyOrigin::TrackList);
        },
    );
    tracing::debug!("tag editor presented");
}

/// G1-adjacent (import-hint fix): a single-track open by path, used by the
/// "Open in Tag Editor" action on an import-error HINT row. There is no
/// browse context for a hint edit — it did not come from the visible track
/// list — so `browse` is always `None`, and completion routes through
/// `finish_apply` tagged `ApplyOrigin::ImportHint` so a clean save can elide
/// the usual success toast (the row just disappearing from the failed-import
/// list is feedback enough).
pub(in crate::ui) fn begin_for_path(shared: &Rc<Shared>, path: &str) {
    let seed = {
        let conn = shared.conn.borrow();
        conn.query_row(
            "SELECT id,title,artist,album,album_artist,year,track_no,genre,rating,bitrate_kbps \
             FROM tracks WHERE path=?1 AND removed_at IS NULL",
            [path],
            |row| {
                let year = row
                    .get::<_, Option<i32>>(5)?
                    .and_then(|value| u32::try_from(value).ok());
                let track_no = row
                    .get::<_, Option<i32>>(6)?
                    .and_then(|value| u32::try_from(value).ok());
                let bitrate = row
                    .get::<_, Option<i64>>(9)?
                    .and_then(|value| u32::try_from(value).ok());
                Ok((
                    row.get::<_, i64>(0)?,
                    EditableTags {
                        title: row.get(1)?,
                        artist: row.get(2)?,
                        album: row.get(3)?,
                        album_artist: row.get(4)?,
                        year,
                        track_no,
                        genre: row.get(7)?,
                    },
                    row.get::<_, i32>(8)?,
                    bitrate,
                ))
            },
        )
        .optional()
    };
    let Ok(Some((id, tags, rating, bitrate))) = seed else {
        tracing::warn!(path, "tag editor: import hint has no live track row");
        return;
    };
    let Some(window) = shared.window.upgrade() else {
        tracing::warn!("tag editor: window is gone");
        return;
    };
    let conn = shared.conn.clone();
    let shared_for_saved = shared.clone();
    let session_track = SessionTrack {
        id,
        path: PathBuf::from(path),
        tags,
        rating,
    };
    tag_editor::present(
        &window,
        &conn,
        vec![session_track],
        &[bitrate],
        None,
        move |writes, report| {
            finish_apply(&shared_for_saved, &writes, &report, ApplyOrigin::ImportHint);
        },
    );
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

    let writes_for_result = writes.clone();
    let spawned = one_shot_task::spawn_with_progress("reprise-tag-save", move |publish| {
        reprise_core::db::open_migrated(Some(&db_path)).map(|mut worker_conn| {
            apply_track_writes(&mut worker_conn, &writes, &mut |done, done_total| {
                publish((done, done_total));
            })
        })
    });
    let (progress_rx, result_rx) = match spawned {
        Ok(channels) => channels,
        Err(error) => {
            tracing::warn!(%error, "could not start the tag-save worker");
            save_button.set_sensitive(true);
            cancel_button.set_sensitive(true);
            content.set_sensitive(true);
            error_label.set_label(&strings::text(strings::TAG_EDIT_WORKER_FAILED));
            error_label.set_visible(true);
            return;
        }
    };

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

/// TAG-1 (G2): forces the live selection to `updated_ids`, position-mapped
/// against the *current* (pre-reload) view, right before `reload(shared)`
/// runs. `reload()`'s own TAG-1 restore mechanic (Task A) then captures and
/// carries exactly this selection across the model swap, so the net effect
/// once `reload()` returns is "selection = the tracks this save actually
/// wrote" — never whatever was selected before the dialog opened, and never
/// a track this save left untouched. An id no longer in the current view
/// (deleted, filtered out) drops out silently, the same TAG-1 "no side
/// effect" rule `reload_restore::positions_for_ids` already applies to a
/// plain reload.
fn select_written_tracks(shared: &Shared, updated_ids: &[i64]) {
    let current_ids = shared.current_view_ids();
    let positions = reload_restore::positions_for_ids(updated_ids, &current_ids);
    shared.selection.unselect_all();
    for position in positions {
        shared.selection.select_item(position, false);
    }
}

fn finish_apply(
    shared: &Rc<Shared>,
    writes: &[TrackWrite],
    report: &TagBatchReport,
    origin: ApplyOrigin,
) {
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
        select_written_tracks(shared, &report.updated_ids);
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
        // ImportHint (the "Open in Tag Editor" fix for an import HINT row)
        // elides the success toast: the row just disappearing from the
        // failed-import list is feedback enough. TrackList saves still get
        // the normal confirmation. Mirrors `completion_toast`'s tested
        // suppression rule above, adapted to FB-3's toast text.
        let hint_edit_succeeded = origin == ApplyOrigin::ImportHint && updated > 0;
        if !hint_edit_succeeded {
            show_toast(shared, &strings::tag_save_result_toast(updated));
        }
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
    let Some(mode) = parse_smoke_tag_edit_mode(&value) else {
        tracing::warn!(
            %value,
            "{SMOKE_TAG_EDIT_ENV_VAR} ignored; expected title:<value> or open:<positive count>"
        );
        return;
    };
    let shared_weak = Rc::downgrade(shared);
    glib::idle_add_local_once(move || {
        let Some(shared) = shared_weak.upgrade() else {
            return;
        };
        let requested_count = match &mode {
            SmokeTagEditMode::SaveTitle(_) => 2,
            SmokeTagEditMode::Open(count) => *count,
        };
        let requested_count = u32::try_from(requested_count).unwrap_or(u32::MAX);
        let count = shared.model.n_items().min(requested_count);
        if count == 0 {
            tracing::warn!("tag-edit smoke: list is empty");
            return;
        }
        shared.selection.select_range(0, count, true);
        let Some((tracks, bitrates)) = tracks_and_bitrates_from_selection(&shared) else {
            return;
        };
        let SmokeTagEditMode::SaveTitle(title) = mode else {
            open_editor(&shared, tracks, &bitrates);
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
            Ok(report) => finish_apply(&shared, &writes, &report, ApplyOrigin::TrackList),
            Err(error) => tracing::warn!(%error, "tag-edit smoke: could not open database"),
        }
    });
}

#[cfg(test)]
mod task_5_6_tests {
    use super::*;

    #[test]
    fn healed_import_hint_refreshes_in_place_without_a_success_toast() {
        assert_eq!(completion_toast(ApplyOrigin::ImportHint, 1, 0), None);
        assert_eq!(
            completion_toast(ApplyOrigin::TrackList, 1, 0).as_deref(),
            Some("Updated 1 track")
        );
        assert_eq!(
            completion_toast(ApplyOrigin::ImportHint, 0, 1).as_deref(),
            Some("Updated 0 tracks; 1 failed")
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke_tag_edit_mode_parses_open_count_and_preserves_title_save() {
        assert_eq!(
            parse_smoke_tag_edit_mode("open:2"),
            Some(SmokeTagEditMode::Open(2))
        );
        assert_eq!(parse_smoke_tag_edit_mode("open:0"), None);
        assert_eq!(parse_smoke_tag_edit_mode("open:many"), None);
        assert_eq!(
            parse_smoke_tag_edit_mode("title:Acceptance title"),
            Some(SmokeTagEditMode::SaveTitle("Acceptance title".into()))
        );
    }

    /// TAG-1 (G2): `select_written_tracks` composes entirely from
    /// `reload_restore::positions_for_ids` (already `#[test]`-covered at
    /// Task A's pure-logic level) plus real `gtk4::MultiSelection` widget
    /// calls this crate's headless suite cannot construct outside the
    /// display-test harness (`scripts/check-display-tests.sh`) — see this
    /// package's report for why a full `Shared` fixture wasn't built for
    /// this wave. This test instead pins the exact mapping the post-save
    /// selection depends on: written ids win, an unrelated failed id never
    /// widens the selection, and an id no longer in the (possibly
    /// concurrently changed) current view drops out silently rather than
    /// erroring — the same "no side effect from a vanished id" rule a plain
    /// `reload()` already applies.
    #[test]
    fn tag_1_selection_after_save_is_written_tracks() {
        let updated_ids = vec![7_i64, 9_i64];
        let current_view = vec![11_i64, 7_i64, 9_i64];
        let positions = reload_restore::positions_for_ids(&updated_ids, &current_view);
        assert_eq!(
            positions,
            vec![1, 2],
            "selection follows the written ids, not the unrelated failed track at position 0"
        );

        let narrowed_view = vec![9_i64];
        assert_eq!(
            reload_restore::positions_for_ids(&updated_ids, &narrowed_view),
            vec![0],
            "a written id no longer in the current view drops out silently"
        );
    }
}
