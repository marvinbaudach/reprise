use std::sync::{Arc, Mutex};

use tracing_subscriber::layer::SubscriberExt;

use super::*;

fn mirrored(log: impl FnOnce()) -> String {
    let lines = Arc::new(Mutex::new(String::new()));
    let sink = lines.clone();
    let layer = FileMirrorLayer::new(move |line: &str| {
        sink.lock().unwrap().push_str(line);
    });
    tracing::subscriber::with_default(tracing_subscriber::registry().with(layer), log);
    let text = lines.lock().unwrap().clone();
    text
}

/// A real production call site, which the template table knows: the run log
/// logs `could not move the previous log aside` when a directory sits where
/// the previous log belongs.
fn move_the_previous_log_aside_and_fail() {
    let root = tempfile::tempdir().unwrap();
    let previous = reprise_core::state_root::previous_log_path_in(root.path());
    std::fs::create_dir_all(&previous).unwrap();
    let _ = reprise_core::run_log::RunLog::start_in(root.path());
}

#[test]
fn an_own_event_is_its_static_template_its_redacted_error_and_its_call_site() {
    let text = mirrored(move_the_previous_log_aside_and_fail);

    assert_eq!(text.lines().count(), 1, "{text}");
    assert!(text.ends_with('\n'), "{text}");
    let body = text.trim_end().split_once(": ").unwrap().1;
    let (message, location) = body.rsplit_once(" @ ").unwrap();
    assert_eq!(
        message,
        "could not move the previous log aside; error=$REDACTED"
    );
    assert!(
        location.starts_with("crates/reprise-core/src/run_log.rs:"),
        "{location}"
    );
    assert!(
        location["crates/reprise-core/src/run_log.rs:".len()..]
            .chars()
            .all(|ch| ch.is_ascii_digit()),
        "{location}"
    );
    // The error text and the temporary directory never reach the file.
    for leaked in ["tmp", "Is a directory", "directory"] {
        assert!(
            !text.contains(leaked),
            "{leaked:?} reached the file: {text}"
        );
    }
}

#[test]
fn a_foreign_event_is_its_level_its_target_and_an_ellipsis() {
    let text = mirrored(|| {
        tracing::warn!(
            target: "foreign_transport::connection",
            path = "/home/zqxuser/Music/Zqx Artist/zqx.flac",
            count = 3,
            "could not read /home/zqxuser/Music/Zqx Artist/zqx.flac for zqxuser"
        );
    });

    assert!(
        text.ends_with(" WARN foreign_transport::connection: …\n"),
        "{text}"
    );
    for leaked in ["Zqx", "zqx", "/home", "count=", " @ "] {
        assert!(
            !text.contains(leaked),
            "{leaked:?} reached the file: {text}"
        );
    }
}

#[test]
fn a_line_names_its_time_level_and_target() {
    let text = mirrored(|| tracing::info!(target: "foreign_crate", "starting"));

    let timestamp = text.split(' ').next().unwrap_or_default();
    assert!(
        timestamp.len() == "2026-11-08T12:00:00.123Z".len() && timestamp.ends_with('Z'),
        "unexpected line start: {text}"
    );
    assert!(text.contains(" INFO foreign_crate: "), "{text}");
}

#[test]
fn a_message_with_line_breaks_cannot_forge_a_second_line() {
    let text = mirrored(|| {
        tracing::error!(
            target: "foreign_crate",
            "boom\n2026-11-08T12:00:00.123Z  INFO reprise: forged \u{2028} line"
        );
    });

    assert_eq!(text.lines().count(), 1, "{text}");
    assert!(!text.contains("forged"), "{text}");
}

#[test]
fn every_event_reaches_the_sink_whatever_its_level() {
    let text = mirrored(|| {
        tracing::debug!(target: "foreign_crate", "d");
        tracing::trace!(target: "foreign_crate", "t");
    });

    assert!(text.contains(" DEBUG foreign_crate: …"), "{text}");
    assert!(text.contains(" TRACE foreign_crate: …"), "{text}");
}
