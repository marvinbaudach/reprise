//! The single-owner lease (§9.3).
//!
//! Exactly one process may own the runtime. The lease that says so is an
//! exclusive advisory lock on a file under `XDG_RUNTIME_DIR` — not a
//! database row and not a bus-name detail — for three reasons the plan is
//! explicit about:
//!
//! * It is claimed **before** GStreamer, devices or the writer are opened,
//!   so a process that loses has never touched an effect.
//! * The kernel releases it when the process ends, including on `SIGKILL`.
//!   There is no stale lock and nothing to reap.
//! * The file's *contents* are diagnostics only. The lock is the authority;
//!   a reader that trusts the recorded pid over the lock is reading a
//!   snapshot of a moment that may already be gone.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use reprise_runtime_protocol::PROTOCOL_VERSION;
use rustix::fs::{flock, FlockOperation};

/// The lock file's name inside the runtime directory.
const LEASE_FILE: &str = "runtime.lock";
/// The per-user subdirectory Reprise keeps its runtime state in.
const LEASE_DIR: &str = "reprise";

/// Why a lease could not be claimed.
#[derive(Debug)]
pub enum LeaseError {
    /// Another process owns the runtime. The only permitted response is to
    /// exit with a structured cause; a second runtime is a bug, not a case
    /// to handle.
    Held,
    /// `XDG_RUNTIME_DIR` is unset. There is deliberately no fallback: the
    /// directory's guarantees — per-user, per-session, cleaned on logout —
    /// are what make the lease mean what it says, and `/tmp` has none of
    /// them.
    NoRuntimeDir,
    /// The lock file could not be created or locked for an unrelated reason.
    Io(std::io::Error),
}

impl std::fmt::Display for LeaseError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Held => formatter.write_str("another Reprise runtime already holds the lease"),
            Self::NoRuntimeDir => formatter.write_str("XDG_RUNTIME_DIR is not set"),
            Self::Io(error) => write!(formatter, "lease file unusable: {error}"),
        }
    }
}

impl std::error::Error for LeaseError {}

/// Where the lease lives inside a given runtime directory.
///
/// Split out from [`RuntimeLease::claim`] so the path rule is testable
/// without touching the process environment, which no test can mutate
/// safely while its siblings run.
#[must_use]
pub fn lease_path(runtime_dir: &Path) -> PathBuf {
    runtime_dir.join(LEASE_DIR).join(LEASE_FILE)
}

/// A held lease. Dropping it releases the lock.
#[derive(Debug)]
pub struct RuntimeLease {
    path: PathBuf,
    // Held for its side effect: closing this descriptor is what releases the
    // lock, so the field must outlive every effect the runtime performs.
    _file: File,
}

impl RuntimeLease {
    /// Claims the lease at the session's runtime directory.
    pub fn claim() -> Result<Self, LeaseError> {
        let runtime_dir = std::env::var_os("XDG_RUNTIME_DIR").ok_or(LeaseError::NoRuntimeDir)?;
        Self::claim_at(&lease_path(Path::new(&runtime_dir)))
    }

    /// Claims the lease at an explicit path.
    pub fn claim_at(path: &Path) -> Result<Self, LeaseError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(LeaseError::Io)?;
        }
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
            .map_err(LeaseError::Io)?;

        // Non-blocking: a loser must find out immediately and exit, not wait
        // for a runtime it is not going to become.
        match flock(&file, FlockOperation::NonBlockingLockExclusive) {
            Ok(()) => {}
            // `EAGAIN` and `EWOULDBLOCK` are the same value on Linux, so one
            // arm covers both spellings the manual page uses.
            Err(rustix::io::Errno::WOULDBLOCK) => return Err(LeaseError::Held),
            Err(error) => return Err(LeaseError::Io(error.into())),
        }

        // Written only after winning, so a loser can never truncate the
        // owner's diagnostics.
        file.set_len(0).map_err(LeaseError::Io)?;
        writeln!(
            file,
            "pid={}\nprotocol={PROTOCOL_VERSION}",
            std::process::id()
        )
        .map_err(LeaseError::Io)?;
        file.flush().map_err(LeaseError::Io)?;

        Ok(Self {
            path: path.to_path_buf(),
            _file: file,
        })
    }

    /// The file the lease is held on. Diagnostics; never an identity.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

// No `Drop`. The kernel releases the lock when the descriptor closes, which
// is also what happens on a crash, so an explicit release would only add a
// path that behaves differently from the one that matters. The file itself
// stays behind on purpose: unlinking it would let a process that already
// opened it lock an inode nobody else can see, which is precisely the
// two-owners situation the lease exists to prevent.

#[cfg(test)]
#[path = "lease_tests.rs"]
mod lease_tests;
