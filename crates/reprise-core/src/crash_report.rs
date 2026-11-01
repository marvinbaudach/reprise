//! Evidence a crash leaves behind: the running sentinel and the panic report.
//!
//! The sentinel is a small file written at startup and removed on a clean
//! shutdown. It records an end that nobody got to report, a kill or a native
//! abort that never reaches the panic hook, as well as a panic: the next start
//! finds the file still there. The panic report goes into the log file, with
//! every value a tester would not want to publish removed first.
//!
//! The pure parts take their paths and context explicitly so tests do not
//! depend on the process environment.

use std::fmt::Write as _;
use std::io::{self, Write as _};
use std::path::Path;

use crate::diagnostics::{redact_log_message, RedactionContext};
use crate::state_root;

/// How the run before this one ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreviousExit {
    /// It removed its sentinel, or there was no earlier run.
    Clean,
    /// Its sentinel was still there.
    Unclean,
}

/// Writes the sentinel for this run and reports how the previous one ended. A
/// state directory that cannot be written reports `Clean`: nothing is known,
/// and an offer built on a guess would be noise.
#[must_use]
pub fn arm() -> PreviousExit {
    let Some(marker) = state_root::crash_marker_path() else {
        return PreviousExit::Clean;
    };
    let contents = marker_contents(
        env!("CARGO_PKG_VERSION"),
        crate::db::SUPPORTED_SCHEMA_VERSION,
        &chrono::Utc::now().to_rfc3339(),
    );
    arm_at(&marker, &contents).unwrap_or_else(|error| {
        tracing::warn!(%error, "could not write the running marker");
        PreviousExit::Clean
    })
}

/// Removes the sentinel: this run ended the way it meant to.
pub fn disarm() {
    let Some(marker) = state_root::crash_marker_path() else {
        return;
    };
    if let Err(error) = disarm_at(&marker) {
        tracing::warn!(%error, "could not remove the running marker");
    }
}

/// Appends a panic report to the log file, ignoring its size cap: the report
/// is the reason the log exists. Called from the panic hook, so it never
/// panics and reports nothing back.
pub fn write_report(message: &str, location: &str, backtrace: &str) {
    let Some(log) = state_root::log_path() else {
        return;
    };
    let report = render_report(
        message,
        location,
        backtrace,
        &redaction_context(),
        &chrono::Utc::now().to_rfc3339(),
    );
    // Nowhere left to report a failure to: the process is already panicking.
    let _ = append_to(&log, &report);
}

fn marker_contents(version: &str, schema: i64, started: &str) -> String {
    format!("version: {version}\nschema: {schema}\nstarted: {started}\n")
}

fn arm_at(marker: &Path, contents: &str) -> io::Result<PreviousExit> {
    let previous = if marker.exists() {
        PreviousExit::Unclean
    } else {
        PreviousExit::Clean
    };
    if let Some(directory) = marker.parent() {
        state_root::create_private_dir(directory)?;
    }
    state_root::private_file_options()
        .write(true)
        .truncate(true)
        .open(marker)?
        .write_all(contents.as_bytes())?;
    Ok(previous)
}

