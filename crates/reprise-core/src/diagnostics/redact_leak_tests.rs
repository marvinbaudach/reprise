//! The free-text redactor against the leaks of issue #1335 (V1 to V6), in the
//! context the issue reproduced them in: music folder `/home/marvin/Music`,
//! home `/home/marvin`, user `marvin`, host `my-nas`.
//!
//! Every case asserts two things: none of the names it carries survives, and
//! redacting the result again changes nothing (the report redacts the lines
//! the file mirror has already redacted).

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

/// Redacts `input`, requires that none of `leaks` is left, and that a second
/// pass is a no-op. Returns the output for further assertions.
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

const PINK_FLOYD: &[&str] = &["Pink", "Floyd"];

// V1 -- path tails with spaces

#[test]
fn v1_a_library_path_takes_the_rest_of_its_name() {
    let output = assert_no_leak(
        "scan /home/marvin/Music/Pink Floyd/Dark Side of the Moon failed",
        &["Pink", "Floyd", "Dark", "Moon"],
    );
    assert!(output.starts_with("scan $XDG_MUSIC_DIR/…"), "{output}");
}

#[test]
fn v1_a_quoted_path_ends_at_its_closing_quote() {
    let output = assert_no_leak(
        "open \"/media/disk/Music/Pink Floyd/The Wall\" failed",
        &["Pink", "Floyd", "Wall", "disk"],
    );
    assert_eq!(output, "open \"…\" failed");

    // The common `Debug` shape of a `PathBuf` in an error.
    let output = assert_no_leak(
        "NotFound { path: \"/srv/audio/Pink Floyd/The Wall\" }",
        &["Pink", "Floyd", "Wall", "audio"],
    );
    assert!(!output.contains("/srv"), "{output}");

    // An escaped quote inside a quoted `Debug` string.
    assert_no_leak(
        "failed: \"open \\\"/srv/audio/Pink Floyd/The Wall\\\"\" now",
        &["Pink", "Floyd", "Wall"],
    );
}

#[test]
fn v1_a_comma_in_a_library_name_does_not_end_it() {
    assert_no_leak(
        "read /home/marvin/Music/Earth, Wind & Fire/Greatest Hits/Track.flac",
        &["Earth", "Wind", "Fire", "Greatest", "Hits", "Track"],
    );
    // Outside the library the next slash word still pulls the words before it in.
    assert_no_leak(
        "read /mnt/x/Earth, Wind & Fire/Greatest Hits/Track.flac now",
        &["Earth", "Wind", "Fire", "Greatest", "Hits", "Track"],
    );
}

#[test]
fn v1_brackets_in_a_library_name_do_not_end_it() {
    assert_no_leak(
        "scan /home/marvin/Music/Wish You Were Here [FLAC]/Shine On failed",
        &["Wish", "Were", "FLAC", "Shine"],
    );
    assert_no_leak(
        "scan /mnt/x/Wish You Were Here [FLAC]/Shine failed",
        &["Wish", "Were", "FLAC", "Shine"],
    );
}

#[test]
fn v1_a_no_break_space_inside_a_name_is_a_gap() {
    assert_no_leak(
        "open /srv/a/Pink\u{a0}Floyd/The\u{a0}Wall/x done",
        &["Pink", "Floyd", "Wall"],
    );
    assert_no_leak(
        "open /home/marvin/Music/Pink\u{a0}Floyd/The\u{a0}Wall failed",
        &["Pink", "Floyd", "Wall"],
    );
}

#[test]
fn v1_the_typed_fields_after_a_library_path_stay_readable() {
    let output = assert_no_leak(
        "scan /home/marvin/Music/Pink Floyd/Wall failed count=3 elapsed_ms=1.5",
        PINK_FLOYD,
    );
    assert!(output.ends_with("count=3 elapsed_ms=1.5"), "{output}");
}

#[test]
fn v1_a_second_path_after_a_library_path_is_its_own_span() {
    let output = assert_no_leak(
        "failed /home/marvin/Music/Pink Floyd/a.flac, /var/tmp/x.log, file:///srv/y.flac done",
        PINK_FLOYD,
    );
    assert!(output.contains("file://…"), "{output}");
}

// V2 -- URIs

#[test]
fn v2_an_mtp_uri_takes_the_device_name_and_the_path() {
    let output = assert_no_leak(
        "copy to mtp://Google Pixel 7/Internal shared storage/Music/Pink Floyd/Time.flac failed",
        &["Google", "Pixel", "Internal", "Pink", "Floyd", "Time"],
    );
    assert!(output.contains("mtp://…"), "{output}");
}

