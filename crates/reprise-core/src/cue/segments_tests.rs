use std::path::{Path, PathBuf};

use super::{parse, resolve_file, segments, CueError, CueFile, CueSegment, CueSheet};

/// A resolver as the scanner would build it: names resolve against the files
/// in `/m`, durations are looked up by the resolved path.
fn resolver(files: &[(&str, i64)]) -> impl Fn(&CueFile) -> Option<(PathBuf, i64)> {
    let files: Vec<(PathBuf, i64)> = files
        .iter()
        .map(|(path, duration)| (PathBuf::from(path), *duration))
        .collect();
    move |file| {
        let existing: Vec<PathBuf> = files.iter().map(|(path, _)| path.clone()).collect();
        let path = resolve_file(Path::new("/m"), &file.name, &existing)?;
        let duration = files.iter().find(|(candidate, _)| *candidate == path)?.1;
        Some((path, duration))
    }
}

fn run(sheet: &CueSheet, files: &[(&str, i64)]) -> Result<Vec<CueSegment>, CueError> {
    segments(sheet, resolver(files))
}

fn spans(result: &[CueSegment]) -> Vec<(u32, i64, i64)> {
    result
        .iter()
        .map(|segment| (segment.track_no, segment.start_ms, segment.end_ms))
        .collect()
}

#[test]
fn resolves_an_exact_referenced_file() {
    let files = vec![PathBuf::from("/m/disc.flac"), PathBuf::from("/m/disc.wav")];

    assert_eq!(
        resolve_file(Path::new("/m"), "disc.wav", &files),
        Some(PathBuf::from("/m/disc.wav"))
    );
}

#[test]
fn resolves_a_referenced_file_case_insensitively() {
    let files = vec![PathBuf::from("/m/Disc One.FLAC")];

    assert_eq!(
        resolve_file(Path::new("/m"), "disc one.flac", &files),
        Some(PathBuf::from("/m/Disc One.FLAC"))
    );
}

