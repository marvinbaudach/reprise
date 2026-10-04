use super::{parse, CueError, Frames};

#[test]
fn parses_an_eac_style_sheet() {
    let sheet = parse(
        r#"REM GENRE "Rock"
REM DATE 1997
PERFORMER "Björk"
TITLE "Homogenic"
FILE "Björk - Homogenic.wav" WAVE
  TRACK 01 AUDIO
    TITLE "Hunter"
    PERFORMER "Björk"
    INDEX 01 00:00:00
  TRACK 02 AUDIO
    TITLE "Jóga"
    PERFORMER "Björk"
    INDEX 01 04:15:37
"#
        .as_bytes(),
    )
    .expect("valid EAC sheet");

    assert_eq!(sheet.title, "Homogenic");
    assert_eq!(sheet.performer, "Björk");
    assert_eq!(sheet.date.as_deref(), Some("1997"));
    assert_eq!(sheet.genre.as_deref(), Some("Rock"));
    assert_eq!(sheet.files[0].name, "Björk - Homogenic.wav");
    assert_eq!(sheet.files[0].tracks[0].title, "Hunter");
    assert_eq!(sheet.files[0].tracks[1].number, 2);
    assert_eq!(sheet.files[0].tracks[1].index01, Frames(19_162));
}

#[test]
fn parses_lowercase_multi_file_sheets_and_inherits_the_album_performer() {
    let sheet = parse(
        b"\xef\xbb\xbfperformer \"Massive Attack\"\r\n\
title \"Mezzanine\"\r\n\
file \"01 - Angel.flac\" WAVE\r\n\
  track 01 AUDIO\r\n\
    title \"Angel\"\r\n\
    index 01 00:00:00\r\n\
file \"02 - Risingson.flac\" WAVE\r\n\
  track 02 AUDIO\r\n\
    title \"Risingson\"\r\n\
    index 01 00:00:00\r\n",
    )
    .expect("valid multi-file sheet");

    assert_eq!(sheet.files.len(), 2);
    assert_eq!(sheet.files[1].name, "02 - Risingson.flac");
    assert_eq!(sheet.files[0].tracks[0].performer, "Massive Attack");
    assert_eq!(sheet.files[1].tracks[0].performer, "Massive Attack");
}

#[test]
fn falls_back_to_cp1252() {
    let sheet = parse(
        b"PERFORMER \"M\xfcller\"\n\
TITLE \"Gr\xfc\xdfe\"\n\
FILE \"album.flac\" WAVE\n\
  TRACK 01 AUDIO\n\
    TITLE \"F\xfcr immer\"\n\
    INDEX 01 00:00:00\n",
    )
    .expect("valid CP1252 sheet");

    assert_eq!(sheet.performer, "Müller");
    assert_eq!(sheet.title, "Grüße");
    assert_eq!(sheet.files[0].tracks[0].title, "Für immer");
}

#[test]
fn rejects_duplicate_track_numbers() {
    let error = parse(
        br#"FILE "one.flac" WAVE
  TRACK 01 AUDIO
    INDEX 01 00:00:00
FILE "two.flac" WAVE
  TRACK 01 AUDIO
    INDEX 01 00:00:00
"#,
    )
    .expect_err("duplicate track number");

    assert_eq!(error, CueError::DuplicateTrackNumber { track: 1 });
}

#[test]
fn rejects_non_monotonic_indices_within_a_file() {
    let error = parse(
        br#"FILE "album.flac" WAVE
  TRACK 01 AUDIO
    INDEX 01 01:00:00
  TRACK 02 AUDIO
    INDEX 01 00:30:00
"#,
    )
    .expect_err("backwards index");

    assert_eq!(error, CueError::NonMonotonicIndex { track: 2 });
}

#[test]
fn keeps_index_zero_as_the_tracks_pregap() {
    let sheet = parse(
        br#"FILE "album.flac" WAVE
  TRACK 01 AUDIO
    INDEX 01 00:00:00
  TRACK 02 AUDIO
    INDEX 00 03:59:70
    INDEX 01 04:01:00
"#,
    )
    .expect("valid pregap");

    assert_eq!(sheet.files[0].tracks[1].index00, Some(Frames(17_995)));
    assert_eq!(sheet.files[0].tracks[1].index01, Frames(18_075));
}

#[test]
fn rejects_a_track_without_index_one() {
    let error = parse(
        br#"FILE "album.flac" WAVE
  TRACK 07 AUDIO
    TITLE "Broken"
    INDEX 00 00:00:00
"#,
    )
    .expect_err("missing INDEX 01");

    assert_eq!(error, CueError::MissingIndex01 { track: 7 });
}

#[test]
fn rejects_an_index_that_overflows_frames() {
    let error = parse(
        br#"FILE "album.flac" WAVE
  TRACK 01 AUDIO
    INDEX 01 4294967295:59:74
"#,
    )
    .expect_err("overflowing index");

    assert_eq!(
        error,
        CueError::InvalidIndex {
            track: 1,
            value: "4294967295:59:74".to_owned(),
        }
    );
}
