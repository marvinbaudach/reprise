//! Paths and rotation for the small amount of persistent application state.
//!
//! Test builds use a process-private directory so they cannot inspect or alter
//! the user's saved diagnostics.

use std::ffi::OsString;
use std::fs::OpenOptions;
use std::io;
use std::path::{Path, PathBuf};

pub const MAX_LOG_BYTES: u64 = 8 * 1024 * 1024;

const APP_DIRECTORY: &str = "reprise";
const LOG_FILE: &str = "run.log";
const PREVIOUS_LOG_FILE: &str = "run.log.1";
/// The names an earlier version wrote. Its lines passed the heuristic
/// redaction, so a report must never read them; the new names keep a file of
/// that kind out of every report, and the first instance deletes it.
const LEGACY_LOG_FILES: [&str; 2] = ["reprise.log", "reprise.log.1"];
const CRASH_MARKER_FILE: &str = "last-crash";
#[cfg(unix)]
const PRIVATE_DIRECTORY_MODE: u32 = 0o700;
#[cfg(unix)]
const PRIVATE_FILE_MODE: u32 = 0o600;

/// The active log: `<state root>/reprise/run.log`.
#[must_use]
pub fn log_path() -> Option<PathBuf> {
    root().map(|root| log_path_in(&root))
}

/// The previous run's log, kept as exactly one generation.
#[must_use]
pub fn previous_log_path() -> Option<PathBuf> {
    root().map(|root| previous_log_path_in(&root))
}

/// The running sentinel: present while a run has not ended cleanly.
#[must_use]
pub fn crash_marker_path() -> Option<PathBuf> {
    root().map(|root| crash_marker_path_in(&root))
}

/// Moves an existing log aside so a run starts on a clean file. The caller logs
/// the error and carries on: a read-only state directory must never keep the
/// application from starting.
pub fn rotate_on_start() -> io::Result<()> {
    let (Some(active), Some(previous)) = (log_path(), previous_log_path()) else {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "state directory unavailable",
        ));
    };
    if let Some(parent) = active.parent() {
        create_private_dir(parent)?;
    }
    rotate_paths(&active, &previous)
}

/// Deletes the run logs an earlier version wrote (`reprise.log` and
/// `reprise.log.1`) from the state directory, and nothing else. The first
/// instance calls it once at start. A file that is already gone is fine; any
/// other failure is returned after both names were tried.
pub fn remove_legacy_logs() -> io::Result<()> {
    match root() {
        Some(root) => remove_legacy_logs_in(&root),
        None => Ok(()),
    }
}

/// [`remove_legacy_logs`] beneath a supplied state root.
pub fn remove_legacy_logs_in(root: &Path) -> io::Result<()> {
    let directory = app_directory_in(root);
    let mut failure = None;
    for name in LEGACY_LOG_FILES {
        match std::fs::remove_file(directory.join(name)) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => failure = failure.or(Some(error)),
        }
    }
    failure.map_or(Ok(()), Err)
}

/// Builds the active log path beneath a supplied state root.
#[must_use]
pub fn log_path_in(root: &Path) -> PathBuf {
    app_directory_in(root).join(LOG_FILE)
}

/// Builds the previous-run log path beneath a supplied state root.
#[must_use]
pub fn previous_log_path_in(root: &Path) -> PathBuf {
    app_directory_in(root).join(PREVIOUS_LOG_FILE)
}

/// Builds the running-sentinel path beneath a supplied state root.
#[must_use]
pub fn crash_marker_path_in(root: &Path) -> PathBuf {
    app_directory_in(root).join(CRASH_MARKER_FILE)
}

fn app_directory_in(root: &Path) -> PathBuf {
    root.join(APP_DIRECTORY)
}

/// `$XDG_STATE_HOME` when it is an absolute path (the XDG rule: an empty or
/// relative value counts as unset), otherwise the data directory.
#[must_use]
pub fn state_root_from(
    xdg_state_home: Option<OsString>,
    data_root: Option<PathBuf>,
) -> Option<PathBuf> {
    xdg_state_home
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or(data_root)
}

/// The directory the `reprise/` state folder hangs below.
#[must_use]
pub fn root() -> Option<PathBuf> {
    #[cfg(any(test, feature = "test-private-dirs"))]
    {
        Some(isolated_root().to_path_buf())
    }
    #[cfg(not(any(test, feature = "test-private-dirs")))]
    {
        state_root_from(
            std::env::var_os("XDG_STATE_HOME"),
            crate::data_root::user_data_root(),
        )
    }
}

#[cfg(any(test, feature = "test-private-dirs"))]
fn isolated_root() -> &'static Path {
    static ROOT: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    ROOT.get_or_init(|| {
        std::env::temp_dir().join(format!("reprise-test-state-{}", std::process::id()))
    })
}

/// Creates the state directory (and what is missing above it) readable by the
/// owner alone: a log and a crash report describe one person's machine.
pub(crate) fn create_private_dir(directory: &Path) -> io::Result<()> {
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, PRIVATE_DIRECTORY_MODE);
    builder.create(directory)
}

