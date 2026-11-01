//! The file the running application mirrors its log into.
//!
//! The file holds exactly one run: [`RunLog::start_in`] moves the previous run
//! aside before the first byte is written. A run that logs more than
//! [`state_root::MAX_LOG_BYTES`] stops writing instead of rotating a second
//! time, because the log worth having in full is the one from the crash, and a
//! rotation in the middle would hand out half of it.
//!
//! Callers decide *when* the file may be touched: a second `reprise` launch
//! only forwards its request to the first and must never rotate the first
//! one's live log away. [`DeferredRunLog`] therefore holds early output back
//! until its owner either activates it or discards it.

use std::fs::File;
use std::io::{self, Write};
use std::path::Path;

use crate::state_root;

/// What a [`DeferredRunLog`] keeps while it waits to learn whether it may
/// write. Startup logs a handful of lines before that is known.
const MAX_PENDING_BYTES: usize = 64 * 1024;

/// An append-only log file with a byte budget.
#[derive(Debug)]
pub struct RunLog {
    file: File,
    written: u64,
    cap: u64,
}

impl RunLog {
    /// Moves the previous run's log aside and opens a fresh one beneath
    /// `root`. Fails when the state directory cannot be created or the file
    /// cannot be opened; a rotation that fails on its own is only reported
    /// through `tracing`, because the log is still worth writing.
    pub fn start_in(root: &Path) -> io::Result<Self> {
        Self::start_in_capped(root, state_root::MAX_LOG_BYTES)
    }

    fn start_in_capped(root: &Path, cap: u64) -> io::Result<Self> {
        let active = state_root::log_path_in(root);
        if let Some(directory) = active.parent() {
            state_root::create_private_dir(directory)?;
        }
        if let Err(error) =
            state_root::rotate_paths(&active, &state_root::previous_log_path_in(root))
        {
            tracing::warn!(%error, "could not move the previous log aside");
        }
        let file = state_root::private_file_options()
            .append(true)
            .open(active)?;
        Ok(Self {
            file,
            written: 0,
            cap,
        })
    }

    /// The same as [`Self::start_in`] for the real state directory, or `None`
    /// when there is none or it cannot be written. Logging never keeps the
    /// application from starting.
    #[must_use]
    pub fn start() -> Option<Self> {
        Self::start_in(&state_root::root()?).ok()
    }
}

impl Write for RunLog {
    /// Past the cap a write is reported as done and dropped, so a full log is
    /// not an error for the logging layer above it.
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let length = u64::try_from(buf.len()).unwrap_or(u64::MAX);
        if self.written.saturating_add(length) > self.cap {
            return Ok(buf.len());
        }
        self.file.write_all(buf)?;
        self.written += length;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

/// A [`Write`] target whose destination is decided later.
#[derive(Debug, Default)]
pub struct DeferredRunLog {
    state: State,
}

#[derive(Debug)]
enum State {
    Pending(Vec<u8>),
    Active(RunLog),
    Off,
}

impl Default for State {
    fn default() -> Self {
        Self::Pending(Vec::new())
    }
}

impl DeferredRunLog {
    /// Starts writing: what was held back goes into `log` first.
    pub fn activate(&mut self, mut log: RunLog) {
        if let State::Pending(held) = &self.state {
            // A failed first write only loses the lines that were held back.
            let _ = log.write_all(held);
        }
        self.state = State::Active(log);
    }

    /// Gives up on the file, for good: this process is not the one to log.
    pub fn discard(&mut self) {
        self.state = State::Off;
    }

