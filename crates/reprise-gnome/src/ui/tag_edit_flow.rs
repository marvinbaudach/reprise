//! Context-menu orchestration for batch tag editing: resolve the complete
//! selection, show the dirty-field dialog, perform file/DB work on one
//! dedicated thread, then refresh GTK state on the main context.

use std::path::PathBuf;
use std::rc::Rc;

use gtk4::gio;
use gtk4::gio::prelude::*;
use gtk4::glib;
use gtk4::prelude::*;
use reprise_core::library::tag_edit::{
    apply_patch_batch, summarize, EditableTags, TagBatchReport, TagPatch,
};

use crate::ui::player_controller::PlayerController;
use crate::ui::sidebar::Sidebar;
use crate::ui::strings;
use crate::ui::tag_editor;
use crate::ui::track_list::{reload, show_toast, Shared, TrackList};
use crate::ui::track_list_context_menu::current_selection_positions;

pub(super) const ACTION_EDIT_TAGS: &str = "edit-tags";
const SMOKE_TAG_EDIT_ENV_VAR: &str = "REPRISE_SMOKE_TAG_EDIT";
type SelectedTags = (Vec<(i64, PathBuf)>, Vec<EditableTags>);

pub(super) fn wire_refresh(
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

pub(super) fn add_action(group: &gio::SimpleActionGroup, shared: &Rc<Shared>) {
    let action = gio::SimpleAction::new(ACTION_EDIT_TAGS, None);
    {
        let shared = shared.clone();
        action.connect_activate(move |_, _| begin(&shared));
    }
    group.add_action(&action);
}

fn selected_tags(shared: &Rc<Shared>) -> Option<SelectedTags> {
    let positions = current_selection_positions(shared);
    if positions.is_empty() {
        return None;
    }
    let mut tracks = Vec::with_capacity(positions.len());
    let mut tags = Vec::with_capacity(positions.len());
    for position in positions {
        let track = shared.model.track_at(position)?;
        tracks.push((track.id, PathBuf::from(&track.path)));
        tags.push(EditableTags {
            title: track.title,
            artist: track.artist,
            album: track.album,
            album_artist: track.album_artist,
            year: track.year.and_then(|value| u32::try_from(value).ok()),
            track_no: track.track_no.and_then(|value| u32::try_from(value).ok()),
            genre: track.genre,
        });
    }
    Some((tracks, tags))
}

fn begin(shared: &Rc<Shared>) {
    let Some((tracks, tags)) = selected_tags(shared) else {
        tracing::debug!("tag editor requested without a fully resolvable selection");
        return;
    };
    let Some(summary) = summarize(&tags) else {
        return;
    };
    let Some(window) = shared.window.upgrade() else {
        tracing::warn!("tag editor: window is gone");
        return;
    };
    let shared = shared.clone();
    tag_editor::present(&window, &summary, move |patch| {
        start_apply(&shared, tracks.clone(), patch);
    });
}

fn start_apply(shared: &Rc<Shared>, tracks: Vec<(i64, PathBuf)>, patch: TagPatch) {
    if patch.is_empty() {
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
    let (sender, receiver) = async_channel::bounded(1);
    let tracks_for_worker = tracks.clone();
    let spawned = std::thread::Builder::new()
        .name("reprise-tag-edit".into())
        .spawn(move || {
            let result = reprise_core::db::open(Some(&db_path))
                .map(|mut conn| apply_patch_batch(&mut conn, &tracks_for_worker, &patch))
                .map_err(|error| error.to_string());
            let _ = sender.try_send(result);
        });
    if let Err(error) = spawned {
        tracing::warn!(%error, "could not start tag-edit worker");
        show_toast(shared, &strings::text(strings::TAG_EDIT_WORKER_FAILED));
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
            Ok(report) => finish_apply(&shared, &tracks, &report),
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

fn finish_apply(shared: &Rc<Shared>, tracks: &[(i64, PathBuf)], report: &TagBatchReport) {
    let updated = report.updated_ids.len();
    let failed = report.failures.len();
    if updated > 0 {
        let paths: Vec<_> = tracks
            .iter()
            .filter(|(id, _)| report.updated_ids.contains(id))
            .map(|(_, path)| path.clone())
            .collect();
        shared.cover_loader.invalidate_paths(&paths);
        shared.browse_bar.refresh();
        reload(shared);
        let on_tags_mutated = shared.on_tags_mutated.borrow().clone();
        if let Some(on_tags_mutated) = on_tags_mutated {
            on_tags_mutated(&paths);
        }
    }
    tracing::info!(updated, failed, "tag-edit batch completed");
    show_toast(shared, &strings::tag_edit_result_toast(updated, failed));
}

pub(super) fn arm_smoke(shared: &Rc<Shared>) {
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
        let Some((tracks, _)) = selected_tags(&shared) else {
            return;
        };
        start_apply(
            &shared,
            tracks,
            TagPatch {
                title: Some(title),
                ..TagPatch::default()
            },
        );
    });
}
