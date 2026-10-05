//! A track cut from a CUE sheet is read-only: its tags live in the sheet, and
//! writing the file would change every track in it.

use std::path::Path;

use crate::db::Db;

fn seeded() -> Db {
    let db = Db::open_in_memory().unwrap();
    db.conn()
        .execute_batch(
            "INSERT INTO tracks (id, path, title, added_at, segment_index, segment_start_ms,
                                 segment_end_ms)
             VALUES (20, '/m/live.flac', 'Two', 1, 2, 400, 900),
                    (40, '/m/plain.flac', 'Plain', 1, 0, NULL, NULL);",
        )
        .unwrap();
    db
}

#[test]
fn cue_5_a_cue_track_has_no_tag_edit_seed() {
    let db = seeded();

    assert!(super::tag_edit_seed::track_edit_seed_by_id(&db, 20)
        .unwrap()
        .is_none());
    assert!(
        super::tag_edit_seed::live_track_edit_seed_by_path(&db, "/m/live.flac")
            .unwrap()
            .is_none()
    );
    assert!(super::tag_edit_seed::track_edit_seed_by_id(&db, 40)
        .unwrap()
        .is_some());
}

#[test]
fn cue_5_a_cue_track_does_not_pass_tag_write_validation() {
    let db = seeded();

    let refused =
        super::tag_mutation::validate_registered_track(db.conn(), 20, Path::new("/m/live.flac"));
    let allowed =
        super::tag_mutation::validate_registered_track(db.conn(), 40, Path::new("/m/plain.flac"));

    assert!(refused.unwrap_err().contains("CUE"));
    assert!(allowed.is_ok());
}
