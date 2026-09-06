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

use std::path::PathBuf;
use std::rc::Rc;

use gtk4::gio;
use gtk4::gio::prelude::*;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::library::tag_edit::{
    apply_track_writes, live_track_edit_seed_by_path, track_edit_seed_by_id, EditableTags,
    TagBatchReport, TagWriteFailure, TrackWrite,
};
use reprise_core::library::tag_edit_session::SessionTrack;
use reprise_core::view_source::ViewSource;

use crate::ui::one_shot_task;
use crate::ui::player_controller::PlayerController;
use crate::ui::sidebar::Sidebar;
use crate::ui::strings;
use crate::ui::tag_edit::tag_reload_anchor::{post_save_reload_anchor, OpenedReloadState};
use crate::ui::tag_edit::tag_save_refresh::{self, TagSaveRefresh};
use crate::ui::tag_edit::tag_write_admission;
use crate::ui::tag_editor;
use crate::ui::tag_editor_failures;
use crate::ui::track_list::tag_mutation_refresh::{
    refresh_after_tag_mutation_with_anchor, refresh_after_tag_mutation_with_view_ids,
};
use crate::ui::track_list::track_list_activation::current_queue_ids;
use crate::ui::track_list::track_list_reload::{capture_reload_anchor, reload_with_anchor};
use crate::ui::track_list::{show_toast, Shared, TrackList};
use crate::ui::track_list_context_menu::current_selection_positions;
use reprise_core::db::Db;

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
fn tracks_and_bitrates_for_ids(db: &Db, ids: &[i64]) -> Vec<(SessionTrack, Option<u32>)> {
    ids.iter()
        .filter_map(|&id| {
            let seed = track_edit_seed_by_id(db, id).ok().flatten()?;
            let tags = reprise_core::library::tag_edit::read_editable_tags(&seed.path).ok()?;
            Some((
                SessionTrack {
                    id: seed.id,
                    path: seed.path,
                    tags,
                    rating: seed.rating,
                },
                seed.bitrate_kbps,
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

pub(in crate::ui) fn begin_for_ids(shared: &Rc<Shared>, ids: &[i64]) {
    let entries = tracks_and_bitrates_for_ids(&shared.conn, ids);
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
    let queue_items = queue_ids
        .iter()
        .copied()
        .map(reprise_core::up_next::QueueItem::Track)
        .collect::<Vec<_>>();
    let total = match i64::try_from(ids.len()) {
        Ok(total) => total,
        Err(error) => {
            tracing::warn!(%error, "tag editor: browse snapshot too large to query");
            return None;
        }
    };
    let rows = {
        let conn = &shared.conn;
        reprise_core::queries::query_track_window_browsed(
            conn,
            &source,
            &sort.field,
            &sort.dir,
            &filter,
            &browse_filter,
            0,
            total,
            &queue_items,
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
    let opened_reload = OpenedReloadState {
        anchor: capture_reload_anchor(shared),
        view_ids: browse
            .as_ref()
            .map(tag_editor::BrowseSnapshot::ids)
            .unwrap_or_default(),
    };
    let on_write_started = shared
        .on_tag_write_started
        .borrow()
        .clone()
        .unwrap_or_else(|| Rc::new(|| {}));
    let _ = tag_editor::present(
        &window,
        &conn,
        tracks,
        bitrates,
        browse,
        &shared.cover_loader,
        tag_editor::PresentCallbacks {
            on_write_started,
            on_saved: move |writes: Vec<TrackWrite>, report, write_ms, tracks| {
                finish_apply(
                    &shared_for_saved,
                    &writes,
                    &report,
                    ApplyOrigin::TrackList,
                    Some(opened_reload.clone()),
                    write_ms,
                    tracks,
                );
            },
        },
    );
}

/// G1-adjacent (import-hint fix): a single-track open by path, used by the
/// "Open in Tag Editor" action on an import-error HINT row. There is no
/// browse context for a hint edit — it did not come from the visible track
/// list — so `browse` is always `None`, and completion routes through
/// `finish_apply` tagged `ApplyOrigin::ImportHint` so a clean save can elide
/// the usual success toast (the row just disappearing from the failed-import
/// list is feedback enough).
pub(in crate::ui) fn begin_for_path(shared: &Rc<Shared>, path: &str) {
    let Ok(Some(seed)) = live_track_edit_seed_by_path(&shared.conn, path) else {
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
        id: seed.id,
        path: seed.path,
        tags: seed.tags,
        rating: seed.rating,
    };
    let on_write_started = shared
        .on_tag_write_started
        .borrow()
        .clone()
        .unwrap_or_else(|| Rc::new(|| {}));
    let _ = tag_editor::present(
        &window,
        &conn,
        vec![session_track],
        &[seed.bitrate_kbps],
        None,
        &shared.cover_loader,
        tag_editor::PresentCallbacks {
            on_write_started,
            on_saved: move |writes: Vec<TrackWrite>, report, write_ms, tracks| {
                finish_apply(
                    &shared_for_saved,
                    &writes,
                    &report,
                    ApplyOrigin::ImportHint,
                    None,
                    write_ms,
                    tracks,
                );
            },
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
    conn: &Rc<Db>,
    widgets: SaveProgressWidgets,
    writes: Vec<TrackWrite>,
    on_write_started: &Rc<dyn Fn()>,
    on_finished: impl Fn(Vec<TrackWrite>, TagBatchReport, u128, usize) + 'static,
) {
    let write_started = std::time::Instant::now();
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

    let db_path = conn.path();
    let Some(db_path) = db_path else {
        tracing::warn!("tag-edit save: database has no path; aborting save");
        save_button.set_sensitive(true);
        cancel_button.set_sensitive(true);
        content.set_sensitive(true);
        error_label.set_label(&strings::text(strings::TAG_EDIT_DATABASE_UNAVAILABLE));
        error_label.set_visible(true);
        return;
    };
    let lock_attempt = match tag_write_admission::acquire(&db_path) {
        Ok(attempt) => attempt,
        Err(failure) => {
            tracing::warn!(detail = %failure.detail, "tag-edit save could not acquire write slot");
            save_button.set_sensitive(true);
            cancel_button.set_sensitive(true);
            content.set_sensitive(true);
            error_label.set_label(&failure.user_message());
            error_label.set_visible(true);
            return;
        }
    };
    let writes_for_result = writes.clone();
    let spawned = one_shot_task::spawn_with_progress("reprise-tag-save", move |publish| {
        reprise_core::db::Db::open_migrated(Some(&db_path))
            .map_err(|error| error.to_string())
            .map(|worker_conn| {
                apply_track_writes(
                    &worker_conn,
                    &writes,
                    lock_attempt,
                    &mut |done, done_total| {
                        publish((done, done_total));
                    },
                )
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
    on_write_started();

    let progress_button = save_button.clone();
    glib::spawn_future_local(async move {
        while let Ok((done, done_total)) = progress_rx.recv().await {
            progress_button.set_label(&strings::tag_saving_progress(done, done_total));
        }
    });

    glib::spawn_future_local(async move {
        match result_rx.recv().await {
            Ok(Ok(report)) => {
                let write_ms = write_started.elapsed().as_millis();
                dialog.close();
                on_finished(writes_for_result, report, write_ms, total);
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

fn finish_apply(
    shared: &Rc<Shared>,
    writes: &[TrackWrite],
    report: &TagBatchReport,
    origin: ApplyOrigin,
    opened_reload: Option<OpenedReloadState>,
    write_ms: u128,
    tracks: usize,
) {
    let reload_started = std::time::Instant::now();
    let mut delta = false;
    let updated = report.updated_ids.len();
    let failed = report.failures.len();
    if updated > 0 {
        let tag_changed_paths: Vec<PathBuf> = writes
            .iter()
            .filter(|write| !write.patch.tags.is_empty() && report.updated_ids.contains(&write.id))
            .map(|write| write.path.clone())
            .collect();
        let has_pre_save_view = opened_reload
            .as_ref()
            .is_some_and(|state| !state.view_ids.is_empty());
        let live_reload = opened_reload.unwrap_or_else(|| OpenedReloadState {
            anchor: capture_reload_anchor(shared),
            view_ids: shared.current_view_ids(),
        });
        let sort_field = shared.sort.borrow().field.clone();
        let layout = crate::ui::track_list::track_list_geometry::layout(
            shared,
            live_reload.anchor.row_height,
            live_reload.view_ids.len(),
        );
        let save_anchor = if let Some(layout) = layout.as_ref() {
            post_save_reload_anchor(
                live_reload.anchor,
                &report.updated_ids,
                writes,
                &sort_field,
                &live_reload.view_ids,
                layout,
            )
        } else {
            let mut anchor = live_reload.anchor;
            anchor.selected_ids = report.updated_ids.clone();
            anchor
        };
        if !tag_changed_paths.is_empty() {
            let tag_changed_ids = tag_save_refresh::tag_changed_ids(writes, &report.updated_ids);
            if has_pre_save_view {
                let after_ids = shared.current_view_ids();
                let generation = shared.model.generation();
                delta = tag_save_refresh::tag_save_model_change(
                    &live_reload.view_ids,
                    &after_ids,
                    &tag_changed_ids,
                    generation,
                )
                .is_some();
                if delta {
                    refresh_after_tag_mutation_with_view_ids(
                        shared,
                        &tag_changed_ids,
                        &tag_changed_paths,
                        save_anchor,
                        &live_reload.view_ids,
                        after_ids,
                    );
                } else {
                    refresh_after_tag_mutation_with_anchor(
                        shared,
                        &tag_changed_ids,
                        &tag_changed_paths,
                        save_anchor,
                    );
                }
            } else {
                refresh_after_tag_mutation_with_anchor(
                    shared,
                    &tag_changed_ids,
                    &tag_changed_paths,
                    save_anchor,
                );
            }
        } else {
            let source = shared.source.borrow().clone();
            let browse = shared.browse_filter.borrow().clone();
            match tag_save_refresh::plan(writes, &report.updated_ids, &source, &sort_field, &browse)
            {
                TagSaveRefresh::InPlaceRatings(ratings) => {
                    tag_save_refresh::apply_in_place(shared, &ratings);
                }
                TagSaveRefresh::Reload => reload_with_anchor(shared, &save_anchor),
            }
        }
    }
    tracing::info!(
        write_ms,
        tracks,
        reload_ms = reload_started.elapsed().as_millis(),
        delta,
        updated,
        failed,
        "tag-edit batch completed"
    );

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
    let toast = crate::ui::toasts::plain(&strings::tag_save_result_toast_with_failures(
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
        let db_path = shared.conn.path();
        let Some(db_path) = db_path else {
            tracing::warn!("tag-edit smoke: database has no path");
            return;
        };
        let opened_reload = OpenedReloadState {
            anchor: capture_reload_anchor(&shared),
            view_ids: shared.current_view_ids(),
        };
        let write_started = std::time::Instant::now();
        let report = tag_write_admission::acquire(&db_path).and_then(|lock_attempt| {
            reprise_core::db::Db::open_migrated(Some(&db_path))
                .map_err(|error| tag_write_admission::TagWriteAdmissionFailure {
                    busy: false,
                    detail: error.to_string(),
                })
                .map(|worker_conn| {
                    apply_track_writes(&worker_conn, &writes, lock_attempt, &mut |_, _| {})
                })
        });
        let write_ms = write_started.elapsed().as_millis();
        let tracks = writes.len();
        match report {
            Ok(report) => {
                finish_apply(
                    &shared,
                    &writes,
                    &report,
                    ApplyOrigin::TrackList,
                    Some(opened_reload),
                    write_ms,
                    tracks,
                );
            }
            Err(failure) => {
                tracing::warn!(detail = %failure.detail, "tag-edit smoke: write did not start");
            }
        }
    });
}

#[cfg(test)]
#[path = "tag_edit_flow_tests.rs"]
mod tests;
