use std::path::{Path, PathBuf};

use super::*;

#[test]
fn lyr_7_sidecar_paths_follow_the_source_and_transcoded_device_audio() {
    assert_eq!(
        paths_for_track(
            Path::new("/library/Artist/Album/Song.flac"),
            "Artist/Album/01 Song.opus"
        ),
        Some(LyricsSidecarPaths {
            source_path: PathBuf::from("/library/Artist/Album/Song.lrc"),
            device_path: "Artist/Album/01 Song.lrc".into(),
        })
    );
}

#[test]
fn lyr_7_an_empty_device_path_has_no_attachment_target() {
    assert_eq!(paths_for_track(Path::new("/library/song.flac"), ""), None);
}

#[test]
fn lyr_7_sidecar_detection_is_case_insensitive() {
    assert!(is_sidecar_path(Path::new("Artist/Album/Song.LRC")));
    assert!(!is_sidecar_path(Path::new("Artist/Album/Song.flac")));
}

#[test]
fn lyr_7_the_source_size_is_the_byte_count_of_the_library_sidecar() {
    let library = tempfile::tempdir().unwrap();
    let sidecar = library.path().join("Song.lrc");
    std::fs::write(&sidecar, b"[00:01.00] a line").unwrap();

    assert_eq!(source_file_size(&sidecar), Some(17));
}

#[test]
fn lyr_7_a_missing_sidecar_and_a_directory_both_have_no_source_size() {
    let library = tempfile::tempdir().unwrap();
    let directory = library.path().join("Song.lrc");
    std::fs::create_dir(&directory).unwrap();

    assert_eq!(source_file_size(&library.path().join("Absent.lrc")), None);
    assert_eq!(source_file_size(&directory), None);
}
