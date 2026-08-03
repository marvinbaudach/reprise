use std::fs;

use tempfile::TempDir;

use super::*;
use crate::library::source::ExistingPathSource;
use crate::lyrics::parse_lrc;

#[test]
fn lyr_7_synced_lines_are_written_as_round_trippable_standard_lrc() {
    let temp = TempDir::new().unwrap();
    let track = temp.path().join("song.flac");
    fs::write(&track, b"fixture").unwrap();
    let lines = vec![
        TimedLine::new(1_230, "First line"),
        TimedLine::new(62_340, "Second line"),
    ];

    assert_eq!(write_sidecar(&track, &lines), SidecarWrite::Written);
    let contents = fs::read_to_string(track.with_extension("lrc")).unwrap();
    assert_eq!(contents, "[00:01.23]First line\n[01:02.34]Second line\n");
    assert_eq!(parse_lrc(&contents), lines);
}

#[test]
fn lyr_7_an_existing_sidecar_is_never_overwritten() {
    let temp = TempDir::new().unwrap();
    let track = temp.path().join("song.flac");
    let sidecar = track.with_extension("lrc");
    fs::write(&track, b"fixture").unwrap();
    fs::write(&sidecar, b"[00:00.00]User text\n").unwrap();

    assert_eq!(
        write_sidecar(&track, &[TimedLine::new(1_000, "Network text")]),
        SidecarWrite::AlreadyPresent
    );
    assert_eq!(
        fs::read(&sidecar).unwrap(),
        b"[00:00.00]User text\n",
        "the user's sidecar must remain byte-for-byte unchanged"
    );
}

#[test]
fn lyr_7_a_missing_music_file_is_not_an_applicable_write_target() {
    let temp = TempDir::new().unwrap();
    let track = temp.path().join("missing.flac");

    assert_eq!(
        write_sidecar(&track, &[TimedLine::new(1_000, "Network text")]),
        SidecarWrite::NotApplicable
    );
    assert!(!track.with_extension("lrc").exists());
}

#[test]
fn lyr_7_a_track_with_a_very_long_filename_still_gets_a_sidecar() {
    let temp = TempDir::new().unwrap();
    let track = temp.path().join(format!("{}.flac", "a".repeat(245)));
    fs::write(&track, b"fixture").unwrap();

    assert_eq!(
        write_sidecar(&track, &[TimedLine::new(1_000, "Network text")]),
        SidecarWrite::Written
    );
    assert!(track.with_extension("lrc").is_file());
}

#[test]
fn lyr_7_a_write_failure_leaves_no_temporary_file() {
    let temp = TempDir::new().unwrap();
    let track = temp.path().join("missing-parent/song.flac");

    let result = write_sidecar_with_source(
        &ExistingPathSource::FILE,
        &track,
        &[TimedLine::new(1_000, "Network text")],
    );

    assert_eq!(result, SidecarWrite::Failed);
    assert!(fs::read_dir(temp.path()).unwrap().next().is_none());
}
