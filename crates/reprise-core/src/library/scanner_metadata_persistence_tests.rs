//! Scanner metadata persistence and move-reconciliation coverage.
//!
//! Kept separate from `scanner_tests.rs` so the combined main/album-grid
//! test suite remains below the repository's 800-line source-file limit.

use super::tests::{completed, fixture_copy};
use super::*;
use lofty::prelude::*;
use lofty::tag::{Tag, TagType};

#[test]
fn scan_persists_musicbrainz_artist_id() {
    let tmp = tempfile::tempdir().unwrap();
    let file = fixture_copy(tmp.path(), "tagged.flac");
    let mut tag = Tag::new(TagType::VorbisComments);
    tag.set_artist("Tagged Artist".into());
    tag.insert_text(
        lofty::tag::ItemKey::MusicBrainzArtistId,
        "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa".into(),
    );
    tag.save_to_path(&file, lofty::config::WriteOptions::default())
        .unwrap();

    let mut conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    let report = completed(scan_folder(&mut conn, tmp.path()).unwrap());
    assert_eq!(report.added, 1);

    let stored: (Option<String>, i64) = conn
        .query_row(
            "SELECT artist_mbid, artist_mbid_negative FROM tracks WHERE path = ?1",
            [file.to_string_lossy().to_string()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(
        stored,
        (Some("aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa".into()), 0)
    );
}

#[test]
fn scan_persists_disc_number_and_move_reconcile_preserves_it() {
    let tmp = tempfile::tempdir().unwrap();
    let original = fixture_copy(tmp.path(), "disc-two.flac");
    let mut tag = Tag::new(TagType::VorbisComments);
    tag.set_title("Second Disc".into());
    tag.set_artist("Artist".into());
    tag.set_album("Album".into());
    tag.set_track(1);
    tag.set_disk(2);
    tag.save_to_path(&original, lofty::config::WriteOptions::default())
        .unwrap();

    let mut conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    completed(scan_folder(&mut conn, tmp.path()).unwrap());
    let imported_disc: Option<i32> = conn
        .query_row(
            "SELECT disc_no FROM tracks WHERE path = ?1",
            [original.to_string_lossy().to_string()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(imported_disc, Some(2));

    let moved = tmp.path().join("disc-two-moved.flac");
    std::fs::rename(&original, &moved).unwrap();
    let report = completed(scan_folder(&mut conn, tmp.path()).unwrap());
    assert_eq!(report.moved, 1);
    let moved_disc: Option<i32> = conn
        .query_row(
            "SELECT disc_no FROM tracks WHERE path = ?1",
            [moved.to_string_lossy().to_string()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(moved_disc, Some(2));
}