/// Options for a state file that only its owner may read or write. The mode
/// applies when the file is created; rotation keeps it.
pub(crate) fn private_file_options() -> OpenOptions {
    let mut options = OpenOptions::new();
    options.create(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, PRIVATE_FILE_MODE);
    options
}

pub(crate) fn rotate_paths(active: &Path, previous: &Path) -> io::Result<()> {
    match std::fs::remove_file(previous) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    match std::fs::rename(active, previous) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_path_follows_xdg_state_home() {
        let root = state_root_from(Some("/state".into()), Some("/data".into()));
        assert_eq!(
            root.as_deref().map(log_path_in),
            Some(PathBuf::from("/state/reprise/run.log"))
        );
    }

    #[test]
    fn log_path_falls_back_to_the_data_dir_when_state_home_is_unset() {
        let data = Some(PathBuf::from("/data"));
        for unset in [None, Some(OsString::new()), Some("relative/state".into())] {
            let root = state_root_from(unset, data.clone());
            assert_eq!(
                root.as_deref().map(log_path_in),
                Some(PathBuf::from("/data/reprise/run.log"))
            );
        }
        assert_eq!(state_root_from(None, None), None);
    }

    #[test]
    fn the_marker_and_the_previous_log_sit_beside_the_log() {
        let root = Path::new("/state");
        assert_eq!(
            previous_log_path_in(root),
            PathBuf::from("/state/reprise/run.log.1")
        );
        assert_eq!(
            crash_marker_path_in(root),
            PathBuf::from("/state/reprise/last-crash")
        );
    }

    #[test]
    fn rotation_keeps_exactly_one_previous_run() {
        let root = tempfile::tempdir().unwrap();
        let active = log_path_in(root.path());
        std::fs::create_dir_all(active.parent().unwrap()).unwrap();
        std::fs::write(&active, "current").unwrap();
        let previous = previous_log_path_in(root.path());
        std::fs::write(&previous, "older").unwrap();

        rotate_paths(&active, &previous).unwrap();

        assert_eq!(std::fs::read_to_string(previous).unwrap(), "current");
        assert!(!active.exists());
    }

    #[test]
    fn rotation_reports_a_failure_and_keeps_the_current_log() {
        // A directory where the previous log belongs makes the rotation fail for
        // every user, root included; a read-only directory would not stop root.
        let root = tempfile::tempdir().unwrap();
        let active = log_path_in(root.path());
        std::fs::create_dir_all(active.parent().unwrap()).unwrap();
        std::fs::write(&active, "current").unwrap();
        let previous = previous_log_path_in(root.path());
        std::fs::create_dir(&previous).unwrap();

        let result = rotate_paths(&active, &previous);

        assert!(result.is_err());
        assert_eq!(std::fs::read_to_string(active).unwrap(), "current");
    }

    #[test]
    fn start_6_the_first_instance_removes_the_old_log_names_and_nothing_else() {
        let root = tempfile::tempdir().unwrap();
        let directory = app_directory_in(root.path());
        std::fs::create_dir_all(&directory).unwrap();
        for name in [
            "reprise.log",
            "reprise.log.1",
            "run.log",
            "run.log.1",
            "last-crash",
        ] {
            std::fs::write(directory.join(name), name).unwrap();
        }
        // A look-alike beside the state directory is not ours to delete.
        std::fs::write(root.path().join("reprise.log"), "elsewhere").unwrap();

        remove_legacy_logs_in(root.path()).unwrap();

        let mut left: Vec<_> = std::fs::read_dir(&directory)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        left.sort();
        assert_eq!(left, ["last-crash", "run.log", "run.log.1"]);
        assert!(root.path().join("reprise.log").exists());
    }

    #[test]
    fn start_6_a_state_directory_without_old_logs_is_not_an_error() {
        let root = tempfile::tempdir().unwrap();
        // Neither the state directory nor the files exist.
        remove_legacy_logs_in(root.path()).unwrap();
        // The directory exists, only one of the two names does.
        let directory = app_directory_in(root.path());
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join("reprise.log.1"), "old").unwrap();

        remove_legacy_logs_in(root.path()).unwrap();

        assert!(!directory.join("reprise.log.1").exists());
    }

    #[test]
    fn start_6_a_failed_removal_is_reported_after_both_names_were_tried() {
        // A directory where an old log belongs makes `remove_file` fail for
        // every user, root included.
        let root = tempfile::tempdir().unwrap();
        let directory = app_directory_in(root.path());
        std::fs::create_dir_all(directory.join("reprise.log")).unwrap();
        std::fs::write(directory.join("reprise.log.1"), "old").unwrap();

        let result = remove_legacy_logs_in(root.path());

        assert!(result.is_err());
        assert!(!directory.join("reprise.log.1").exists());
    }

    #[test]
    fn the_run_log_has_its_own_names_in_the_same_directory() {
        let root = Path::new("/state");
        assert_eq!(log_path_in(root), PathBuf::from("/state/reprise/run.log"));
        assert_eq!(
            previous_log_path_in(root),
            PathBuf::from("/state/reprise/run.log.1")
        );
    }
}
