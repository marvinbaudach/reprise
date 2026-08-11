//! Catalog-sync and announcement coherence for deliberate deletion memory.

use chrono::NaiveDate;

use crate::artist_news::{delta_candidates, refresh_with, unseen_release_count, FetchScope};

const ARTIST_ID: &str = "83d91898-7763-47d7-b03b-b92132375c47";
const DELETED_RELEASE: &str = r#"{"release-groups":[
  {"id":"deleted","title":"Deleted Album","first-release-date":"2026-08-01","primary-type":"Album","secondary-types":[]}
]}"#;

fn date() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 7, 13).unwrap()
}

fn database_with_deletion_memory() -> crate::db::Db {
    let db = crate::db::Db::open_in_memory().unwrap();
    db.conn()
        .execute(
            "INSERT INTO tracks (
               id, path, title, artist, album_artist, artist_mbid, album, play_count, added_at
             ) VALUES
               (1, '/music/deleted.flac', 'Deleted Song', 'Artist', 'Artist', ?1,
                'Deleted Album', 1, 0),
               (2, '/music/anchor.flac', 'Library Anchor', 'Artist', 'Artist', ?1,
                'Owned Album', 20, 0)",
            [ARTIST_ID],
        )
        .unwrap();
    crate::deleted_releases::remember_deleted_releases(db.conn(), &[1], 100).unwrap();
    db.conn()
        .execute("DELETE FROM tracks WHERE id = 1", [])
        .unwrap();
    crate::deleted_releases::apply_deleted_release_memory(db.conn()).unwrap();
    db
}

fn fetch_deleted_release(db: &crate::db::Db) {
    let mut fetch = |_url: &str| Ok(DELETED_RELEASE.to_string());
    let report = refresh_with(db, date(), 1_000, FetchScope::TopArtists, true, &mut fetch).unwrap();
    assert_eq!(report.releases_upserted, 1);
}

#[test]
fn nr_32_memory_applies_to_a_release_fetched_later() {
    let db = database_with_deletion_memory();

    fetch_deleted_release(&db);

    let hidden: bool = db
        .conn()
        .query_row(
            "SELECT hidden FROM new_releases WHERE release_group_mbid = 'deleted'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(hidden);
}

#[test]
fn nr_32_badge_and_popover_follow_the_memory() {
    let db = database_with_deletion_memory();

    fetch_deleted_release(&db);

    assert!(delta_candidates(&db, date()).unwrap().is_empty());
    assert_eq!(unseen_release_count(&db, date()).unwrap(), 0);
}
