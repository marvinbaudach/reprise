//! Paths and rotation for the small amount of persistent application state.
//!
//! Test builds use a process-private directory so they cannot inspect or alter
//! the user's saved diagnostics.

use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};

pub const MAX_LOG_BYTES: u64 = 8 * 1024 * 1024;

const APP_DIRECTORY: &str = "reprise";
const LOG_FILE: &str = "reprise.log";
const PREVIOUS_LOG_FILE: &str = "reprise.log.1";
const CRASH_MARKER_FILE: &str = "last-crash";

/// The active log: `<state root>/reprise/reprise.log`.
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
        std::fs::create_dir_all(parent)?;
    }
    rotate_paths(&active, &previous)
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
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    #[test]
    fn log_path_follows_xdg_state_home() {
        let root = state_root_from(Some("/state".into()), Some("/data".into()));
        assert_eq!(
            root.as_deref().map(log_path_in),
            Some(PathBuf::from("/state/reprise/reprise.log"))
        );
    }

    #[test]
    fn log_path_falls_back_to_the_data_dir_when_state_home_is_unset() {
        let data = Some(PathBuf::from("/data"));
        for unset in [None, Some(OsString::new()), Some("relative/state".into())] {
            let root = state_root_from(unset, data.clone());
            assert_eq!(
                root.as_deref().map(log_path_in),
                Some(PathBuf::from("/data/reprise/reprise.log"))
            );
        }
        assert_eq!(state_root_from(None, None), None);
    }

    #[test]
    fn the_marker_and_the_previous_log_sit_beside_the_log() {
        let root = Path::new("/state");
        assert_eq!(
            previous_log_path_in(root),
            PathBuf::from("/state/reprise/reprise.log.1")
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
    fn rotation_reports_but_survives_a_read_only_directory() {
        let root = tempfile::tempdir().unwrap();
        let active = log_path_in(root.path());
        let directory = active.parent().unwrap().to_path_buf();
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(&active, "current").unwrap();
        let previous = previous_log_path_in(root.path());
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o555)).unwrap();

        let result = rotate_paths(&active, &previous);

        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(result.is_err());
        assert_eq!(std::fs::read_to_string(active).unwrap(), "current");
    }
}
