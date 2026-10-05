//! Shared tracing capture for yt-dlp boundary tests.

use std::{
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

use super::YtDlpTimeouts;

/// Writes `contents` to `path` and marks it executable, through a child process.
///
/// A write descriptor held by the test process can be inherited by a sibling
/// test's `fork` and keep the file busy (`ETXTBSY`) when that test executes it.
/// The writer therefore lives in a short-lived `sh` that is waited on, and the
/// text travels as an argument rather than through a pipe, so no test process
/// ever holds a write descriptor on an executable it is about to run.
pub(in crate::podcasts) fn write_executable(path: &Path, contents: &str) {
    let status = Command::new("sh")
        .args(["-c", r#"printf '%s\n' "$1" > "$2" && chmod 755 "$2""#, "_"])
        .arg(contents)
        .arg(path)
        .status()
        .expect("start the fixture writer");
    assert!(status.success(), "the fixture writer failed: {status}");
}

pub(super) fn fake_binary(directory: &Path, body: &str) -> PathBuf {
    let path = directory.join("fake-yt-dlp");
    write_executable(&path, &format!("#!/bin/sh\nset -eu\n{body}"));
    path
}

pub(super) fn short_timeouts() -> YtDlpTimeouts {
    YtDlpTimeouts {
        version: Duration::from_secs(2),
        update: Duration::from_secs(2),
        list: Duration::from_secs(2),
        search: Duration::from_secs(2),
        channel_head: Duration::from_secs(2),
        resolve: Duration::from_secs(2),
        download: Duration::from_secs(2),
    }
}

pub(super) use crate::log_capture::CapturedLogs;
