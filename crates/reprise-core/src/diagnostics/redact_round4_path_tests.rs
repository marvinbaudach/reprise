//! Leaks the fourth review of PR 1374 found in relative paths and bare file
//! names: a single slash with a space in a name, a first segment that holds
//! punctuation, a file name touched by punctuation, and a backslash in front
//! of an escape letter. Same context as `redact_leak_tests`.

use super::{redact_log_message, RedactionContext};

fn context() -> RedactionContext {
    RedactionContext {
        music_dir: Some("/home/marvin/Music".into()),
        home_dir: Some("/home/marvin".into()),
        username: Some("marvin".into()),
        hostname: Some("my-nas".into()),
    }
}

fn redacted(input: &str) -> String {
    redact_log_message(input, &context())
}

/// None of `leaks` survives, and a second pass changes nothing.
fn assert_no_leak(input: &str, leaks: &[&str]) -> String {
    let output = redacted(input);
    for leaked in leaks {
        assert!(
            !output.contains(leaked),
            "{leaked:?} leaked from {input:?}: {output:?}"
        );
    }
    assert_eq!(redacted(&output), output, "not idempotent: {input:?}");
    output
}

const NAMES: &[&str] = &[
    "Pink", "Floyd", "Wall", "Flesh", "Dark", "Moon", "wish", "zqxa", "zqxb", "Zqxa", "Zqxb",
];

#[test]
fn a_relative_path_with_one_slash_and_a_space_in_a_name_is_a_path() {
    for input in [
        "copied Pink Floyd/01 - Time.flac to device",
        "skipping The Wall/01 In the Flesh.flac (exists)",
        "could not import Pink Floyd/the wall",
        "Pink Floyd/wish you were here",
        "missing: Dark Side of the Moon/03 time",
        "copied pink floyd/the wall to device",
        "copied zqxa/zqxb failed",
        "zqxa/zqxb",
        "sync: 01 zqxa/01 zqxb (exists)",
        "skip (Zqxa/2019 - Zqxb) now",
        "missing \"Zqxa Zqxb/zqxa zqxb\" failed",
        "copied the zqxa/1-01 Zqxb to device",
    ] {
        assert_no_leak(input, NAMES);
    }
}

#[test]
fn ordinary_slashes_in_prose_stay_readable() {
    for input in [
        "decoded 12/11/2026 at 3/12 with I/O",
        "mode read/write",
        "type audio/flac",
        "rate 5 MB/s",
        "ratio 3/12 and 4/5 done",
        "TCP/IP and/or UDP/IP failed",
        "uptime 24/7 and 16/44.1 and 1.5/2",
        "caps audio/x-raw and application/json and image/jpeg",
        "http/1.1 and HTTP/2 stalled",
        "toggle play/pause failed",
        "speed 3 kbit/s and 40 km/h and 12/min",
        "value is N/A on this device",
        "I/O and read/write took 3 ms",
    ] {
        assert_eq!(redacted(input), input);
    }
}

#[test]
fn a_first_segment_with_punctuation_in_it_goes_back_to_the_start_of_the_message() {
    for input in [
        "copied Bach: Goldberg Variations/Disc 1/01 Aria.flac",
        "copied Mozart; Requiem/Kyrie/01.flac",
        "copied Florence + the Machine (Deluxe Edition)/Lungs/01 Dog Days.flac",
        "copied Prince [Remastered] Purple Rain/Disc 1/01 Purple Rain.flac",
        "copied Sigur Ros <Live> Takk/Disc 1/01 Glosoli.flac",
        "copied Panic! at the Disco: Pretty. Odd./Disc/01.flac",
        "copied Guns N' Roses=Appetite/Disc 1/01.flac",
        "copied 'Weird Al' Yankovic/Disc 1/01.flac",
        "copied \"Heroes\" Bowie/Disc 1/01.flac",
        "copied Bach:Goldberg Variations/Disc 1/01 Aria.flac",
    ] {
        assert_no_leak(
            input,
            &[
                "Bach",
                "Goldberg",
                "Mozart",
                "Requiem",
                "Florence",
                "Machine",
                "Deluxe",
                "Lungs",
                "Prince",
                "Remastered",
                "Purple",
                "Sigur",
                "Takk",
                "Panic",
                "Disco",
                "Pretty",
                "Odd",
                "Guns",
                "Roses",
                "Appetite",
                "Weird",
                "Yankovic",
                "Heroes",
                "Bowie",
            ],
        );
    }
}