#[test]
fn resolves_the_same_stem_after_audio_conversion() {
    let files = vec![
        PathBuf::from("/m/other.ape"),
        PathBuf::from("/m/Album.flac"),
    ];

    assert_eq!(
        resolve_file(Path::new("/m"), "album.wav", &files),
        Some(PathBuf::from("/m/Album.flac"))
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
    let path = PathBuf::from("/m/album.flac");

    let result = run(&sheet, &[("/m/album.flac", 180_000)]).expect("valid segments");

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
fn rejects_two_file_blocks_that_resolve_to_the_same_path() {
    let sheet = parse(
        br#"FILE "album.flac" WAVE
  TRACK 01 AUDIO
    INDEX 01 00:00:00
FILE "Album.WAV" WAVE
  TRACK 02 AUDIO
    INDEX 01 00:01:00
"#,
    )
    .expect("parseable sheet");

    let error = run(&sheet, &[("/m/album.flac", 180_000)]).expect_err("overlapping blocks");

    assert_eq!(
        error,
        CueError::DuplicateFile {
            path: PathBuf::from("/m/album.flac"),
        }
    );
}

#[test]
fn segments_follow_the_resolved_path_not_the_referenced_name() {
    let sheet = parse(
        br#"FILE "Album.wav" WAVE
  TRACK 01 AUDIO
    INDEX 01 00:00:00
  TRACK 02 AUDIO
    INDEX 01 01:00:00
"#,
    )
    .expect("valid sheet");

    let result = run(&sheet, &[("/m/Album.flac", 180_000)]).expect("resolves by stem");

    assert_eq!(result[0].path, PathBuf::from("/m/Album.flac"));
    assert_eq!(spans(&result), vec![(1, 0, 60_000), (2, 60_000, 180_000)]);
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
    let first = PathBuf::from("/m/CD1.flac");
    let second = PathBuf::from("/m/cd2.flac");

    let result = run(
        &sheet,
        &[("/m/CD1.flac", 120_000), ("/m/cd2.flac", 180_000)],
    )
    .expect("valid segments");

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
fn rejects_an_index_at_or_past_the_audio_duration() {
    let sheet = parse(
        br#"FILE "album.flac" WAVE
  TRACK 01 AUDIO
    INDEX 01 00:02:00
"#,
    )
    .expect("parseable sheet");

    for duration_ms in [0, 1_999, 2_000] {
        let error = run(&sheet, &[("/m/album.flac", duration_ms)]).expect_err("index at end");

        assert_eq!(
            error,
            CueError::IndexPastEnd {
                path: PathBuf::from("/m/album.flac"),
                track: 1,
                position_ms: 2_000,
                duration_ms,
            }
        );
    }
    assert!(run(&sheet, &[("/m/album.flac", 2_001)]).is_ok());
}

#[test]
fn a_zero_duration_rejects_even_an_index_at_zero() {
    let sheet = parse(
        br#"FILE "album.flac" WAVE
  TRACK 01 AUDIO
    INDEX 01 00:00:00
"#,
    )
    .expect("parseable sheet");

    let error = run(&sheet, &[("/m/album.flac", 0)]).expect_err("empty audio");

    assert!(matches!(
        error,
        CueError::IndexPastEnd { position_ms: 0, .. }
    ));
}

#[test]
fn rejects_a_negative_duration() {
    let sheet = parse(
        br#"FILE "album.flac" WAVE
  TRACK 01 AUDIO
    INDEX 01 00:00:00
"#,
    )
    .expect("parseable sheet");

    let error = run(&sheet, &[("/m/album.flac", -1)]).expect_err("negative duration");

    assert_eq!(
        error,
        CueError::InvalidDuration {
            path: PathBuf::from("/m/album.flac"),
            duration_ms: -1,
        }
    );
}

#[test]
fn every_file_ends_its_last_track_at_its_own_duration() {
    let sheet = parse(
        br#"FILE "cd1.flac" WAVE
  TRACK 01 AUDIO
    INDEX 01 00:00:00
  TRACK 02 AUDIO
    INDEX 01 01:00:00
FILE "cd2.flac" WAVE
  TRACK 03 AUDIO
    INDEX 01 00:00:00
  TRACK 04 AUDIO
    INDEX 01 00:30:00
"#,
    )
    .expect("valid multi-file sheet");

    let result =
        run(&sheet, &[("/m/cd1.flac", 150_000), ("/m/cd2.flac", 90_000)]).expect("valid segments");

    assert_eq!(
        spans(&result),
        vec![
            (1, 0, 60_000),
            (2, 60_000, 150_000),
            (3, 0, 30_000),
            (4, 30_000, 90_000)
        ]
    );
}

#[test]
fn one_missing_file_fails_the_whole_sheet() {
    let sheet = parse(
        br#"FILE "cd1.flac" WAVE
  TRACK 01 AUDIO
    INDEX 01 00:00:00
FILE "cd2.flac" WAVE
  TRACK 02 AUDIO
    INDEX 01 00:00:00
"#,
    )
    .expect("valid multi-file sheet");

    let error = run(&sheet, &[("/m/cd1.flac", 60_000)]).expect_err("cd2 is missing");

    assert_eq!(
        error,
        CueError::MissingDuration {
            file: "cd2.flac".to_owned(),
        }
    );
}

#[test]
fn a_data_track_produces_no_segment_but_ends_the_previous_audio_track() {
    let sheet = parse(
        br#"FILE "album.flac" WAVE
  TRACK 01 AUDIO
    INDEX 01 00:00:00
  TRACK 02 AUDIO
    INDEX 01 01:00:00
  TRACK 03 MODE1/2352
    INDEX 01 02:00:00
"#,
    )
    .expect("valid sheet");

    let result = run(&sheet, &[("/m/album.flac", 180_000)]).expect("valid segments");

    assert_eq!(spans(&result), vec![(1, 0, 60_000), (2, 60_000, 120_000)]);
    assert_eq!(
        result
            .iter()
            .map(|item| item.segment_index)
            .collect::<Vec<_>>(),
        vec![1, 2]
    );
}

#[test]
fn a_file_holding_only_data_tracks_needs_no_resolution() {
    let sheet = parse(
        br#"FILE "album.flac" WAVE
  TRACK 01 AUDIO
    INDEX 01 00:00:00
FILE "data.bin" BINARY
  TRACK 02 MODE1/2352
    INDEX 01 00:00:00
"#,
    )
    .expect("valid sheet");

    let result = run(&sheet, &[("/m/album.flac", 60_000)]).expect("valid segments");

    assert_eq!(spans(&result), vec![(1, 0, 60_000)]);
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

    let error = run(&sheet, &[]).expect_err("missing duration");

    assert_eq!(
        error,
        CueError::MissingDuration {
            file: "missing.flac".to_owned(),
        }
    );
}

#[test]
fn unsupported_same_stem_files_do_not_resolve() {
    let files = vec![PathBuf::from("/m/album.ape")];

    assert_eq!(resolve_file(Path::new("/m"), "album.wav", &files), None);
}

fn paths(names: &[&str]) -> Vec<PathBuf> {
    names.iter().map(PathBuf::from).collect()
}

#[test]
fn an_exact_match_beats_a_case_insensitive_one() {
    let files = paths(&["/m/ALBUM.flac", "/m/album.flac"]);

    assert_eq!(
        resolve_file(Path::new("/m"), "album.flac", &files),
        Some(PathBuf::from("/m/album.flac"))
    );
}

#[test]
fn case_insensitive_ties_go_to_the_sorted_first_path() {
    let files = paths(&["/m/b.FLAC", "/m/B.flac"]);

    assert_eq!(
        resolve_file(Path::new("/m"), "b.flac", &files),
        Some(PathBuf::from("/m/B.flac"))
    );
}

#[test]
fn the_extension_fallback_prefers_flac_then_wav_then_the_rest() {
    let all = paths(&["/m/a.mp3", "/m/a.wav", "/m/a.flac", "/m/a.m4a"]);
    let wav_up = paths(&["/m/a.mp3", "/m/a.WAV", "/m/a.m4a"]);
    let rest = paths(&["/m/a.mp3", "/m/a.m4a", "/m/a.ogg"]);

    assert_eq!(
        resolve_file(Path::new("/m"), "a.ape", &all),
        Some(PathBuf::from("/m/a.flac"))
    );
    assert_eq!(
        resolve_file(Path::new("/m"), "a.ape", &wav_up),
        Some(PathBuf::from("/m/a.WAV"))
    );
    assert_eq!(
        resolve_file(Path::new("/m"), "a.ape", &rest),
        Some(PathBuf::from("/m/a.m4a"))
    );
}

#[test]
fn extension_fallback_ties_go_to_the_sorted_first_path() {
    let files = paths(&["/m/a.flac", "/m/A.FLAC"]);

    assert_eq!(
        resolve_file(Path::new("/m"), "a.wav", &files),
        Some(PathBuf::from("/m/A.FLAC"))
    );
}

#[test]
fn backslashes_in_file_names_are_separators() {
    let files = paths(&["/m/CD1/01 Intro.flac"]);

    assert_eq!(
        resolve_file(Path::new("/m"), "CD1\\01 Intro.flac", &files),
        Some(PathBuf::from("/m/CD1/01 Intro.flac"))
    );
}

#[test]
fn an_absolute_file_name_resolves_when_it_is_a_candidate() {
    let files = paths(&["/m/album.flac", "/elsewhere/album.flac"]);

    assert_eq!(
        resolve_file(Path::new("/m"), "/elsewhere/album.flac", &files),
        Some(PathBuf::from("/elsewhere/album.flac"))
    );
}

#[test]
fn an_absolute_file_name_outside_the_candidates_falls_back_to_its_basename() {
    let files = paths(&["/m/album.flac", "/other/b.flac"]);

    assert_eq!(
        resolve_file(Path::new("/m"), "/etc/album.flac", &files),
        Some(PathBuf::from("/m/album.flac"))
    );
    assert_eq!(
        resolve_file(Path::new("/m"), "/m/ALBUM.wav", &files),
        Some(PathBuf::from("/m/album.flac"))
    );
    assert_eq!(resolve_file(Path::new("/m"), "/etc/b.flac", &files), None);
}

#[test]
fn a_windows_absolute_file_name_falls_back_to_its_basename() {
    let files = paths(&["/m/a.flac"]);

    assert_eq!(
        resolve_file(Path::new("/m"), "C:\\Music\\a.flac", &files),
        Some(PathBuf::from("/m/a.flac"))
    );
    assert_eq!(
        resolve_file(Path::new("/m"), "\\\\nas\\share\\a.flac", &files),
        Some(PathBuf::from("/m/a.flac"))
    );
}

#[test]
fn a_relative_file_name_never_falls_back_to_its_basename() {
    let files = paths(&["/m/a.flac"]);

    assert_eq!(resolve_file(Path::new("/m"), "sub/a.flac", &files), None);
}

#[test]
fn a_file_name_with_a_dotted_stem_and_no_extension_keeps_its_dot() {
    let files = paths(&["/m/Vol.1.flac", "/m/Vol.wav"]);

    assert_eq!(
        resolve_file(Path::new("/m"), "Vol.1", &files),
        Some(PathBuf::from("/m/Vol.1.flac"))
    );
}

#[test]
fn a_dotted_stem_survives_an_extension_swap() {
    let files = paths(&["/m/Vol.1.flac"]);

    assert_eq!(
        resolve_file(Path::new("/m"), "Vol.1.ape", &files),
        Some(PathBuf::from("/m/Vol.1.flac"))
    );
}

#[test]
fn a_sheet_without_audio_tracks_is_an_error() {
    let sheet = parse(
        br#"FILE "data.bin" BINARY
  TRACK 01 MODE1/2352
    INDEX 01 00:00:00
"#,
    )
    .expect("parseable sheet");

    assert_eq!(run(&sheet, &[]), Err(CueError::NoAudioTracks));
}
