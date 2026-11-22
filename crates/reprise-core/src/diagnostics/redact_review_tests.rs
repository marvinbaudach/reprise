//! Leaks an adversarial review of the first fix found (PR 1374), one test per
//! family of inputs. Same context as `redact_leak_tests`: music folder
//! `/home/marvin/Music`, home `/home/marvin`, user `marvin`, host `my-nas`.

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

const NAMES: &[&str] = &["Pink", "Floyd", "Rock", "Collection", "Phone", "Drive"];

// 1 -- a path whose last segment holds a space

#[test]
fn a_path_in_the_users_data_takes_the_words_after_its_last_segment() {
    for input in [
        "cannot read /mnt/usb/Pink Floyd after 3 tries",
        "cannot open /data/Rock Collection/Pink Floyd",
        "copy to /run/media/bob/My Phone/Pink Floyd",
        "copy to /run/user/1000/gvfs/mtp:host=Google_Pixel_7/Internal storage/Pink Floyd failed",
        "copy to /Volumes/External Drive/Rock Collection",
        "cannot read /srv/share/Pink Floyd - The Wall (1979)",
        "open /srv/x/Pink\u{a0}Floyd failed",
        "open /srv/x/Pink\tFloyd failed",
        "open /srv/x/Pink Floyd\nsecond line",
        "open file:/srv/x/Pink Floyd",
        "open file:/opt/x/Pink Floyd now",
    ] {
        assert_no_leak(
            input,
            &[
                "Pink",
                "Floyd",
                "Rock",
                "Collection",
                "Phone",
                "Drive",
                "gvfs",
                "Pixel",
            ],
        );
    }
}

#[test]
fn a_system_path_takes_the_following_words_that_open_like_a_name() {
    for input in [
        "cannot read /opt/data/Pink Floyd",
        "scan failed: /var/tmp/Pink Floyd: permission denied",
        "cannot read /usr/share/Pink Floyd - The Wall (1979)",
    ] {
        assert_no_leak(input, NAMES);
    }
    // The sentence after it goes with it.
    assert_eq!(
        redacted("moved /var/tmp/x.mp3 to /opt/share/y.mp3 successfully"),
        "moved …"
    );
}

// 2 -- an apostrophe closes nothing

#[test]
fn an_apostrophe_followed_by_a_name_is_not_a_closing_quote() {
    for input in [
        "open '/home/marvin/Music/Guns N' Roses' failed",
        "open '/media/disk/Foo/Guns N' Roses' failed",
        "open '/media/disk/Foo/Guns n' roses' failed",
        "open '/media/disk/Foo/Guns N' Roses",
    ] {
        assert_no_leak(input, &["Guns", "Roses", "roses"]);
    }
    assert_eq!(
        redacted("open '/media/disk/Foo/Rock Pink Floyd' and then more words here"),
        "open '…"
    );
}

// 3 -- secrets around the colon

#[test]
fn more_secret_keys_take_the_line() {
    for key in [
        "pass:",
        "pin:",
        "auth:",
        "sessionid:",
        "signature:",
        "sig:",
        "otp:",
        "code:",
        "login:",
        "user:",
        "ssid:",
        "session_token:",
    ] {
        let input = format!("failed {key} hunter2 and more");
        assert_no_leak(&input, &["hunter2", "more"]);
    }
    assert_no_leak("failed code: 4/0AX4XfWh", &["4/0AX4XfWh"]);
}

#[test]
fn a_secret_key_is_found_whatever_space_or_colon_stands_around_it() {
    for input in [
        "password\u{a0}: hunter2",
        "password\u{a0}:\u{a0}hunter2",
        "password：hunter2",
        "password ： hunter2",
        "password\n: hunter2",
        "password\t:\thunter2",
        "\"password\" : \"hunter2\"",
    ] {
        assert_no_leak(input, &["hunter2"]);
    }
}

