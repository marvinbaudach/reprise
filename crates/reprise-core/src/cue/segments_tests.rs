use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::{parse, resolve_file, segments, CueError};

#[test]
fn resolves_an_exact_referenced_file() {
    let files = vec![
        PathBuf::from("/music/album/disc.flac"),
        PathBuf::from("/music/album/disc.wav"),
    ];

    assert_eq!(
        resolve_file(Path::new("/music/album"), "disc.wav", &files),
        Some(PathBuf::from("/music/album/disc.wav"))
    );
}

#[test]
fn resolves_a_referenced_file_case_insensitively() {
    let files = vec![PathBuf::from("/music/album/Disc One.FLAC")];

    assert_eq!(
        resolve_file(Path::new("/music/album"), "disc one.flac", &files),
        Some(PathBuf::from("/music/album/Disc One.FLAC"))
    );
}

#[test]
fn resolves_the_same_stem_after_audio_conversion() {
    let files = vec![
        PathBuf::from("/music/album/other.ape"),
        PathBuf::from("/music/album/Album.flac"),
    ];

    assert_eq!(
        resolve_file(Path::new("/music/album"), "album.wav", &files),
        Some(PathBuf::from("/music/album/Album.flac"))
    );
}

#[test]
fn builds_single_file_segments_and_gives_the_pregap_to_the_previous_track() {
    let sheet = parse(
        br#"REM DATE 1979
REM GENRE "Post-punk"
PERFORMER "Joy Division"
TITLE "Unknown Pleasures"
FILE "album.flac" WAVE
  TRACK 01 AUDIO
    TITLE "Disorder"
    INDEX 01 00:00:00
  TRACK 02 AUDIO
    TITLE "Day of the Lords"
    INDEX 00 00:59:50
    INDEX 01 01:00:00
  TRACK 03 AUDIO
    TITLE "Candidate"
    INDEX 01 02:00:00
"#,
    )
    .expect("valid sheet");
    let path = PathBuf::from("/music/album.flac");
    let durations = HashMap::from([(path.clone(), 180_000)]);

    let result = segments(&sheet, &durations).expect("valid segments");

    assert_eq!(result.len(), 3);
    assert_eq!(result[0].path, path);
    assert_eq!(
        (
            result[0].segment_index,
            result[0].start_ms,
            result[0].end_ms
        ),
        (1, 0, 60_000)
    );
    assert_eq!(
        (
            result[1].segment_index,
            result[1].start_ms,
            result[1].end_ms
        ),
        (2, 60_000, 120_000)
    );
    assert_eq!(
        (
            result[2].segment_index,
            result[2].start_ms,
            result[2].end_ms
        ),
        (3, 120_000, 180_000)
    );
    assert_eq!(result[1].title, "Day of the Lords");
    assert_eq!(result[1].performer, "Joy Division");
    assert_eq!(result[1].track_no, 2);
    assert_eq!(result[1].album, "Unknown Pleasures");
    assert_eq!(result[1].album_artist, "Joy Division");
    assert_eq!(result[1].date.as_deref(), Some("1979"));
    assert_eq!(result[1].genre.as_deref(), Some("Post-punk"));
}

#[test]
fn segment_indices_stay_unique_when_a_path_has_multiple_file_blocks() {
    let sheet = parse(
        br#"FILE "album.flac" WAVE
  TRACK 01 AUDIO
    INDEX 01 00:00:00
FILE "album.flac" WAVE
  TRACK 02 AUDIO
    INDEX 01 01:00:00
"#,
    )
    .expect("valid repeated file reference");
    let durations = HashMap::from([(PathBuf::from("/music/album.flac"), 180_000)]);

    let result = segments(&sheet, &durations).expect("valid segments");

    assert_eq!(result[0].segment_index, 1);
    assert_eq!(result[1].segment_index, 2);
}

#[test]
fn multi_file_segments_reset_the_index_for_each_path() {
    let sheet = parse(
        br#"FILE "cd1.flac" WAVE
  TRACK 01 AUDIO
    INDEX 01 00:00:00
  TRACK 02 AUDIO
    INDEX 01 01:00:00
FILE "cd2.flac" WAVE
  TRACK 03 AUDIO
    INDEX 01 00:00:00
"#,
    )
    .expect("valid multi-file sheet");
    let first = PathBuf::from("/music/CD1.flac");
    let second = PathBuf::from("/music/cd2.flac");
    let durations = HashMap::from([(first.clone(), 120_000), (second.clone(), 180_000)]);

    let result = segments(&sheet, &durations).expect("valid segments");

    assert_eq!(
        result
            .iter()
            .map(|item| item.segment_index)
            .collect::<Vec<_>>(),
        vec![1, 2, 1]
    );
    assert_eq!(
        result.iter().map(|item| &item.path).collect::<Vec<_>>(),
        vec![&first, &first, &second]
    );
}

#[test]
fn rejects_an_index_past_the_audio_duration() {
    let sheet = parse(
        br#"FILE "album.flac" WAVE
  TRACK 01 AUDIO
    INDEX 01 00:02:00
"#,
    )
    .expect("parseable sheet");
    let path = PathBuf::from("/music/album.flac");
    let durations = HashMap::from([(path.clone(), 1_999)]);

    let error = segments(&sheet, &durations).expect_err("index past duration");

    assert_eq!(
        error,
        CueError::IndexPastEnd {
            path,
            track: 1,
            position_ms: 2_000,
            duration_ms: 1_999,
        }
    );
}

#[test]
fn reports_a_file_without_a_duration() {
    let sheet = parse(
        br#"FILE "missing.flac" WAVE
  TRACK 01 AUDIO
    INDEX 01 00:00:00
"#,
    )
    .expect("parseable sheet");

    let error = segments(&sheet, &HashMap::new()).expect_err("missing duration");

    assert_eq!(
        error,
        CueError::MissingDuration {
            file: "missing.flac".to_owned(),
        }
    );
}

#[test]
fn unsupported_same_stem_files_do_not_resolve() {
    let files = vec![PathBuf::from("/music/album/album.ape")];

    assert_eq!(
        resolve_file(Path::new("/music/album"), "album.wav", &files),
        None
    );
}
