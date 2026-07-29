use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::{YtDlp, YtDlpTimeouts};

fn fake_binary(directory: &Path, body: &str) -> PathBuf {
    let path = directory.join("fake-yt-dlp-range");
    fs::write(&path, format!("#!/bin/sh\nset -eu\n{body}\n")).unwrap();
    let mut permissions = fs::metadata(&path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&path, permissions).unwrap();
    path
}

fn short_timeouts() -> YtDlpTimeouts {
    let short = Duration::from_secs(2);
    YtDlpTimeouts {
        version: short,
        update: short,
        list: short,
        search: short,
        resolve: short,
        download: short,
    }
}

#[test]
fn pod_10_extended_listing_requests_the_first_forty_provider_items() {
    let directory = tempfile::tempdir().unwrap();
    let log = directory.path().join("args");
    let binary = fake_binary(
        directory.path(),
        &format!(
            "printf '%s\\n' \"$@\" > '{}'\nprintf '%s\\n' '{{\"title\":\"Channel\",\"entries\":[]}}'",
            log.display()
        ),
    );
    let runner = YtDlp::with_binary_and_timeouts(binary, short_timeouts());

    runner
        .list_range("https://www.youtube.com/channel/UCmore", 40)
        .unwrap();

    assert_eq!(
        fs::read_to_string(log).unwrap().lines().collect::<Vec<_>>(),
        [
            "--no-warnings",
            "--flat-playlist",
            "-I",
            "1:40",
            "-J",
            "https://www.youtube.com/channel/UCmore",
        ]
    );
}
