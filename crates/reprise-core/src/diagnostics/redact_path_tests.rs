//! Absolute paths whose folder names contain spaces (P2).

use super::{redact_log_message, RedactionContext};

fn context() -> RedactionContext {
    RedactionContext {
        music_dir: Some("/home/u/Music".into()),
        home_dir: Some("/home/u".into()),
        username: Some("u".into()),
        hostname: None,
    }
}

#[test]
fn a_music_folder_name_with_spaces_does_not_leak_its_tail() {
    let output = redact_log_message(
        "could not scan /home/u/Music/Pink Floyd/The Wall",
        &context(),
    );

    for leaked in ["Pink", "Floyd", "The"] {
        assert!(!output.contains(leaked), "{leaked:?} leaked: {output}");
    }
    assert!(
        output.starts_with("could not scan $XDG_MUSIC_DIR/…"),
        "{output}"
    );
}

#[test]
fn an_absolute_path_with_spaces_outside_the_library_does_not_leak_its_tail() {
    let output = redact_log_message("could not scan /media/disk/My Band/Live 2019", &context());

    for leaked in ["My", "Band", "Live", "disk"] {
        assert!(!output.contains(leaked), "{leaked:?} leaked: {output}");
    }
}

#[test]
fn every_word_after_a_path_belongs_to_it() {
    let output = redact_log_message(
        "opened /var/a b/c d/e f/g then closed",
        &RedactionContext::default(),
    );

    assert_eq!(output, "opened …");
}

#[test]
fn a_path_ends_at_its_line_not_at_a_word_or_a_delimiter() {
    let none = RedactionContext::default();

    assert_eq!(
        redact_log_message("read /a/b and wrote /c/d.", &none),
        "read …"
    );
    assert_eq!(
        redact_log_message("read (/a/b), then /c/d x/y; done", &none),
        "read (…"
    );
    assert_eq!(
        redact_log_message("scan /opt/My Band/Live 2019 failed", &none),
        "scan …"
    );
    assert_eq!(
        redact_log_message("scan /media/My Band/Live now", &none),
        "scan …"
    );
    assert_eq!(
        redact_log_message("scan /a/b\nthen /c/d\nread", &none),
        "scan …\nthen …\nread"
    );
}
