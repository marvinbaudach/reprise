//! The log file of the running instance.
//!
//! `init_logging` installs the file layer before the application knows whether
//! it is the first instance, because the lines logged until then are worth
//! keeping. Until [`start`] or [`discard`] decides, the shared writer holds
//! them back; a second `reprise` launch only forwards its request to the first
//! and discards them, so it can never rotate the first one's live log away.
//! The file handling itself lives in `reprise_core::run_log`.

use std::io;
use std::sync::{Mutex, OnceLock, PoisonError};

use reprise_core::run_log::{DeferredRunLog, RunLog};

static RUN_LOG: OnceLock<Mutex<DeferredRunLog>> = OnceLock::new();

fn shared() -> &'static Mutex<DeferredRunLog> {
    RUN_LOG.get_or_init(|| Mutex::new(DeferredRunLog::default()))
}

/// One handle per formatted event; every handle writes to the one shared log.
pub(crate) struct FileLogWriter;

impl FileLogWriter {
    pub(crate) fn new() -> Self {
        Self
    }
}

impl io::Write for FileLogWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        shared()
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        shared()
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .flush()
    }
}

/// This process is the first instance: rotate the previous run aside and
/// begin the file with the lines held back so far. Without a writable state
/// directory the log stays on stderr alone.
pub(crate) fn start() {
    let started = RunLog::start();
    let mut deferred = shared().lock().unwrap_or_else(PoisonError::into_inner);
    match started {
        Some(log) => deferred.activate(log),
        None => deferred.discard(),
    }
}

/// This process is a second instance and writes no file.
pub(crate) fn discard() {
    shared()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .discard();
}