#[test]
fn a_short_secret_word_inside_prose_is_not_a_secret() {
    for input in [
        "Failed to decode: invalid data",
        "spinning: waiting for the device",
        "Err(Os { code: 2, kind: NotFound })",
        "passenger: none",
    ] {
        assert_eq!(redacted(input), input, "{input}");
    }
}

// 4 -- network identifiers

#[test]
fn an_address_that_opens_with_a_colon_is_an_address() {
    for input in [
        "peer ::ffff:192.168.1.5 closed",
        "peer ::ffff:c0a8:0105 closed",
        "peer [::ffff:192.168.1.5]:80 closed",
        "peer ::1 closed",
    ] {
        assert_no_leak(input, &["ffff", "192.168", "c0a8", "::1"]);
    }
}

#[test]
fn a_name_in_a_network_domain_is_a_machine() {
    for input in [
        "request to nas.local:445 failed",
        "connect to my-nas.fritz.box:445 failed",
        "connect to other-pc.fritz.box failed",
        "connect to printer.lan failed",
        "connect MY-NAS.local failed",
    ] {
        assert_no_leak(
            input,
            &["nas", "other", "printer", "fritz", ".local", ".lan"],
        );
    }
    assert_no_leak("ssid: Home Net 5G", &["Home", "Net"]);
    assert_no_leak("connected ssid=Home Net 5G", &["Home", "Net"]);
}

// 5 -- the smaller leaks

#[test]
fn a_gvfs_name_and_a_relative_path_with_spaces_are_paths() {
    for input in [
        "copy to mtp:host=Google_Pixel_7/Internal storage/x failed",
        "scan Albums/Pink Floyd/The Wall failed",
        "scan Pink Floyd/The Wall/01 Time.flac failed",
        "synced Pink Floyd/The Wall",
    ] {
        assert_no_leak(
            input,
            &[
                "Google", "Pixel", "Internal", "Pink", "Floyd", "Wall", "Time",
            ],
        );
    }
    // A pair that is no path stays readable.
    for input in [
        "I/O error",
        "read/write failed",
        "TCP/IP down",
        "3/12 tracks",
    ] {
        assert!(
            !redacted(input).contains('…') || input == "TCP/IP down",
            "{input}"
        );
    }
}

#[test]
fn a_value_after_an_unknown_key_in_colon_form_is_redacted() {
    for input in [
        "{\"foo\":\"Pink Floyd\"}",
        "{\"foo\": \"Pink Floyd\", \"count\": 3}",
        "album_name: Dark Side",
        "display_name: Pink Floyd",
        "{\"name\":\"Pink Floyd\",\"count\":3}",
    ] {
        assert_no_leak(input, &["Pink", "Floyd", "Dark", "Side"]);
    }
    // A Rust path and a time are not fields.
    for input in ["reprise_core::scan: started", "at 12:30:45 started"] {
        assert_eq!(redacted(input), input, "{input}");
    }
}

#[test]
fn a_name_is_matched_whatever_its_case_in_any_script() {
    let context = RedactionContext {
        username: Some("müller".into()),
        ..RedactionContext::default()
    };
    let output = redact_log_message("opened by MÜLLER and Müller and müller", &context);

    assert_eq!(output, "opened by $USER and $USER and $USER");
}

#[test]
fn an_encoded_uri_and_credentials_without_a_scheme_are_redacted() {
    for input in [
        "https%3A%2F%2Fbob%3Apw%40example.com%2Fx",
        "HTTPS%3a%2f%2fbob.example.com/x",
        "bob:pw@example.com/x",
        "login bob:hunter2@example.com failed",
    ] {
        assert_no_leak(input, &["bob", "hunter2", "pw", "example"]);
    }
}

#[test]
fn a_windows_path_with_backslashes_and_an_scp_remote_are_paths() {
    for input in [
        "Music\\Pink Floyd\\The Wall\\01.flac failed",
        "Documents\\Pink Floyd\\Notes",
        "open \\\\?\\C:\\Users\\bob\\Pink Floyd",
        "scp bob@nas.local:Music/Pink Floyd/x",
        "bob@10.0.0.2:/srv/x",
    ] {
        assert_no_leak(input, &["Pink", "Floyd", "bob", "nas", "Users", "10.0.0.2"]);
    }
}

