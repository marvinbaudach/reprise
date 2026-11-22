//! Leaks the fourth review of PR 1374 found in secrets and in colon values:
//! a secret word behind a prefix (`db.password`), a short word followed by a
//! value that looks like one, a literal `…` standing in for a value, and two
//! values in a port or kept-key position. Same context as `redact_leak_tests`.

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

#[test]
fn a_secret_word_behind_a_prefix_takes_its_value() {
    for input in [
        "db.password hunter2",
        "self.token abc123 expired",
        "x.password -> hunter2",
        "auth.password(hunter2)",
        "config/secret is hunter2",
        "settings.api_key abc123",
        "lastfm.session_key abc123def",
        "req.headers.authorization abc123",
        "db.password.hunter2",
        "x.y.z.token=abc123",
        "a-b.api_key|abc123",
    ] {
        assert_no_leak(input, &["hunter2", "abc123"]);
    }
}

#[test]
fn a_short_word_followed_by_a_value_that_looks_like_one_takes_it() {
    for input in [
        "pass hunter2",
        "PIN 1234",
        "otp 123456",
        "session abc123def",
        "sessionid abc123def",
        "session_id abc123def",
        "auth abc123def",
        "code 4/0AX4XfWh",
        "key AIzaSyD-abc",
        "sent pass hunter2 to the server",
        "pass deadbeef12 and more",
        "my_auth abc123def",
        "sent sessionid  abc123def now",
        "db.session abc123def",
    ] {
        assert_no_leak(
            input,
            &[
                "hunter2",
                "1234",
                "123456",
                "abc123def",
                "0AX4XfWh",
                "AIzaSyD",
            ],
        );
    }
}

#[test]
fn a_short_word_followed_by_an_ordinary_word_stays_readable() {
    for input in [
        "exit code 101",
        "status code 404",
        "http status code 200 returned",
        "track analysis session refused a chunk",
        "stats group key recovered by name fallback",
        "key frame decoded",
        "pass the ball",
        "auth failed for the server",
        "code path taken",
    ] {
        assert_eq!(redacted(input), input);
    }
}

#[test]
fn a_literal_ellipsis_as_a_colon_value_does_not_switch_the_key_off() {
    for input in [
        "title: … Pink Floyd",
        "artist: …, Pink Floyd",
        "error: … could not open Pink Floyd",
        "path: … Pink Floyd",
        "album: …\tPink Floyd",
        "title: …Pink Floyd",
    ] {
        assert_no_leak(input, &["Pink", "Floyd"]);
    }
}

#[test]
fn an_ellipsis_the_path_pass_wrote_stays_where_nothing_follows_it() {
    for input in [
        "path: …",
        "title: …",
        "error: …",
        "scan failed path: …\nsecond line",
        "could not scan path: …; count=3",
        "could not scan path: …; count=3 index=2",
    ] {
        assert_eq!(redacted(input), input);
    }
}

#[test]
fn a_word_in_the_port_position_that_is_not_a_port_is_redacted() {
    for input in [
        "connect 192.168.1.5:hunter2",
        "peer 192.168.1.5:hunter2 gone",
        "to 10.0.0.1:Zqxsecret ok",
    ] {
        assert_no_leak(input, &["hunter2", "Zqxsecret"]);
    }
    assert_eq!(redacted("connect 192.168.1.5:8080 ok"), "connect $IP ok");
}

#[test]
fn a_secret_word_as_the_value_of_a_kept_key_takes_the_words_after_it() {
    for input in ["state=password hunter2", "state=token abc123 expired"] {
        assert_no_leak(input, &["hunter2", "abc123"]);
    }
}
