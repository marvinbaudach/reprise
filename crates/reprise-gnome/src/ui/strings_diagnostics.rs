//! Copy for the crash offer and the Copy Diagnostics action.

macro_rules! N_ {
    ($message:literal) => {
        $message
    };
}

/// The menu entry and the toast button that copy the diagnostics report.
pub const COPY_DIAGNOSTICS: &str = N_!("Copy Diagnostics");
pub const DIAGNOSTICS_COPIED: &str = N_!("Diagnostics copied to the clipboard");
pub const DIAGNOSTICS_COPY_FAILED: &str = N_!("Diagnostics could not be copied");
/// START-6: shown once when the previous run did not end cleanly.
pub const PREVIOUS_RUN_ENDED_UNEXPECTEDLY: &str = N_!("Reprise did not close properly last time");
