//! Catalog-sync and announcement coherence for deliberate deletion memory.

use chrono::NaiveDate;

use crate::artist_news::{
    delta_candidates, refresh_with, refresh_with_progress_at, unseen_release_count, FetchScope,
    RefreshHooks,
};

const ARTIST_ID: &str = "83d91898-7763-47d7-b03b-b92132375c47";
const SECOND_ARTIST_ID: &str = "11111111-1111-1111-1111-111111111111";
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

#[test]
fn nr_32_progress_never_exposes_a_fetched_remembered_gap() {
    let db = database_with_deletion_memory();
    let mut fetch = |_url: &str| Ok(DELETED_RELEASE.to_string());
    let mut observed_hidden = Vec::new();
    let mut completion_time = || 1_001;

    refresh_with_progress_at(
        &db,
        date(),
        1_000,
        FetchScope::TopArtists,
        true,
        &mut RefreshHooks {
            fetch: &mut fetch,
            on_progress: &mut |progress: crate::artist_news::RefreshProgress| {
                if progress.checked == 0 {
                    return;
                }
                observed_hidden.push(
                    db.conn()
                        .query_row(
                            "SELECT hidden FROM new_releases
                             WHERE release_group_mbid = 'deleted'",
                            [],
                            |row| row.get::<_, bool>(0),
                        )
                        .unwrap(),
                );
            },
            completion_time: &mut completion_time,
        },
    )
    .unwrap();

    assert_eq!(observed_hidden, [true]);
}

#[test]
fn nr_32_mid_refresh_database_error_still_reconciles_committed_rows() {
    let db = database_with_deletion_memory();
    db.conn()
        .execute_batch(
            "CREATE TRIGGER fail_release_ledger_write
             BEFORE INSERT ON artist_news_fetch
             BEGIN
               SELECT RAISE(ABORT, 'injected ledger failure');
             END;",
        )
        .unwrap();
    let mut fetch = |_url: &str| Ok(DELETED_RELEASE.to_string());

    assert!(refresh_with(&db, date(), 1_000, FetchScope::TopArtists, true, &mut fetch,).is_err());

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
fn nr_32_refresh_runs_one_full_memory_reconciliation_for_all_artists() {
    let db = database_with_deletion_memory();
    db.conn()
        .execute(
            "INSERT INTO tracks (
               id, path, title, artist, album_artist, artist_mbid, album, play_count, added_at
             ) VALUES (3, '/music/second.flac', 'Second Song', 'Second Artist',
                       'Second Artist', ?1, 'Second Album', 10, 0)",
            [SECOND_ARTIST_ID],
        )
        .unwrap();
    let mut fetch = |url: &str| {
        if url.contains(SECOND_ARTIST_ID) {
            Ok(r#"{"release-groups":[]}"#.to_string())
        } else {
            Ok(DELETED_RELEASE.to_string())
        }
    };
    crate::deleted_releases::reset_full_reconciliation_call_count();

    let report =
        refresh_with(&db, date(), 1_000, FetchScope::TopArtists, true, &mut fetch).unwrap();

    assert_eq!(report.artists_fetched, 2);
    assert_eq!(crate::deleted_releases::full_reconciliation_call_count(), 1);
}
