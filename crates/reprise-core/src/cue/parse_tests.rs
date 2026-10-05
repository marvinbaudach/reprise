use super::{parse, CueError, Frames};

#[test]
fn rejects_a_sheet_without_tracks() {
    let error = parse(b"REM COMMENT no tracks\nTITLE \"Empty\"\n")
        .expect_err("a CUE sheet must contain a track");

    assert_eq!(error, CueError::EmptySheet);
}

#[test]
fn decodes_utf16_little_and_big_endian_sheets() {
    let text =
        "FILE \"album.flac\" WAVE\n  TRACK 01 AUDIO\n    TITLE \"Björk\"\n    INDEX 01 00:00:00\n";
    let mut little = vec![0xff, 0xfe];
    little.extend(text.encode_utf16().flat_map(u16::to_le_bytes));
    let mut big = vec![0xfe, 0xff];
    big.extend(text.encode_utf16().flat_map(u16::to_be_bytes));

    assert_eq!(
        parse(&little).expect("UTF-16LE sheet").files[0].tracks[0].title,
        "Björk"
    );
    assert_eq!(
        parse(&big).expect("UTF-16BE sheet").files[0].tracks[0].title,
        "Björk"
    );
}

#[test]
fn rejects_malformed_utf16() {
    let error = parse(&[0xff, 0xfe, 0x41]).expect_err("odd UTF-16 byte count");

    assert_eq!(error, CueError::InvalidTextEncoding);
}

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

fn one_track(extra: &str) -> Vec<u8> {
    format!("FILE \"album.flac\" WAVE\n  TRACK 01 AUDIO\n{extra}").into_bytes()
}

#[test]
fn rejects_garbage_bytes() {
    let error = parse(&[0x00, 0x9f, 0xfe, 0x81, 0x00, 0xc3, 0x28, 0xff]).expect_err("garbage");

    assert_eq!(error, CueError::EmptySheet);
}

#[test]
fn rejects_an_empty_input() {
    assert_eq!(parse(b""), Err(CueError::EmptySheet));
}

#[test]
fn cp1252_maps_the_0x80_to_0x9f_block() {
    let mut raw = b"FILE \"a.flac\" WAVE\n  TRACK 01 AUDIO\n    TITLE \"".to_vec();
    raw.extend([0x80, b' ', 0x92, b' ', 0x96, b' ', 0x81]);
    raw.extend(b"\"\n    INDEX 01 00:00:00\n");

    let sheet = parse(&raw).expect("valid CP1252 sheet");

    // 0x81 is undefined in CP1252 and maps to the C1 control of the same value.
    assert_eq!(
        sheet.files[0].tracks[0].title,
        "\u{20ac} \u{2019} \u{2013} \u{0081}"
    );
}

#[test]
fn a_utf8_bom_is_not_part_of_the_first_keyword() {
    let mut bytes = vec![0xef, 0xbb, 0xbf];
    bytes.extend(b"TITLE \"Album\"\n");
    bytes.extend(one_track("    INDEX 01 00:00:00\n"));

    let sheet = parse(&bytes).expect("BOM sheet");

    assert_eq!(sheet.title, "Album");
}

#[test]
fn rejects_malformed_msf_values() {
    for value in ["00:60:00", "00:00:75", "00:00", "00:00:00:00", "aa:bb:cc"] {
        let error =
            parse(&one_track(&format!("    INDEX 01 {value}\n"))).expect_err("malformed MSF");

        assert_eq!(
            error,
            CueError::InvalidIndex {
                track: 1,
                value: value.to_owned(),
            }
        );
    }
}

#[test]
fn msf_values_floor_to_whole_frames() {
    let sheet = parse(&one_track("    INDEX 01 00:59:74\n")).expect("last valid frame");

    assert_eq!(sheet.files[0].tracks[0].index01, Frames(59 * 75 + 74));
}

#[test]
fn rejects_a_track_before_any_file() {
    let error = parse(b"TRACK 01 AUDIO\n  INDEX 01 00:00:00\n").expect_err("no FILE yet");

    assert!(matches!(error, CueError::InvalidStatement { line: 1, .. }));
}

#[test]
fn rejects_a_track_without_a_datatype() {
    let error = parse(b"FILE \"a.flac\" WAVE\n  TRACK 01\n    INDEX 01 00:00:00\n")
        .expect_err("missing datatype");

    assert!(matches!(error, CueError::InvalidStatement { line: 2, .. }));
}