#[test]
fn v2_a_bracketed_address_and_a_comma_in_a_uri_do_not_end_it() {
    let output = assert_no_leak(
        "fetch http://[2001:db8::1]:8080/Pink Floyd/Time",
        &["2001", "8080", "Pink", "Floyd", "Time"],
    );
    assert!(output.contains("http://…"), "{output}");
    assert_no_leak(
        "fetch https://example.com/a,b/Pink Floyd",
        &["a,b", "Pink", "Floyd"],
    );
}

#[test]
fn v2_a_quoted_uri_ends_at_its_quote_and_a_web_address_at_its_punctuation() {
    assert_eq!(
        assert_no_leak(
            "copy 'smb://nas/music/Pink Floyd/a.flac' failed",
            PINK_FLOYD
        ),
        "copy 'smb://…' failed"
    );
    // The usual shape of an HTTP client error keeps its cause.
    assert_eq!(
        assert_no_leak(
            "error sending request for url (https://example.com/x/y): connection refused",
            &["example"]
        ),
        "error sending request for url (https://…): connection refused"
    );
    assert_eq!(
        assert_no_leak("GET https://example.com/x/y: HTTP 404", &["example"]),
        "GET https://…: HTTP 404"
    );
}

// V3 -- colon form and secret needles

#[test]
fn v3_a_colon_form_field_the_policy_redacts_is_redacted() {
    for (input, leaks) in [
        ("title: Pink Floyd", PINK_FLOYD),
        ("artist:Pink Floyd", PINK_FLOYD),
        ("{\"title\":\"Pink Floyd\"}", PINK_FLOYD),
        ("title : Pink Floyd", PINK_FLOYD),
        ("Foo { artist: \"Pink Floyd\", count: 3 }", PINK_FLOYD),
    ] {
        assert_no_leak(input, leaks);
    }
}

#[test]
fn v3_every_spelling_of_a_secret_key_takes_the_line() {
    for key in [
        "Password :",
        "password:",
        "passwd:",
        "pwd:",
        "api-key:",
        "x-api-key:",
        "apikey:",
        "API_KEY:",
        "authorization:",
        "Authorization :",
        "cookie:",
    ] {
        let input = format!("login failed {key} Bearer hunter2 and more");
        let output = assert_no_leak(&input, &["hunter2", "Bearer", "more"]);
        assert!(output.starts_with("login failed"), "{output}");
    }
}

#[test]
fn v3_a_remote_root_takes_the_host_and_the_tail() {
    assert_eq!(
        assert_no_leak(
            "mount nas:/export/music/Pink Floyd",
            &["nas", "export", "Pink", "Floyd"]
        ),
        "mount …"
    );
    assert_no_leak(
        "mount user@host:/music/Pink Floyd",
        &["user", "host", "Pink", "Floyd"],
    );
}

#[test]
fn v3_colon_prose_and_rust_paths_are_not_fields() {
    for input in [
        "scan failed: no such file",
        "reprise_core::db::open failed",
        "at 12:30:45 the player stopped",
        "db::open failed",
    ] {
        assert_eq!(redacted(input), input, "{input}");
    }
}

// V4 -- network identifiers

#[test]
fn v4_addresses_and_machine_names_are_redacted() {
    for (input, leaks) in [
        ("connect 192.168.0.12:8080 failed", &["192.168", "8080"][..]),
        ("connect 192.168.0.12 failed", &["192.168"]),
        ("link fe80::1%eth0 up", &["fe80", "eth0"]),
        (
            "peer 2001:db8:85a3:0:0:8a2e:370:7334 gone",
            &["2001", "7334"],
        ),
        ("peer [2001:db8::1]:443 gone", &["2001", "db8"]),
        ("mac aa:bb:cc:dd:ee:ff seen", &["aa:bb"]),
        ("mac AA-BB-CC-DD-EE-FF seen", &["AA-BB"]),
        ("host Marvin-PC is gone", &["Marvin"]),
        ("host MY-NAS is gone", &["MY-NAS"]),
        ("mounted my-nas.local share", &["my-nas"]),
        ("addr=192.168.0.12:8080", &["192.168"]),
    ] {
        assert_no_leak(input, leaks);
    }
}

#[test]
fn v4_numbers_versions_times_and_rust_paths_are_not_addresses() {
    for input in [
        "version=1.2.3.4",
        "version=0.1.331",
        "elapsed_ms=12.5ms count=300 volume=0.5",
        "at 12:30:45 for 1.5s",
        "reprise_core::db::fa failed",
        "300.1.1.1 is not an address",
        "10:20:30 position",
    ] {
        let output = redacted(input);
        assert!(
            !output.contains("$IP") && !output.contains("$MAC"),
            "{input}: {output}"
        );
    }
    assert_eq!(redacted("version=1.2.3.4"), "version=1.2.3.4");
    assert_eq!(
        redacted("reprise_core::db::fa failed"),
        "reprise_core::db::fa failed"
    );
}

