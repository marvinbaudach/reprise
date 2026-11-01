use std::cell::Cell;

use super::*;
use crate::cover_download::{album_key, downloaded_dir_in, negative_marker_path_in};

fn unresolved_calls(
    cache: &Path,
    artist: &str,
    album: &str,
    resolves: bool,
) -> (CoverAlbumTriage, u32) {
    let calls = Cell::new(0_u32);
    let triage = triage_album_in(cache, artist, album, || {
        calls.set(calls.get() + 1);
        resolves
    });
    (triage, calls.get())
}

#[test]
fn net_7e_triage_settles_cached_and_missing_albums_without_reading_tags() {
    let cache = tempfile::tempdir().unwrap();
    let dir = downloaded_dir_in(cache.path());
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join(format!("{}.jpg", album_key("Band", "Cached"))),
        b"x",
    )
    .unwrap();
    std::fs::write(
        negative_marker_path_in(&dir, &album_key("Band", "Missing")),
        b"",
    )
    .unwrap();

    assert_eq!(
        unresolved_calls(cache.path(), "Band", "Cached", true),
        (CoverAlbumTriage::Cached, 0)
    );
    assert_eq!(
        unresolved_calls(cache.path(), "Band", "Missing", true),
        (CoverAlbumTriage::KnownMissing, 0)
    );
}

#[test]
fn net_7e_triage_records_local_art_once_and_then_settles_it_by_stat() {
    let cache = tempfile::tempdir().unwrap();

    assert_eq!(
        unresolved_calls(cache.path(), "Band", "Local", true),
        (CoverAlbumTriage::LocalResolved, 1)
    );
    assert_eq!(
        unresolved_calls(cache.path(), "Band", "Local", true),
        (CoverAlbumTriage::LocalMarker, 0)
    );
    assert_eq!(
        stat_only_triage_in(cache.path(), "Band", "Local"),
        Some(CoverAlbumTriage::LocalMarker)
    );
}

#[test]
fn net_7e_triage_asks_the_network_only_for_albums_without_local_art() {
    let cache = tempfile::tempdir().unwrap();

    assert_eq!(
        unresolved_calls(cache.path(), "Band", "Bare", false),
        (CoverAlbumTriage::NeedsRequest, 1)
    );
    assert_eq!(stat_only_triage_in(cache.path(), "Band", "Bare"), None);
}
