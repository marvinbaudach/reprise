use std::path::PathBuf;

fn fixture_copy(dir: &std::path::Path, name: &str) -> PathBuf {
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sine.flac");
    let dst = dir.join(name);
    std::fs::copy(&src, &dst).unwrap();
    dst
}

#[test]
fn diagnose_dir_finds_audio_files() {
    let tmp = tempfile::tempdir().unwrap();
    fixture_copy(tmp.path(), "a.flac");
    fixture_copy(tmp.path(), "b.flac");

    // Create a non-audio file that should be skipped
    std::fs::write(tmp.path().join("readme.txt"), b"not audio").unwrap();

    let results = super::diagnose_dir(tmp.path());
    // sine.flac is a healthy file, so each diagnosis should have no issues
    assert_eq!(results.len(), 2);
    for d in &results {
        assert!(d.issues.is_empty());
    }
}

#[test]
fn diagnose_dir_recurses_subdirectories() {
    let tmp = tempfile::tempdir().unwrap();
    let sub = tmp.path().join("artist/album");
    std::fs::create_dir_all(&sub).unwrap();
    fixture_copy(&sub, "track.flac");

    let results = super::diagnose_dir(tmp.path());
    assert_eq!(results.len(), 1);
}

#[test]
fn diagnose_dir_empty_returns_empty() {
    let tmp = tempfile::tempdir().unwrap();
    let results = super::diagnose_dir(tmp.path());
    assert!(results.is_empty());
}

#[test]
fn diagnose_library_reads_tracks_from_db() {
    let tmp = tempfile::tempdir().unwrap();
    let path = fixture_copy(tmp.path(), "track.flac");

    let conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();

    // Insert a track row pointing at our fixture copy
    conn.execute(
        "INSERT INTO tracks (path, title, added_at) VALUES (?1, ?2, 0)",
        rusqlite::params![path.to_string_lossy(), "Test"],
    )
    .unwrap();

    let results = super::diagnose_library(&conn);
    assert_eq!(results.len(), 1);
    assert!(results[0].issues.is_empty());
}

#[test]
fn diagnose_library_skips_missing_files() {
    let conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();

    conn.execute(
        "INSERT INTO tracks (path, title, added_at) VALUES (?1, ?2, 0)",
        rusqlite::params!["/nonexistent/track.mp3", "Gone"],
    )
    .unwrap();

    // Should not panic; missing file is silently skipped
    let results = super::diagnose_library(&conn);
    assert!(results.is_empty());
}
