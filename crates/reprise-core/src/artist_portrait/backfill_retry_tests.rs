use std::fs::{File, FileTimes};
use std::time::{Duration, UNIX_EPOCH};

use super::*;

fn library_with_artist(name: &str) -> Db {
    let db = Db::open_in_memory().unwrap();
    db.conn()
        .execute(
            "INSERT INTO tracks (path, title, artist, album_artist, added_at) VALUES ('/track', 'Track', ?1, ?1, 0)",
            [name],
        )
        .unwrap();
    db
}

#[test]
fn net_7e_a_fresh_retry_marker_skips_the_artist_and_an_expired_one_does_not() {
    let db = library_with_artist("Rejected artist");
    let cache = tempfile::tempdir().unwrap();
    cache::write_retry(cache.path(), "Rejected artist");
    let marker = cache::retry_marker_path(cache.path(), "Rejected artist");

    assert!(
        pending_artists(&db, cache.path(), chrono::Utc::now().timestamp())
            .unwrap()
            .is_empty()
    );

    File::options()
        .write(true)
        .open(&marker)
        .unwrap()
        .set_times(FileTimes::new().set_modified(
            UNIX_EPOCH
                + Duration::from_secs(
                    (chrono::Utc::now().timestamp() - cache::RETRY_TTL_SECONDS - 2) as u64,
                ),
        ))
        .unwrap();
    assert_eq!(
        pending_artists(&db, cache.path(), chrono::Utc::now().timestamp()).unwrap(),
        vec!["Rejected artist"]
    );
}

#[test]
fn net_7e_positive_and_negative_results_remove_retry_markers_and_pruning_removes_orphans() {
    let db = library_with_artist("Here");
    let cache = tempfile::tempdir().unwrap();
    cache::write_retry(cache.path(), "Here");
    cache::store_image(cache.path(), "Here", b"image", "jpg");
    assert!(!cache::retry_marker_path(cache.path(), "Here").exists());

    cache::write_retry(cache.path(), "Here");
    cache::write_negative(cache.path(), "Here");
    assert!(!cache::retry_marker_path(cache.path(), "Here").exists());

    cache::write_retry(cache.path(), "Gone");
    pending_artists(&db, cache.path(), chrono::Utc::now().timestamp()).unwrap();
    assert!(!cache::retry_marker_path(cache.path(), "Gone").exists());
}