    #[must_use]
    pub fn is_active(&self) -> bool {
        matches!(self.state, State::Active(_))
    }
}

impl Write for DeferredRunLog {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match &mut self.state {
            State::Pending(held) => {
                if held.len().saturating_add(buf.len()) <= MAX_PENDING_BYTES {
                    held.extend_from_slice(buf);
                }
                Ok(buf.len())
            }
            State::Active(log) => log.write(buf),
            State::Off => Ok(buf.len()),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match &mut self.state {
            State::Active(log) => log.flush(),
            State::Pending(_) | State::Off => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(path: &Path) -> String {
        std::fs::read_to_string(path).unwrap()
    }

    #[test]
    fn file_writer_is_absent_when_the_state_directory_cannot_be_created() {
        let root = tempfile::tempdir().unwrap();
        let blocker = root.path().join("not-a-directory");
        std::fs::write(&blocker, "a regular file").unwrap();

        assert!(RunLog::start_in(&blocker).is_err());
    }

    #[test]
    fn starting_moves_the_previous_run_aside_and_begins_a_clean_file() {
        let root = tempfile::tempdir().unwrap();
        let active = state_root::log_path_in(root.path());
        std::fs::create_dir_all(active.parent().unwrap()).unwrap();
        std::fs::write(&active, "last run\n").unwrap();

        let mut log = RunLog::start_in(root.path()).unwrap();
        log.write_all(b"this run\n").unwrap();

        assert_eq!(read(&active), "this run\n");
        assert_eq!(
            read(&state_root::previous_log_path_in(root.path())),
            "last run\n"
        );
    }

    #[test]
    fn writes_past_the_cap_are_dropped_without_a_second_rotation() {
        let root = tempfile::tempdir().unwrap();
        let mut log = RunLog::start_in_capped(root.path(), 10).unwrap();

        log.write_all(b"123456").unwrap();
        log.write_all(b"too long to fit").unwrap();
        log.write_all(b"789").unwrap();

        assert_eq!(read(&state_root::log_path_in(root.path())), "123456789");
        assert!(!state_root::previous_log_path_in(root.path()).exists());
    }

    #[test]
    fn a_deferred_log_holds_early_lines_until_it_is_activated() {
        let root = tempfile::tempdir().unwrap();
        let mut deferred = DeferredRunLog::default();
        deferred.write_all(b"early\n").unwrap();
        assert!(!state_root::log_path_in(root.path()).exists());

        deferred.activate(RunLog::start_in(root.path()).unwrap());
        deferred.write_all(b"late\n").unwrap();

        assert!(deferred.is_active());
        assert_eq!(read(&state_root::log_path_in(root.path())), "early\nlate\n");
    }

    #[test]
    fn a_discarded_deferred_log_never_touches_the_file() {
        let root = tempfile::tempdir().unwrap();
        let active = state_root::log_path_in(root.path());
        std::fs::create_dir_all(active.parent().unwrap()).unwrap();
        std::fs::write(&active, "the first instance's log\n").unwrap();
        let mut deferred = DeferredRunLog::default();
        deferred.write_all(b"secondary\n").unwrap();

        deferred.discard();
        deferred.write_all(b"after\n").unwrap();

        assert!(!deferred.is_active());
        assert_eq!(read(&active), "the first instance's log\n");
    }

    #[test]
    fn a_deferred_log_stops_holding_lines_at_its_limit() {
        let mut deferred = DeferredRunLog::default();
        let line = vec![b'x'; MAX_PENDING_BYTES];

        deferred.write_all(&line).unwrap();
        deferred.write_all(b"one byte too many").unwrap();

        let State::Pending(held) = &deferred.state else {
            panic!("a log nobody activated stays pending");
        };
        assert_eq!(held.len(), MAX_PENDING_BYTES);
    }

    #[cfg(unix)]
    #[test]
    fn the_state_directory_and_the_logs_are_readable_by_the_owner_alone() {
        use std::os::unix::fs::PermissionsExt;
        let mode = |path: &Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
        let root = tempfile::tempdir().unwrap();
        // Two runs, so the first run's log is the previous log of the second.
        drop(RunLog::start_in(root.path()).unwrap());
        drop(RunLog::start_in(root.path()).unwrap());

        let active = state_root::log_path_in(root.path());
        assert_eq!(mode(active.parent().unwrap()), 0o700);
        assert_eq!(mode(&active), 0o600);
        assert_eq!(mode(&state_root::previous_log_path_in(root.path())), 0o600);
    }
}
