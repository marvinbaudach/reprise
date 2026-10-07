//! Which tracks of a selection the tag editor edits (CUE-12). A track cut from
//! a CUE sheet has no tags of its own (CUE-5): its file's tags belong to every
//! track in it. The editor leaves such tracks out; a selection of nothing but
//! them gets a notice instead of an editor.

use std::path::PathBuf;
use std::rc::Rc;

use reprise_core::db::Db;
use reprise_core::library::tag_edit::{track_edit_seed_by_id, EditableTags};
use reprise_core::library::tag_edit_session::SessionTrack;

use crate::ui::track_list::Shared;
use crate::ui::track_list_context_menu::current_selection_positions;

/// One track the editor can open, with the bitrate its header shows.
pub(super) type Editable = (SessionTrack, Option<u32>);

/// What a selection opens.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum TagEditChoice<T> {
    /// The editor, on these tracks; `cue_left_out` CUE tracks were left out.
    Edit { tracks: Vec<T>, cue_left_out: usize },
    /// Only CUE tracks were selected: a notice, no editor.
    CueOnly { count: usize },
    /// Nothing that can be edited or explained.
    Nothing,
}

/// Classifies `items`, each `None` for a CUE track and `Some` for one the
/// editor can open; tracks it cannot open for another reason are absent.
pub(super) fn choose<T>(items: impl IntoIterator<Item = Option<T>>) -> TagEditChoice<T> {
    let mut tracks = Vec::new();
    let mut cue_left_out = 0;
    for item in items {
        match item {
            Some(track) => tracks.push(track),
            None => cue_left_out += 1,
        }
    }
    match (tracks.is_empty(), cue_left_out) {
        (false, _) => TagEditChoice::Edit {
            tracks,
            cue_left_out,
        },
        (true, 0) => TagEditChoice::Nothing,
        (true, count) => TagEditChoice::CueOnly { count },
    }
}

/// Builds one `SessionTrack` plus its bitrate from a `models::Track` row —
/// the in-memory data the visible list already has, no disk re-read needed
/// for the normal open-from-selection path.
pub(super) fn session_track_from_model(track: &reprise_core::models::Track) -> Editable {
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

/// The selection, classified.
pub(super) fn from_selection(shared: &Rc<Shared>) -> TagEditChoice<Editable> {
    let positions = current_selection_positions(shared);
    let mut items = Vec::with_capacity(positions.len());
    for position in positions {
        let Some(track) = shared.model.track_at(position) else {
            // A row the model cannot resolve makes the selection incomplete.
            return TagEditChoice::Nothing;
        };
        // CTX-8: tags are edited on present files only — missing rows are
        // skipped, so a mixed selection edits the present subset and the
        // editor title counts only those. An all-missing selection yields
        // no editor, matching the menu's disabled edit-tags state.
        if track.is_missing() {
            continue;
        }
        items.push(
            track
                .segment
                .is_none()
                .then(|| session_track_from_model(&track)),
        );
    }
    choose(items)
}

/// Fresh, pending-free `SessionTrack`s for an explicit id list (FB-3's
/// "Edit failed tracks…" retry path), classified like a selection — re-reads
/// path/rating from the DB and tags straight from the file, since these ids
/// may not even be in the currently visible/filtered list anymore.
pub(super) fn for_ids(db: &Db, ids: &[i64]) -> TagEditChoice<Editable> {
    choose(ids.iter().filter_map(|&id| {
        let is_cue = reprise_core::queries::query_present_track_by_id(db, id)
            .ok()
            .flatten()
            .is_some_and(|track| track.segment.is_some());
        if is_cue {
            return Some(None);
        }
        let seed = track_edit_seed_by_id(db, id).ok().flatten()?;
        let tags = reprise_core::library::tag_edit::read_editable_tags(&seed.path).ok()?;
        Some(Some((
            SessionTrack {
                id: seed.id,
                path: seed.path,
                tags,
                rating: seed.rating,
            },
            seed.bitrate_kbps,
        )))
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cue_12_a_selection_of_cue_tracks_only_gets_a_notice() {
        assert_eq!(
            choose::<u8>([None, None]),
            TagEditChoice::CueOnly { count: 2 }
        );
    }

    #[test]
    fn cue_12_a_mixed_selection_edits_the_whole_file_tracks_and_counts_the_rest() {
        assert_eq!(
            choose([Some(1), None, Some(3), None]),
            TagEditChoice::Edit {
                tracks: vec![1, 3],
                cue_left_out: 2,
            }
        );
        assert_eq!(
            choose([Some(1)]),
            TagEditChoice::Edit {
                tracks: vec![1],
                cue_left_out: 0,
            }
        );
        assert_eq!(choose::<u8>([]), TagEditChoice::Nothing);
    }

    #[test]
    fn cue_12_the_failed_tracks_retry_leaves_cue_tracks_out() {
        let db = crate::test_db::open().unwrap();
        crate::test_db::connection(&db)
            .execute_batch(
                "INSERT INTO tracks (id, path, title, artist, added_at, segment_index,
                                     segment_start_ms, segment_end_ms)
                 VALUES (7, '/nowhere/album.flac', 'One', '', 0, 1, 0, 1000);",
            )
            .unwrap();

        assert_eq!(for_ids(&db, &[7]), TagEditChoice::CueOnly { count: 1 });
    }
}