// V5 -- other path shapes

#[test]
fn v5_home_tilde_unc_drive_and_relative_paths_are_redacted() {
    for input in [
        "scan $HOME/Music/Pink Floyd/Wall failed",
        "scan ${HOME}/Music/Pink Floyd/Wall failed",
        "scan ~/Music/Pink Floyd/Wall failed",
        "scan ~bob/Music/Pink Floyd/Wall failed",
        "scan \\\\nas\\music\\Pink Floyd\\Wall failed",
        "scan C:\\Users\\marvin\\Music\\Pink Floyd\\Wall failed",
        "scan C:/Users/marvin/Music/Pink Floyd/Wall failed",
        "scan Music/Pink Floyd/Wall failed",
        "scan ./music/Pink Floyd/Wall failed",
    ] {
        assert_no_leak(input, &["Pink", "Floyd", "Wall", "marvin", "bob", "nas"]);
    }
}

#[test]
fn v5_a_path_glued_to_the_word_before_it_is_still_a_path() {
    assert_no_leak(
        "2026-11-21T12:00:00.123Z/home/x/Pink Floyd",
        &["Pink", "Floyd", "home"],
    );
    assert_no_leak("opened/home/x/Pink Floyd/a.flac", &["Pink", "Floyd"]);
}

#[test]
fn v5_the_redactors_own_markers_and_plain_slashes_survive() {
    for input in [
        "scan $XDG_MUSIC_DIR/… failed",
        "scan $HOME/… failed",
        "copy mtp://… failed",
        "track 3/12 of 5/10",
        "date 21/11/2026 ok",
    ] {
        assert_eq!(redacted(input), input, "{input}");
    }
}

// V6 -- multi-line messages

#[test]
fn v6_a_swallowed_value_runs_on_across_line_breaks() {
    assert_eq!(
        assert_no_leak(
            "error=Pink\nFloyd\nDark Side",
            &["Pink", "Floyd", "Dark", "Side"]
        ),
        "error=$REDACTED"
    );
    assert_no_leak(
        "title: Pink\nFloyd\r\nDark Side",
        &["Floyd", "Dark", "Side"],
    );
    assert_no_leak("password: hunter2\nsecond line", &["hunter2", "second"]);
}

// Cross-cutting

#[test]
fn the_harmless_fields_of_the_field_policy_stay_readable() {
    for input in [
        "count=3",
        "count=-3",
        "version=0.1.331",
        "version=1.2.3.4",
        "elapsed_ms=12.5ms",
        "elapsed_ms=12",
        "mode=Compact",
        "state=Playing",
        "id=Track7",
        "state=Some(Track(5))",
        "episode_ids=[1, 2, 3]",
        "error=$REDACTED count=3",
        "mode=Compact; id=12 kind=Music",
        "count: 3, mode: Compact",
        "QueueRow { index: 1 }",
        "2026-11-08T12:00:00.123Z  WARN reprise_core::scan: playback stalled elapsed_ms=12",
    ] {
        assert_eq!(redacted(input), input, "{input}");
    }
}

#[test]
fn a_path_next_to_multibyte_text_neither_panics_nor_leaks() {
    for input in [
        "scan \"/srv/ü/Björk — Vespertine/Hidden Place\" fertig",
        "scan /home/marvin/Music/Sigur Rós/ágætis byrjun/Svefn-g-englar 🎵 fertig",
        "scan /srv/Sigur\u{2003}Rós/ágætis\u{a0}byrjun/x 🎵 fertig",
        "elapsed=12µs /srv/ü/é, ö/x.flac ü",
        "url \u{201c}https://example.com/ü/é\u{201d} ü",
        "Passwörd : x 🎵",
        "𝔘𝔰𝔢𝔯 192.168.0.12:8080 𝔘",
    ] {
        let output = redacted(input);
        assert_eq!(redacted(&output), output, "not idempotent: {input:?}");
        for leaked in ["Björk", "Vespertine", "Sigur", "byrjun", "192.168"] {
            assert!(
                !output.contains(leaked),
                "{leaked:?} leaked from {input:?}: {output:?}"
            );
        }
    }
}

#[test]
fn a_very_long_line_of_paths_stays_linear() {
    let input = "/a/b c/d e/f g ".repeat(20_000);
    let started = std::time::Instant::now();
    let output = redacted(&input);

    assert!(!output.is_empty());
    assert!(started.elapsed() < std::time::Duration::from_secs(5));
}
