//! Context-menu orchestration for batch tag editing: resolve the complete
//! selection into a [`TagEditSession`], present the dialog, and reconcile
//! GTK state once the resulting write batch has been applied.
//!
//! F0 note: the selection is now built straight into `SessionTrack`s (with
//! a parallel `bitrate_kbps` list for the header subtitle) instead of the
//! three parallel `Vec`s (`tracks`/`tags`/`ratings`) the pre-session editor
//! needed — `TagEditSession` is the single state truth from the moment the
//! dialog opens. The actual write still runs as a single synchronous
//! one-shot batch on a worker thread here; F2 replaces this with a
//! streamed, in-dialog progress save and the FB-3 failure-details path.

use std::path::PathBuf;
use std::rc::Rc;

use gtk4::gio;
use gtk4::gio::prelude::*;
use gtk4::glib;
use gtk4::prelude::*;
use reprise_core::library::tag_edit::{
    apply_track_writes, EditableTags, TagBatchReport, TrackWrite,
};
use reprise_core::library::tag_edit_session::SessionTrack;

use crate::ui::one_shot_task;
use crate::ui::player_controller::PlayerController;
use crate::ui::sidebar::Sidebar;
use crate::ui::strings;
use crate::ui::tag_editor;
use crate::ui::track_list::{reload, show_toast, Shared, TrackList};
use crate::ui::track_list_context_menu::current_selection_positions;

pub(in crate::ui) const ACTION_EDIT_TAGS: &str = "edit-tags";
const SMOKE_TAG_EDIT_ENV_VAR: &str = "REPRISE_SMOKE_TAG_EDIT";

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

fn begin(shared: &Rc<Shared>) {
    let Some((tracks, bitrates)) = tracks_and_bitrates_from_selection(shared) else {
        tracing::debug!("tag editor requested without a fully resolvable selection");
        return;
    };
    open_editor(shared, tracks, &bitrates);
}

fn open_editor(shared: &Rc<Shared>, tracks: Vec<SessionTrack>, bitrates: &[Option<u32>]) {
    let Some(window) = shared.window.upgrade() else {
        tracing::warn!("tag editor: window is gone");
        return;
    };
    let conn = shared.conn.clone();
    let shared_for_save = shared.clone();
    tag_editor::present(
        &window,
        &conn,
        tracks,
        bitrates,
        move |writes| {
            start_apply(&shared_for_save, writes);
        },
        |_direction| false,
    );
    tracing::debug!("tag editor presented");
}

fn start_apply(shared: &Rc<Shared>, writes: Vec<TrackWrite>) {
    if writes.is_empty() {
        return;
    }
    let db_path = {
        let conn = shared.conn.borrow();
        conn.path().map(PathBuf::from)
    };
    let Some(db_path) = db_path else {
        show_toast(
            shared,
            &strings::text(strings::TAG_EDIT_DATABASE_UNAVAILABLE),
        );
        return;
    };
    let writes_for_worker = writes.clone();
    let receiver = match one_shot_task::spawn("reprise-tag-edit", move || {
        reprise_core::db::open_migrated(Some(&db_path)).map(|mut conn| {
            apply_track_writes(&mut conn, &writes_for_worker, &mut |_done, _total| {})
        })
    }) {
        Ok(receiver) => receiver,
        Err(error) => {
            tracing::warn!(%error, "could not start tag-edit worker");
            show_toast(shared, &strings::text(strings::TAG_EDIT_WORKER_FAILED));
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
            Ok(report) => finish_apply(&shared, &writes, &report),
            Err(error) => {
                tracing::warn!(%error, "tag-edit worker could not open database");
                show_toast(
                    &shared,
                    &strings::text(strings::TAG_EDIT_DATABASE_UNAVAILABLE),
                );
            }
        }
    });
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
    show_toast(shared, &strings::track_edit_result_toast(updated, failed));
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
        start_apply(&shared, writes);
    });
}