fn disarm_at(marker: &Path) -> io::Result<()> {
    match std::fs::remove_file(marker) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

fn append_to(log: &Path, report: &str) -> io::Result<()> {
    if let Some(directory) = log.parent() {
        state_root::create_private_dir(directory)?;
    }
    state_root::private_file_options()
        .append(true)
        .open(log)?
        .write_all(report.as_bytes())
}

fn redaction_context() -> RedactionContext {
    let variable = |name| std::env::var(name).ok().filter(|value| !value.is_empty());
    RedactionContext {
        music_dir: dirs::audio_dir().map(|path| path.to_string_lossy().into_owned()),
        home_dir: variable("HOME"),
        username: variable("USER"),
    }
}

/// The report text. The panic message is free text and may name a file of the
/// library, so it goes through the same redaction as a copied diagnostics
/// report. Location and backtrace are source paths and symbol names; they
/// only lose the home and music directories a build path can sit in.
fn render_report(
    message: &str,
    location: &str,
    backtrace: &str,
    context: &RedactionContext,
    timestamp: &str,
) -> String {
    let mut report = String::new();
    let _ = writeln!(report, "---- Reprise crash report ----");
    let _ = writeln!(report, "time: {timestamp}");
    let _ = writeln!(report, "version: {}", env!("CARGO_PKG_VERSION"));
    let _ = writeln!(report, "schema: {}", crate::db::SUPPORTED_SCHEMA_VERSION);
    let _ = writeln!(report, "location: {}", scrub_directories(location, context));
    let _ = writeln!(report, "message: {}", redact_log_message(message, context));
    let _ = writeln!(
        report,
        "backtrace:\n{}",
        scrub_directories(backtrace, context)
    );
    let _ = writeln!(report, "---- end of crash report ----");
    report
}

fn scrub_directories(text: &str, context: &RedactionContext) -> String {
    let mut scrubbed = text.to_owned();
    for (directory, replacement) in [
        (&context.music_dir, "$XDG_MUSIC_DIR"),
        (&context.home_dir, "$HOME"),
    ] {
        if let Some(directory) = directory.as_deref().filter(|value| !value.is_empty()) {
            scrubbed = scrubbed.replace(directory, replacement);
        }
    }
    scrubbed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context() -> RedactionContext {
        RedactionContext {
            music_dir: Some("/home/tester/Music".to_owned()),
            home_dir: Some("/home/tester".to_owned()),
            username: Some("tester".to_owned()),
        }
    }

    fn report(message: &str, location: &str, backtrace: &str) -> String {
        render_report(
            message,
            location,
            backtrace,
            &context(),
            "2026-11-01T10:00:00Z",
        )
    }

    #[test]
    fn crash_report_contains_version_schema_and_the_panic_location() {
        let report = report(
            "called `Option::unwrap()` on a `None` value",
            "crates/reprise-gnome/src/ui/window/window.rs:101:17",
            "   0: reprise::ui::window::build",
        );

        assert!(report.contains(&format!("version: {}", env!("CARGO_PKG_VERSION"))));
        assert!(report.contains(&format!("schema: {}", crate::db::SUPPORTED_SCHEMA_VERSION)));
        assert!(report.contains("location: crates/reprise-gnome/src/ui/window/window.rs:101:17"));
        assert!(report.contains("called `Option::unwrap()` on a `None` value"));
        assert!(report.contains("0: reprise::ui::window::build"));
    }

    #[test]
    fn crash_report_never_contains_a_library_path() {
        let report = report(
            "could not open /home/tester/Music/Artist/Album/01 Song.flac and /mnt/nas/Other Album/02.ogg",
            "/home/tester/.cargo/registry/src/index/crate-1.0/src/lib.rs:5:9",
            "at /home/tester/Music/build/src/main.rs:12:5",
        );

        for leaked in [
            "Artist",
            "Song.flac",
            "Other Album",
            "/mnt/nas",
            "/home/tester",
        ] {
            assert!(
                !report.contains(leaked),
                "{leaked:?} leaked into:\n{report}"
            );
        }
        assert!(report.contains("$HOME/.cargo/registry/src/index/crate-1.0/src/lib.rs:5:9"));
    }

    #[test]
    fn the_marker_names_version_schema_and_start() {
        let contents = marker_contents("0.1.9", 92, "2026-11-01T10:00:00Z");

        assert_eq!(
            contents,
            "version: 0.1.9\nschema: 92\nstarted: 2026-11-01T10:00:00Z\n"
        );
    }

    #[test]
    fn arming_reports_an_earlier_run_that_never_disarmed() {
        let root = tempfile::tempdir().unwrap();
        let marker = state_root::crash_marker_path_in(root.path());

        assert_eq!(arm_at(&marker, "first").unwrap(), PreviousExit::Clean);
        assert!(marker.exists(), "a running process leaves its sentinel");
        assert_eq!(arm_at(&marker, "second").unwrap(), PreviousExit::Unclean);
        assert_eq!(std::fs::read_to_string(&marker).unwrap(), "second");
    }

    #[test]
    fn a_clean_shutdown_leaves_the_next_start_with_nothing_to_report() {
        let root = tempfile::tempdir().unwrap();
        let marker = state_root::crash_marker_path_in(root.path());
        arm_at(&marker, "running").unwrap();

        disarm_at(&marker).unwrap();
        disarm_at(&marker).unwrap();

        assert!(!marker.exists());
        assert_eq!(arm_at(&marker, "next").unwrap(), PreviousExit::Clean);
    }

    #[test]
    fn a_report_is_appended_after_what_the_log_already_holds() {
        let root = tempfile::tempdir().unwrap();
        let log = state_root::log_path_in(root.path());
        std::fs::create_dir_all(log.parent().unwrap()).unwrap();
        std::fs::write(&log, "earlier line\n").unwrap();

        append_to(&log, "the report\n").unwrap();

        assert_eq!(
            std::fs::read_to_string(&log).unwrap(),
            "earlier line\nthe report\n"
        );
    }

    #[cfg(unix)]
    #[test]
    fn the_marker_and_the_reports_are_readable_by_the_owner_alone() {
        use std::os::unix::fs::PermissionsExt;
        let mode = |path: &Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
        let root = tempfile::tempdir().unwrap();
        let marker = state_root::crash_marker_path_in(root.path());
        let log = state_root::log_path_in(root.path());

        arm_at(&marker, "running").unwrap();
        append_to(&log, "report\n").unwrap();

        assert_eq!(mode(marker.parent().unwrap()), 0o700);
        assert_eq!(mode(&marker), 0o600);
        assert_eq!(mode(&log), 0o600);
    }
}