#[test]
fn a_path_goes_back_only_to_the_log_prefix_of_its_line() {
    let output = assert_no_leak(
        "2026-11-08T12:00:00.123Z  WARN reprise::sync: copied Bach: Goldberg Variations/Disc 1/01 Aria.flac",
        &["Bach", "Goldberg", "Aria"],
    );

    assert!(
        output.starts_with("2026-11-08T12:00:00.123Z  WARN reprise::sync: "),
        "{output:?}"
    );
}

#[test]
fn a_path_goes_back_only_to_the_start_of_its_own_line() {
    let output = assert_no_leak(
        "first line is fine\nsecond: Pink Floyd/the wall\nthird line is fine",
        &["Pink", "Floyd", "wall"],
    );

    assert!(output.starts_with("first line is fine\n"), "{output:?}");
    assert!(output.ends_with("\nthird line is fine"), "{output:?}");
}

#[test]
fn a_bare_file_name_touched_by_punctuation_is_still_a_file_name() {
    for input in [
        "could not decode 01 - Time.flac.",
        "decode failed: Time.flac!",
        "is this Time.flac?",
        "decoded `Time.flac` ok",
        "cover is <cover.jpg> now",
        "cover is «cover.jpg» now",
        "title “Pink Floyd - Time.flac” gone",
        "title ‘Time.flac’ gone",
        "title *Time.flac* gone",
        "title (Time.flac). gone",
        "title Time.flac… gone",
    ] {
        assert_no_leak(input, &["Time", "Pink", "Floyd", "cover"]);
    }
}

#[test]
fn a_media_file_name_with_spaces_takes_every_word_back_to_the_start_of_the_message() {
    let output = assert_no_leak(
        "decoding Dark Side of the Moon.flac failed",
        &["Dark", "Side", "Moon"],
    );

    assert!(output.ends_with(" failed"), "{output:?}");
    assert_no_leak("opened Wish You Were Here.jpg now", &["Wish", "Here"]);
    assert_no_leak(
        "2026-11-08T12:00:00.123Z  WARN reprise::cover: no cover for Wish You Were Here.png",
        &["Wish", "Here"],
    );
}

#[test]
fn abbreviations_with_dots_are_not_file_names() {
    for input in [
        "tracks, e.g. the first one",
        "tracks (i.e. the first one).",
        "at 3 p.m. today",
        "the U.S.A. tour",
    ] {
        assert_eq!(redacted(input), input);
    }
}

#[test]
fn a_backslash_in_front_of_an_escape_letter_can_still_separate_names() {
    for input in [
        "open \\nirvana\\nevermind\\track one",
        "open ..\\nirvana\\nevermind",
        "open D:nirvana\\nevermind",
        "open Documents\\pink floyd\\notes",
        "open C:Users\\nirvana\\nevermind now",
        "open Pink\\the wall",
        "copied Pink Floyd\\01 time to device",
        "copied zqxa\\0 zqxb",
    ] {
        assert_no_leak(
            input,
            &[
                "nirvana",
                "nevermind",
                "track",
                "pink",
                "Pink",
                "Floyd",
                "floyd",
                "notes",
                "wall",
                "zqx",
                "Users",
            ],
        );
    }
}

#[test]
fn a_path_whose_first_word_opens_with_punctuation_is_still_seen() {
    for input in [
        "copied (Pink Floyd/The Wall/01.flac",
        "copied \"Pink Floyd/Disc 1/…",
        "copied [Remastered]/Pink Floyd/x",
        "copied «Pink Floyd/The Wall»",
        "copied *Pink Floyd/the wall",
        "copied -Pink Floyd/the wall",
        "copied #Pink Floyd/the wall",
        "copied …Pink Floyd/the wall",
        "copied !zqxa/zqxb",
        "copied +Pink Floyd/the wall",
        "copied @Pink Floyd/the wall",
    ] {
        assert_no_leak(
            input,
            &["Pink", "Floyd", "Wall", "wall", "Remastered", "zqx"],
        );
    }
}
