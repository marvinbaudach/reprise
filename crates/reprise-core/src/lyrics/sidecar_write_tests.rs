use std::fs;

use tempfile::TempDir;

use super::*;
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

#[cfg(unix)]
#[test]
fn lyr_7_a_write_failure_leaves_no_temporary_file() {
    use std::os::unix::fs::PermissionsExt;

    let temp = TempDir::new().unwrap();
    let track = temp.path().join("song.flac");
    fs::write(&track, b"fixture").unwrap();
    fs::set_permissions(temp.path(), fs::Permissions::from_mode(0o555)).unwrap();

    let result = write_sidecar(&track, &[TimedLine::new(1_000, "Network text")]);

    fs::set_permissions(temp.path(), fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(result, SidecarWrite::Failed);
    assert_eq!(
        fs::read_dir(temp.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>(),
        [track.file_name().unwrap()]
    );
}
