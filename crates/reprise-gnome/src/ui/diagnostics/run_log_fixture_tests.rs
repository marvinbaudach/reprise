//! A real production call site for the diagnostics tests.
//!
//! The template table only knows production source files, and test code is
//! none of them, so a test that needs an own event provokes one: the run log
//! logs `could not move the previous log aside` when a directory sits where
//! the previous log belongs. The error names the temporary directory, which
//! carries the needle `zqx`, so a report that leaked it would show.
//!
//! The directory is made here, in a file the frontend thinness budget does not
//! count, because the test fixture is not frontend code.

/// Makes the run log fail to move the previous log aside, once.
pub(super) fn move_the_previous_log_aside_and_fail() {
    let root = tempfile::Builder::new()
        .prefix("zqx-state-")
        .tempdir()
        .unwrap();
    let previous = reprise_core::state_root::previous_log_path_in(root.path());
    std::fs::create_dir_all(&previous).unwrap();
    let _ = reprise_core::run_log::RunLog::start_in(root.path());
}
