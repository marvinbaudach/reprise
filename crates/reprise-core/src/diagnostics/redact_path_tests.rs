//! Absolute paths whose folder names contain spaces (P2).

use super::{redact_log_message, RedactionContext};

fn context() -> RedactionContext {
    RedactionContext {
        music_dir: Some("/home/u/Music".into()),
        home_dir: Some("/home/u".into()),
        username: Some("u".into()),
    }
}

#[test]
fn a_music_folder_name_with_spaces_does_not_leak_its_tail() {
    let output = redact_log_message("could not scan /home/u/Music/Pink Floyd/The Wall", &context());

    for leaked in ["Pink", "Floyd", "The"] {
        assert!(!output.contains(leaked), "{leaked:?} leaked: {output}");
    }
    assert!(output.starts_with("could not scan $XDG_MUSIC_DIR/…"), "{output}");
}

#[test]
fn an_absolute_path_with_spaces_outside_the_library_does_not_leak_its_tail() {
    let output = redact_log_message("could not scan /media/disk/My Band/Live 2019", &context());

    for leaked in ["My", "Band", "Live", "disk"] {
        assert!(!output.contains(leaked), "{leaked:?} leaked: {output}");
    }
}

#[test]
fn every_slash_word_after_a_path_belongs_to_it() {
    let output = redact_log_message(
        "opened /srv/a b/c d/e f/g then closed",
        &RedactionContext::default(),
    );

    assert_eq!(output, "opened … then closed");
}

#[test]
fn a_path_ends_at_the_first_word_without_a_slash_or_at_a_delimiter() {
    let none = RedactionContext::default();

    assert_eq!(
        redact_log_message("read /a/b and wrote /c/d.", &none),
        "read … and wrote …"
    );
    assert_eq!(
        redact_log_message("read (/a/b), then /c/d x/y; done", &none),
        "read (…), then …; done"
    );
    // The final segment has no `/` of its own, so it cannot be told from the
    // next word. This is the documented limit.
    assert_eq!(
        redact_log_message("scan /media/My Band/Live 2019", &none),
        "scan … 2019"
    );
}