#[test]
fn parses_the_track_datatype() {
    let sheet = parse(
        br#"FILE "a.flac" WAVE
  TRACK 01 audio
    INDEX 01 00:00:00
  TRACK 02 MODE1/2352
    INDEX 01 01:00:00
"#,
    )
    .expect("valid sheet");

    assert!(sheet.files[0].tracks[0].is_audio);
    assert!(!sheet.files[0].tracks[1].is_audio);
}

#[test]
fn accepts_unquoted_file_names_with_spaces() {
    let sheet = parse(b"FILE My Album Disc 1.flac WAVE\n  TRACK 01 AUDIO\n    INDEX 01 00:00:00\n")
        .expect("unquoted name");

    assert_eq!(sheet.files[0].name, "My Album Disc 1.flac");
}

#[test]
fn a_quoted_file_name_may_contain_quotes_and_a_type_token() {
    let sheet =
        parse(b"FILE \"The \"Best\" Of.flac\" WAVE\n  TRACK 01 AUDIO\n    INDEX 01 00:00:00\n")
            .expect("quoted name");

    assert_eq!(sheet.files[0].name, "The \"Best\" Of.flac");
}

#[test]
fn accepts_index_numbers_without_zero_padding() {
    let sheet = parse(
        br#"FILE "a.flac" WAVE
  TRACK 01 AUDIO
    INDEX 1 00:00:00
  TRACK 02 AUDIO
    INDEX 0 00:59:00
    INDEX 1 01:00:00
"#,
    )
    .expect("unpadded indices");

    assert_eq!(sheet.files[0].tracks[1].index00, Some(Frames(59 * 75)));
    assert_eq!(sheet.files[0].tracks[1].index01, Frames(60 * 75));
}

#[test]
fn rejects_a_duplicate_index_one() {
    let error = parse(&one_track("    INDEX 01 00:00:00\n    INDEX 1 00:01:00\n"))
        .expect_err("duplicate INDEX 01");

    assert_eq!(error, CueError::DuplicateIndex { track: 1, index: 1 });
}

#[test]
fn rejects_an_index_zero_that_is_not_before_index_one() {
    let error = parse(&one_track("    INDEX 00 00:01:00\n    INDEX 01 00:01:00\n"))
        .expect_err("pregap of zero length");

    assert_eq!(error, CueError::NonMonotonicIndex { track: 1 });
}

#[test]
fn rejects_an_index_zero_inside_the_previous_track() {
    let error = parse(
        br#"FILE "a.flac" WAVE
  TRACK 01 AUDIO
    INDEX 01 00:00:00
  TRACK 02 AUDIO
    INDEX 01 01:00:00
  TRACK 03 AUDIO
    INDEX 00 00:59:74
    INDEX 01 02:00:00
"#,
    )
    .expect_err("pregap before the previous track starts");

    assert_eq!(error, CueError::NonMonotonicIndex { track: 3 });
}

#[test]
fn an_index_zero_may_start_exactly_at_the_previous_index_one() {
    let sheet = parse(
        br#"FILE "a.flac" WAVE
  TRACK 01 AUDIO
    INDEX 01 00:00:00
  TRACK 02 AUDIO
    INDEX 00 00:00:00
    INDEX 01 01:00:00
"#,
    )
    .expect("pregap may start where the previous track does");

    assert_eq!(sheet.files[0].tracks[1].index00, Some(Frames(0)));
}

#[test]
fn splits_lines_on_a_lone_carriage_return() {
    let sheet = parse(
        b"TITLE \"Old Mac\"\rFILE \"a.flac\" WAVE\r  TRACK 01 AUDIO\r    TITLE \"One\"\r    INDEX 01 00:00:00\r",
    )
    .expect("CR-only sheet");

    assert_eq!(sheet.title, "Old Mac");
    assert_eq!(sheet.files[0].tracks[0].title, "One");
}

#[test]
fn ignores_junk_after_a_closing_quote() {
    let sheet = parse(
        b"TITLE \"Album\" trailing\nFILE \"a.flac\" WAVE\n  TRACK 01 AUDIO\n    TITLE \"Song\" junk \"x\n    INDEX 01 00:00:00\n",
    )
    .expect("junk after quote");

    assert_eq!(sheet.title, "Album");
    assert_eq!(sheet.files[0].tracks[0].title, "Song");
}
