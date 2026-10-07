use std::path::{Path, PathBuf};

use super::*;
use crate::device_sync::cue_files::CueSyncTrack;

fn album() -> CueSyncFile {
    let track = |id: i64, start_ms: i64, end_ms: i64, title: &str, performer: &str| CueSyncTrack {
        track_id: id,
        start_ms,
        end_ms,
        title: title.to_owned(),
        performer: performer.to_owned(),
        track_no: Some(u32::try_from(id).unwrap()),
    };
    CueSyncFile {
        source_path: PathBuf::from("/music/Band/Album/album.flac"),
        album: "The \"Best\" Album".to_owned(),
        album_artist: "Band".to_owned(),
        year: Some(1979),
        genre: "Post-punk".to_owned(),
        duration_ms: 30_000,
        tracks: vec![
            track(1, 0, 10_013, "Disorder", "Band"),
            track(2, 10_013, 20_000, "Day of the Lords", "Guest"),
            track(3, 20_000, 30_000, "Candidate", "Band"),
        ],
    }
}

#[test]
fn cue_15_the_derived_sheet_sits_beside_its_device_file() {
    let path = sheet_path("Band/Album/album.opus", "FILE");
    assert!(path.starts_with("Band/Album/album."), "{path}");
    assert!(path.ends_with(".cue"));
    assert!(describes(&path, "Band/Album/album.opus"));
    assert!(!describes(&path, "Band/Album/other.opus"));
    assert!(!describes("Band/Album/album.cue", "Band/Album/album.opus"));
    assert!(sheet_path("Band/Album.x/album", "FILE").starts_with("Band/Album.x/album."));
    assert_eq!(
        sheet_path("a/b.opus", "FILE"),
        sheet_path("a/b.opus", "FILE"),
        "the name is stable"
    );
    assert_ne!(
        sheet_path("a/b.opus", "FILE"),
        sheet_path("a/b.opus", "FILF")
    );
}

#[test]
fn cue_15_the_derived_sheet_cuts_the_device_file_where_the_desktop_did() {
    let file = album();
    let device_path = "Band/The _Best_ Album/album.opus";

    let text = render(&file, device_path);
    let sheet = crate::cue::parse(text.as_bytes()).unwrap();

    assert_eq!(sheet.files.len(), 1);
    assert_eq!(sheet.files[0].name, "album.opus");
    assert_eq!(sheet.title, "The \"Best\" Album");
    assert_eq!(sheet.performer, "Band");
    assert_eq!(sheet.date.as_deref(), Some("1979"));
    let directory = Path::new("/sdcard/Music/Reprise/Band/The _Best_ Album");
    let device_file = directory.join("album.opus");
    let resolved = crate::cue::resolve_file(
        directory,
        &sheet.files[0].name,
        std::slice::from_ref(&device_file),
    );
    assert_eq!(resolved.as_ref(), Some(&device_file));
    let segments =
        crate::cue::segments(&sheet, |_| Some((device_file.clone(), file.duration_ms))).unwrap();
    let cut: Vec<(i64, i64, &str, &str)> = segments
        .iter()
        .map(|segment| {
            (
                segment.start_ms,
                segment.end_ms,
                segment.title.as_str(),
                segment.performer.as_str(),
            )
        })
        .collect();
    assert_eq!(
        cut,
        [
            (0, 10_013, "Disorder", "Band"),
            (10_013, 20_000, "Day of the Lords", "Guest"),
            (20_000, 30_000, "Candidate", "Band"),
        ]
    );
}

#[test]
fn cue_15_every_start_a_sheet_can_hold_survives_the_round_trip() {
    // Every start the desktop can hold came from a frame; writing it back must
    // name that same frame, or the phone would report another start (CUE-17).
    for frame in (0..FRAMES_PER_SECOND * SECONDS_PER_MINUTE * 80).step_by(7) {
        let start_ms = frame * MS_PER_SECOND / FRAMES_PER_SECOND;
        let text = format!(
            "FILE \"a.wav\" WAVE\n  TRACK 01 AUDIO\n    INDEX 01 {}\n",
            index_time(start_ms)
        );
        let sheet = crate::cue::parse(text.as_bytes()).unwrap();
        let read_back = crate::cue::segments(&sheet, |_| {
            Some((PathBuf::from("a.wav"), i64::MAX / FRAMES_PER_SECOND))
        })
        .unwrap()[0]
            .start_ms;
        assert_eq!(read_back, start_ms, "frame {frame}");
    }
}
