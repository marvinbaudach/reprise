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

use std::backtrace::Backtrace;
use std::fmt::Write as _;
use std::io::{self, Write as _};
use std::panic::Location;
use std::path::Path;

use crate::diagnostics::{reduce_backtrace, reduce_location, reduce_message, PanicPayload};
use crate::state_root;

/// The line that opens a crash report in the log; the diagnostics tail looks for it.
pub(crate) const CRASH_REPORT_MARKER: &str = "---- Reprise crash report ----";
/// The line that closes a crash report; what follows it in the log is not part of it.
pub(crate) const CRASH_REPORT_END_MARKER: &str = "---- end of crash report ----";

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
///
/// The report is written already reduced. The message is the static payload or
/// a standard phrase or log template, never the formatted text; the location
/// and the backtrace lines are kept only in the shapes of
/// `diagnostics::crash_reduce`.
pub fn write_report(
    payload: &PanicPayload,
    location: Option<&Location<'_>>,
    backtrace: &Backtrace,
) {
    let Some(log) = state_root::log_path() else {
        return;
    };
    write_report_to(
        &log,
        payload,
        location.map(|at| (at.file(), at.line(), at.column())),
        &backtrace.to_string(),
        &chrono::Utc::now().to_rfc3339(),
    );
}

fn write_report_to(
    log: &Path,
    payload: &PanicPayload,
    location: Option<(&str, u32, u32)>,
    backtrace: &str,
    timestamp: &str,
) {
    let report = render_report(payload, location, backtrace, timestamp);
    // Nowhere left to report a failure to: the process is already panicking.
    let _ = append_to(log, &report);
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

/// The report text, every free-form part already reduced. `location` is the
/// panic's file, line and column.
pub(crate) fn render_report(
    payload: &PanicPayload,
    location: Option<(&str, u32, u32)>,
    backtrace: &str,
    timestamp: &str,
) -> String {
    let location_text = location.map_or_else(
        || "unknown location".to_owned(),
        |(file, line, column)| reduce_location(&format!("{file}:{line}:{column}")),
    );
    let mut report = String::new();
    let _ = writeln!(report, "{CRASH_REPORT_MARKER}");
    let _ = writeln!(report, "time: {timestamp}");
    let _ = writeln!(report, "version: {}", env!("CARGO_PKG_VERSION"));
    let _ = writeln!(report, "schema: {}", crate::db::SUPPORTED_SCHEMA_VERSION);
    let _ = writeln!(report, "location: {location_text}");
    let _ = writeln!(
        report,
        "message: {}",
        reduce_message(payload, location.map(|(file, ..)| file))
    );
    let _ = writeln!(report, "backtrace:\n{}", reduce_backtrace(backtrace));
    let _ = writeln!(report, "{CRASH_REPORT_END_MARKER}");
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::diagnostics::{
        read_log_tail, render_report as render_diagnostics, DiagnosticFacts, DiagnosticLog,
    };

    const TIME: &str = "2026-11-01T10:00:00Z";

    fn report(
        payload: &PanicPayload,
        location: Option<(&str, u32, u32)>,
        backtrace: &str,
    ) -> String {
        render_report(payload, location, backtrace, TIME)
    }

    fn formatted(text: &str) -> PanicPayload {
        PanicPayload::Formatted(text.to_owned())
    }

    #[test]
    fn crash_report_contains_version_schema_and_the_panic_location() {
        let report = report(
            &formatted("called `Option::unwrap()` on a `None` value"),
            Some(("crates/reprise-gnome/src/ui/window/window.rs", 101, 17)),
            "   0: reprise::ui::window::build",
        );

        assert!(report.contains(&format!("version: {}", env!("CARGO_PKG_VERSION"))));
        assert!(report.contains(&format!("schema: {}", crate::db::SUPPORTED_SCHEMA_VERSION)));
        assert!(report.contains(&format!("time: {TIME}\n")));
        assert!(report.contains("location: crates/reprise-gnome/src/ui/window/window.rs:101:17"));
        assert!(report.contains("message: called `Option::unwrap()` on a `None` value\n"));
        assert!(report.contains("0: reprise::ui::window::build"));
        assert!(report.starts_with(CRASH_REPORT_MARKER));
        assert!(report.ends_with("---- end of crash report ----\n"));
    }

    #[test]
    fn a_panic_without_a_location_says_so() {
        let report = report(&PanicPayload::Static("boom"), None, "");

        assert!(report.contains("location: unknown location\n"), "{report}");
        assert!(report.contains("message: boom\n"));
    }

    #[test]
    fn start_6_a_crash_section_never_contains_a_library_path() {
        let report = report(
            &formatted(
                "could not open /home/zqxuser/Music/Zqx Artist/Zqx Album/01 Zqx Song.flac and /mnt/zqxnas/Zqx Other Album/02.ogg",
            ),
            Some((
                "/home/zqxuser/.cargo/registry/src/index/zqxcrate-1.0.0/src/lib.rs",
                5,
                9,
            )),
            "   3: zqx_app::open\n             at /home/zqxuser/Music/build/src/main.rs:12:5\n   4: zqx path /home/zqxuser/Zqx.flac\nplain /mnt/zqxnas/Zqx Other Album",
        );

        assert!(!report.to_lowercase().contains("/home/"), "{report}");
        assert!(!report.contains("Album"), "{report}");
        assert!(!report.contains("/mnt"), "{report}");
        assert!(report.contains("location: …/zqxcrate-1.0.0/src/lib.rs:5:9\n"));
        assert!(report.contains("message: …\n"));
    }

    /// The path of a file outside the home directory, and one inside it: the
    /// two shapes a library path takes.
    const OUTSIDE: &str = "/srv/zqxshare/Zqx Album/01.flac";
    const INSIDE: &str = "/home/zqxuser/Music/Zqx Artist/02.flac";

    #[test]
    fn start_6_a_crash_written_to_the_log_reaches_the_copied_report_without_a_needle() {
        let root = tempfile::tempdir().unwrap();
        let log = state_root::log_path_in(root.path());
        let backtrace = format!(
            "   0: zqx::{OUTSIDE}\n             at {OUTSIDE}:3:4\n   1: reprise::main\n             at {INSIDE}:9:9\n   2: <{INSIDE} as zqx::Trait>::run\n{OUTSIDE}\n"
        );

        write_report_to(
            &log,
            &formatted(&format!("could not read {OUTSIDE} nor {INSIDE}")),
            Some((OUTSIDE, 7, 8)),
            &backtrace,
            TIME,
        );
        let facts = DiagnosticFacts {
            log_tail: read_log_tail(&log),
            ..DiagnosticFacts::default()
        };
        let copied = render_diagnostics(&facts, &DiagnosticLog::default());

        assert!(copied.contains(CRASH_REPORT_MARKER), "{copied}");
        assert!(copied.contains("message: …"));
        assert!(copied.contains("location: …:7:8"));
        assert!(copied.contains("   1: reprise::main"));
        assert!(
            !copied.to_lowercase().contains("zqx"),
            "a needle reached the copied report:\n{copied}"
        );
    }

    #[test]
    fn a_static_payload_and_a_standard_phrase_reach_the_copied_report() {
        let root = tempfile::tempdir().unwrap();
        let log = state_root::log_path_in(root.path());

        write_report_to(
            &log,
            &formatted("index out of bounds: the len is 3 but the index is 7"),
            Some(("crates/reprise-core/src/queue.rs", 7, 8)),
            "   0: reprise_core::queue::advance",
            TIME,
        );
        let facts = DiagnosticFacts {
            log_tail: read_log_tail(&log),
            ..DiagnosticFacts::default()
        };
        let copied = render_diagnostics(&facts, &DiagnosticLog::default());

        assert!(copied.contains("message: index out of bounds: the len is … but the index is …"));
        assert!(copied.contains("location: crates/reprise-core/src/queue.rs:7:8"));
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
