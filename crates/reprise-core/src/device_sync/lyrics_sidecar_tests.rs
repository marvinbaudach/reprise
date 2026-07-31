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