#[test]
fn a_secret_word_in_running_text_takes_the_rest_of_the_line() {
    for input in [
        "the password is hunter2",
        "password hunter2",
        "sent header Bearer eyJhbGciOiJIUzI1NiJ9.abc.def to server",
        "Authorization Basic dXNlcjpwYXNz",
    ] {
        assert_no_leak(input, &["hunter2", "eyJ", "dXNl"]);
    }
    // A word on its own at the end of a line has nothing after it.
    assert_eq!(redacted("wrong password"), "wrong password");
}

#[test]
fn a_status_code_is_kept_and_an_authorisation_code_is_not() {
    for input in [
        "Os { code: 2, kind: NotFound }",
        "exit_code: 101 after 3 tries",
    ] {
        assert_eq!(redacted(input), input, "{input}");
    }
    assert_no_leak("failed code: 4/0AX4XfWh", &["4/0AX"]);
    assert_no_leak("failed code: 123456", &["123456"]);
}

#[test]
fn our_own_markers_are_not_keys_or_roots() {
    for input in [
        "opened by marvin: failed",
        "opened by marvin : failed",
        "connect to 192.168.1.5/24 failed",
    ] {
        let once = redacted(input);
        assert_eq!(redacted(&once), once, "{input}");
        assert!(once.contains("failed"), "{input}: {once}");
    }
}

// 6 -- cost

#[test]
fn adversarial_input_is_redacted_in_time() {
    let context = context();
    for (name, unit) in [
        ("unclosed quotes", "'/a "),
        ("unclosed double quotes", "\"/a b "),
        ("apostrophes", "x' Y "),
        ("path words", "/a b "),
        ("path lookahead", "/a b c d e f g h "),
        ("uris", "x://a b "),
        ("encoded uris", "x%3A%2F%2Fa b "),
        ("quoted uris", "'x://a "),
        ("assignments", "a=b "),
        ("colons", "title: x "),
        ("spaced separators", "x : "),
        ("user name", "marvin "),
        ("slashes", "a/b/c "),
        ("capital slashes", "Pink Floyd/The Wall "),
        ("addresses", "1:2:3:4:5:6:7:8:9 "),
    ] {
        let input = unit.repeat(200_000 / unit.len());
        let started = std::time::Instant::now();
        let output = redact_log_message(&input, &context);
        let elapsed = started.elapsed();

        assert!(
            elapsed < std::time::Duration::from_millis(200),
            "{name}: {elapsed:?} for {} bytes",
            input.len()
        );
        assert!(
            output.len() <= 4 * 8 * 1024,
            "{name}: {} bytes out",
            output.len()
        );
    }
}

#[test]
fn a_message_beyond_the_cap_is_cut_and_marked() {
    let input = format!("scan {}", "x".repeat(100_000));
    let output = redacted(&input);

    assert!(output.len() <= 8 * 1024);
    assert!(output.ends_with('…'));
}

// 7 -- idempotence

/// Inputs of the leak tests, kept as one list so that a second pass is checked
/// on every shape that has ever leaked.
const CURATED: &[&str] = &[
    "connect to 192.168.1.5/24 failed",
    "connect to 192.168.1.5:8080/x failed",
    "scan /home/marvin/Music/Pink Floyd/Wall failed count=3",
    "open \"/media/disk/Music/Pink Floyd/The Wall\" failed",
    "open '/home/marvin/Music/Guns N' Roses' failed",
    "copy to mtp://Google Pixel 7/Internal shared storage/Music/Pink Floyd/Time.flac failed",
    "error sending request for url (https://example.com/x/y): connection refused",
    "title: Pink Floyd",
    "password : hunter2",
    "{\"foo\":\"Pink Floyd\"}",
    "nas:/export/music/Pink Floyd",
    "peer [::ffff:192.168.1.5]:80 closed",
    "$HOME/Music/Pink Floyd/x failed",
    "C:\\Users\\marvin\\Music\\Pink Floyd\\x failed",
    "Pink Floyd/The Wall/01 Time.flac",
    "error=Pink\nFloyd\nDark Side",
    "count=3 version=0.1.349 elapsed_ms=12.5 state=Playing",
    "2026-11-21T12:00:00.123Z  WARN reprise_core::scan: playback stalled elapsed_ms=12",
    "opened by MY-NAS.local as Marvin",
];

