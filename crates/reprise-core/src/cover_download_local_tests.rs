use super::*;

#[test]
fn local_marker_is_atomic_fresh_and_expires_after_the_keyed_ttl() {
    let cache = tempfile::tempdir().unwrap();
    let artist = "Local band";
    let album = "Local album";
    let key = album_key(artist, album);
    let dir = downloaded_dir_in(cache.path());
    let marker = dir.join(format!("{key}.local"));

    assert!(!local_marker_is_fresh_in(cache.path(), artist, album));
    write_local_marker_in(cache.path(), artist, album);
    assert!(marker.is_file());
    assert!(local_marker_is_fresh_in(cache.path(), artist, album));

    let expired = SystemTime::now() - negative_marker_ttl(&key) - Duration::from_secs(1);
    std::fs::File::options()
        .write(true)
        .open(&marker)
        .unwrap()
        .set_times(std::fs::FileTimes::new().set_modified(expired))
        .unwrap();
    assert!(!local_marker_is_fresh_in(cache.path(), artist, album));
}

#[test]
fn album_state_lookup_uses_only_downloaded_files_and_negative_markers() {
    let cache = tempfile::tempdir().unwrap();
    let key = album_key("Band", "Album");
    let dir = downloaded_dir_in(cache.path());
    std::fs::create_dir_all(&dir).unwrap();

    assert_eq!(
        album_cover_state_in(cache.path(), "Band", "Album"),
        CoverState::Unknown
    );
    std::fs::write(negative_marker_path_in(&dir, &key), b"").unwrap();
    assert_eq!(
        album_cover_state_in(cache.path(), "Band", "Album"),
        CoverState::KnownMissing
    );
}
