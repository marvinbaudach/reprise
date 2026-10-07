//! The Doctor proposes tag writes, and a track cut from a CUE sheet has no tags
//! of its own to write (CUE-5), so no scope ever contains one.

use super::freeze_scope;
use crate::library::library_doctor::{DoctorScopeRequest, DoctorViewSnapshot, FrozenScope};

/// A plain track `1` and a CUE file cut into tracks `2` and `3`.
fn library_with_a_cue_file() -> crate::db::Db {
    let db = crate::db::Db::open_in_memory().unwrap();
    db.conn()
        .execute_batch(
            "INSERT INTO tracks (id, path, title, added_at) VALUES (1, '/m/plain.flac', 'Plain', 0);
             INSERT INTO tracks (id, path, title, added_at, segment_index, segment_start_ms,
                                 segment_end_ms, cue_path, cue_mtime, cue_size)
               VALUES (2, '/m/album.flac', 'One', 0, 1, 0, 1000, '/m/album.cue', 1, 1),
                      (3, '/m/album.flac', 'Two', 0, 2, 1000, 2000, '/m/album.cue', 1, 1);",
        )
        .unwrap();
    db
}

fn frozen_ids(db: &crate::db::Db, request: &DoctorScopeRequest) -> Vec<i64> {
    match freeze_scope(db, request).unwrap() {
        FrozenScope::Tracks(tracks) => tracks.iter().map(|track| track.track_id).collect(),
        FrozenScope::FallbackRequired => Vec::new(),
    }
}

#[test]
fn cue_13_the_whole_library_scope_skips_cue_tracks() {
    let db = library_with_a_cue_file();

    assert_eq!(frozen_ids(&db, &DoctorScopeRequest::WholeLibrary), [1]);
}

#[test]
fn cue_13_a_selection_scope_skips_cue_tracks() {
    let db = library_with_a_cue_file();

    let request = DoctorScopeRequest::Selection {
        track_ids: vec![1, 2, 3],
    };

    assert_eq!(frozen_ids(&db, &request), [1]);
    let only_cue = DoctorScopeRequest::Selection {
        track_ids: vec![2, 3],
    };
    assert_eq!(
        freeze_scope(&db, &only_cue).unwrap(),
        FrozenScope::FallbackRequired
    );
}

#[test]
fn cue_13_the_current_view_scope_skips_cue_tracks() {
    let db = library_with_a_cue_file();
    let snapshot = DoctorViewSnapshot {
        source: crate::view_source::ViewSource::Library,
        sort_field: "title".into(),
        sort_dir: "asc".into(),
        filter: String::new(),
        browse: crate::queries::BrowseFilter::default(),
        queue_ids: Vec::new(),
    };

    let request = DoctorScopeRequest::CurrentView(Box::new(snapshot));

    assert_eq!(frozen_ids(&db, &request), [1]);
}

#[test]
fn cue_13_the_queue_scope_skips_cue_tracks() {
    let db = library_with_a_cue_file();
    let snapshot = DoctorViewSnapshot {
        source: crate::view_source::ViewSource::Queue,
        sort_field: "title".into(),
        sort_dir: "asc".into(),
        filter: String::new(),
        browse: crate::queries::BrowseFilter::default(),
        queue_ids: vec![3, 1, 2],
    };

    let request = DoctorScopeRequest::CurrentView(Box::new(snapshot));

    assert_eq!(frozen_ids(&db, &request), [1]);
}