#[test]
fn a_second_pass_changes_nothing_on_every_curated_shape() {
    for input in CURATED {
        let once = redacted(input);

        assert_eq!(redacted(&once), once, "{input:?}");
    }
}

#[test]
fn a_second_pass_changes_nothing_on_adversarial_strings() {
    const ATOMS: &[&str] = &[
        "/",
        "\\",
        "'",
        "\"",
        "`",
        " ",
        "\t",
        "\n",
        "\u{a0}",
        ":",
        "=",
        "//",
        "://",
        "http",
        "file",
        "C:",
        "~",
        "$HOME",
        "…",
        "$REDACTED",
        "key",
        "password",
        "title",
        "count",
        "a",
        "Music",
        "home/marvin",
        "marvin",
        "my-nas",
        ",",
        ";",
        ")",
        "(",
        "[",
        "]",
        "{",
        "}",
        "%",
        "::",
        "1",
        "192.168.1.1",
        "é",
        "µs",
        ".flac",
        ".",
        "-",
        "@",
        "<",
        ">",
        "\\\"",
        "\\\\",
        "：",
        "Pink",
        "Floyd",
        "É",
        "\\Foo",
        "mtp:host=",
        "nas.local",
        "::ffff:",
        "Password",
    ];
    let context = context();
    // xorshift: the same strings on every run.
    let mut state: u64 = 0x1234_5678_9abc_def0;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    for _ in 0..20_000 {
        let length = 1 + (next() % 24) as usize;
        let input: String = (0..length)
            .map(|_| ATOMS[(next() % ATOMS.len() as u64) as usize])
            .collect();
        let once = redact_log_message(&input, &context);

        assert_eq!(redact_log_message(&once, &context), once, "{input:?}");
    }
}

// A forged `$` must not turn a real name into one of our own markers.

#[test]
fn a_dollar_sign_in_front_of_a_secret_key_does_not_make_it_a_marker() {
    for input in [
        "login $password: hunter2",
        "login x$token: abc",
        "login $password : hunter2",
        "login $user: bob",
        "login $secret:hunter2",
    ] {
        assert_no_leak(input, &["hunter2", "abc", "bob"]);
    }
    // The marker itself is not a key.
    for input in [
        "opened by $USER: failed",
        "peer $IP: closed",
        "opened by $USER : failed",
    ] {
        assert_eq!(redacted(input), input, "{input}");
    }
}

#[test]
fn a_dollar_path_is_a_marker_only_when_the_whole_marker_stands_there() {
    for input in [
        "scan $HOSTNAME_REAL/Pink Floyd/x failed",
        "scan $USERDATA/Pink Floyd/x failed",
        "scan $IPFS/Pink Floyd/x failed",
        "scan $MACOS/Pink Floyd/x failed",
    ] {
        assert_no_leak(input, &["Pink", "Floyd"]);
    }
    // The prefix length of an address is not a path; a name after `$USER/` is.
    assert_eq!(
        redacted("connect to $IP/24 failed"),
        "connect to $IP/24 failed"
    );
    assert_eq!(redacted("scan $USER/x failed"), "scan …");
}

#[test]
fn a_dollar_sign_in_front_of_a_host_does_not_hide_it() {
    for input in [
        "connect to $MyNas.fritz.box failed",
        "connect to $Nas.local:445 failed",
        "connect to $192.168.1.5 failed",
        "connect to $Fe80::1 failed",
    ] {
        assert_no_leak(input, &["MyNas", "Nas", "192.168", "Fe80", "fritz"]);
    }
}
