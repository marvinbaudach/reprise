//! Shared tracing capture for yt-dlp boundary tests.

use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    time::Duration,
};

use super::YtDlpTimeouts;

pub(super) fn fake_binary(directory: &Path, body: &str) -> PathBuf {
    let path = directory.join("fake-yt-dlp");
    fs::write(&path, format!("#!/bin/sh\nset -eu\n{body}\n")).unwrap();
    let mut permissions = fs::metadata(&path).unwrap().permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(&path, permissions).unwrap();
    path
}

pub(super) fn short_timeouts() -> YtDlpTimeouts {
    YtDlpTimeouts {
        version: Duration::from_secs(2),
        update: Duration::from_secs(2),
        list: Duration::from_secs(2),
        search: Duration::from_secs(2),
        resolve: Duration::from_secs(2),
        download: Duration::from_secs(2),
    }
}

pub(super) use crate::log_capture::{CapturedLogs, LogCapture};
