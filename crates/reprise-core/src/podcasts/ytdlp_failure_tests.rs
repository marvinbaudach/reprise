//! The failure, timeout and subprocess-teardown half of the yt-dlp boundary tests.
//! Split out of `ytdlp_tests.rs` so both files stay under the 800-line limit the
//! architecture gate enforces; `ytdlp_tests.rs` keeps the happy paths.

use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use super::test_support::{fake_binary, short_timeouts, CapturedLogs, LogCapture};
use super::{collect_output, YtDlp, YtDlpTimeouts};

#[test]
fn missing_binary_and_failed_process_are_readable() {
    let missing =
        YtDlp::with_binary_and_timeouts("/definitely/missing/reprise-yt-dlp", short_timeouts());
    assert_eq!(
        missing.probe_version().unwrap_err().to_string(),
        "YouTube component is unavailable — reinstall or repair Reprise"
    );

    let directory = tempfile::tempdir().unwrap();
    let binary = fake_binary(
        directory.path(),
        "printf '%s\\n' 'ERROR: Sign in to confirm you are not a bot' >&2\nexit 1",
    );
    let failed = YtDlp::with_binary_and_timeouts(binary, short_timeouts());
    assert_eq!(
        failed
            .list("https://youtube.test/@show")
            .unwrap_err()
            .to_string(),
        "YouTube requires verification — try again later or use another network"
    );
}

#[test]
fn missing_component_log_names_the_operation_without_exposing_its_path() {
    let runner =
        YtDlp::with_binary_and_timeouts("/missing/private/reprise-yt-dlp", short_timeouts());
    let logs = CapturedLogs::default();
    let subscriber = LogCapture(logs.clone());

    let error =
        tracing::subscriber::with_default(subscriber, || runner.probe_version().unwrap_err());

    assert_eq!(
        error.to_string(),
        "YouTube component is unavailable — reinstall or repair Reprise"
    );
    let logged = logs.joined();
    for expected in [
        "message=could not start yt-dlp operation",
        "operation=\"probe_version\"",
        "failure_kind=\"component_missing\"",
        "error_kind=NotFound",
    ] {
        assert!(
            logged.contains(expected),
            "yt-dlp start log omitted {expected:?}: {logged}"
        );
    }
    for private in ["/missing", "/private", "reprise-yt-dlp"] {
        assert!(
            !logged.contains(private),
            "yt-dlp start log leaked {private:?}: {logged}"
        );
    }
}

#[test]
fn unexecutable_component_is_actionable_and_logged_without_its_path() {
    let directory = tempfile::tempdir().unwrap();
    let binary = directory.path().join("private-yt-dlp");
    fs::write(&binary, "#!/bin/sh\n").unwrap();
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o600)).unwrap();
    let runner = YtDlp::with_binary_and_timeouts(&binary, short_timeouts());
    let logs = CapturedLogs::default();
    let subscriber = LogCapture(logs.clone());

    let error =
        tracing::subscriber::with_default(subscriber, || runner.probe_version().unwrap_err());

    assert_eq!(
        error.to_string(),
        "YouTube component could not start — check its path and permissions"
    );
    let logged = logs.joined();
    for expected in [
        "message=could not start yt-dlp operation",
        "operation=\"probe_version\"",
        "failure_kind=\"start_failed\"",
        "error_kind=PermissionDenied",
    ] {
        assert!(
            logged.contains(expected),
            "yt-dlp start log omitted {expected:?}: {logged}"
        );
    }
    for private in ["private-yt-dlp", directory.path().to_str().unwrap()] {
        assert!(
            !logged.contains(private),
            "yt-dlp start log leaked {private:?}: {logged}"
        );
    }
}

#[test]
fn output_reader_failure_is_actionable_and_logged_without_os_error_text() {
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    sender
        .send(Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "could not read /home/user/private-output",
        )))
        .unwrap();
    let logs = CapturedLogs::default();
    let subscriber = LogCapture(logs.clone());

    let error = tracing::subscriber::with_default(subscriber, || {
        collect_output(
            "resolve",
            "stdout",
            &receiver,
            Instant::now() + Duration::from_millis(100),
            Duration::from_millis(100),
        )
        .unwrap_err()
    });

    assert_eq!(
        error.to_string(),
        "YouTube request failed — check the application log"
    );
    let logged = logs.joined();
    for expected in [
        "message=could not read yt-dlp output",
        "operation=\"resolve\"",
        "failure_kind=\"output_read_failed\"",
        "stream=\"stdout\"",
        "error_kind=PermissionDenied",
    ] {
        assert!(
            logged.contains(expected),
            "yt-dlp output log omitted {expected:?}: {logged}"
        );
    }
    for private in ["/home/user", "private-output"] {
        assert!(
            !logged.contains(private),
            "yt-dlp output log leaked {private:?}: {logged}"
        );
    }
}

#[test]
fn output_reader_timeout_logs_stream_and_configured_deadline() {
    let (_sender, receiver) = std::sync::mpsc::sync_channel(1);
    let logs = CapturedLogs::default();
    let subscriber = LogCapture(logs.clone());

    let error = tracing::subscriber::with_default(subscriber, || {
        collect_output(
            "download",
            "stdout",
            &receiver,
            Instant::now(),
            Duration::from_millis(77),
        )
        .unwrap_err()
    });

    assert_eq!(error.to_string(), "YouTube request timed out — try again");
    let logged = logs.joined();
    for expected in [
        "message=timed out while reading yt-dlp output",
        "operation=\"download\"",
        "failure_kind=\"output_read_timeout\"",
        "stream=\"stdout\"",
        "timeout_ms=77",
    ] {
        assert!(
            logged.contains(expected),
            "yt-dlp output timeout log omitted {expected:?}: {logged}"
        );
    }
}

#[test]
fn hanging_process_is_killed_at_the_operation_deadline() {
    let directory = tempfile::tempdir().unwrap();
    let descendant_pid = directory.path().join("descendant-pid");
    let binary = fake_binary(
        directory.path(),
        &format!(
            "sleep 30 &\nprintf '%s' \"$!\" > '{}'\nwhile :; do :; done",
            descendant_pid.display()
        ),
    );
    let runner = YtDlp::with_binary_and_timeouts(
        binary,
        YtDlpTimeouts {
            version: Duration::from_millis(80),
            ..short_timeouts()
        },
    );
    let started = Instant::now();

    assert_eq!(
        runner.probe_version().unwrap_err().to_string(),
        "YouTube request timed out — try again"
    );
    assert!(started.elapsed() < Duration::from_secs(2));
    assert_process_exits(read_pid(&descendant_pid));
}

#[test]
fn timed_out_probe_logs_operation_and_deadline() {
    let directory = tempfile::tempdir().unwrap();
    let binary = fake_binary(directory.path(), "while :; do :; done");
    let runner = YtDlp::with_binary_and_timeouts(
        binary,
        YtDlpTimeouts {
            version: Duration::from_millis(80),
            ..short_timeouts()
        },
    );
    let logs = CapturedLogs::default();
    let subscriber = LogCapture(logs.clone());

    let error =
        tracing::subscriber::with_default(subscriber, || runner.probe_version().unwrap_err());

    let logged = logs.joined();
    assert_eq!(
        error.to_string(),
        "YouTube request timed out — try again",
        "{logged}"
    );
    for expected in [
        "message=yt-dlp operation timed out",
        "operation=\"probe_version\"",
        "failure_kind=\"timeout\"",
        "timeout_ms=80",
    ] {
        assert!(
            logged.contains(expected),
            "yt-dlp timeout log omitted {expected:?}: {logged}"
        );
    }
}

#[test]
fn successful_parent_exit_does_not_hang_on_descendant_owned_pipes() {
    let directory = tempfile::tempdir().unwrap();
    let descendant_pid = directory.path().join("descendant-pid");
    let binary = fake_binary(
        directory.path(),
        &format!(
            "sleep 30 &\nprintf '%s' \"$!\" > '{}'\nprintf '%s\\n' '2026.07.26'",
            descendant_pid.display()
        ),
    );
    let runner = YtDlp::with_binary_and_timeouts(binary, short_timeouts());
    let started = Instant::now();

    assert_eq!(runner.probe_version().unwrap(), "2026.07.26");
    assert!(started.elapsed() < Duration::from_secs(2));
    assert_process_exits(read_pid(&descendant_pid));
}

fn read_pid(path: &Path) -> u32 {
    fs::read_to_string(path).unwrap().parse().unwrap()
}

#[cfg(target_os = "linux")]
fn assert_process_exits(pid: u32) {
    let process = PathBuf::from(format!("/proc/{pid}"));
    for _ in 0..50 {
        if !process.exists() {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("descendant process {pid} survived yt-dlp cleanup");
}

#[cfg(not(target_os = "linux"))]
fn assert_process_exits(_pid: u32) {}

/// yt-dlp reserves a trailing `.part` for its own partial downloads: it
/// strips that suffix from the output template, writes the media under the
/// shortened name, and its post-processor then fails to find what it just
/// wrote — the run ends in `exit 1` with "Postprocessing: WARNING: unable to
/// obtain file audio codec with ffprobe" and no episode.
///
/// `downloads::partial_path` appends exactly that suffix, because the atomic
/// publish contract needs a temporary name, and the value was handed straight
/// to `-o`. For RSS that is harmless — Reprise writes the file itself — but
/// every YouTube download failed. Verified against the real binary
/// (2026.07.04): the identical command differs only in `-o`, and succeeds
/// with `episode.opus` where it exits 1 with `episode.part`.
///
/// The fake below reproduces that contract rather than the symptom: it
/// refuses any `-o` ending in `.part` and otherwise behaves like a successful
/// run. Handing the reserved path back turns this red.
#[test]
fn a_youtube_download_is_not_handed_an_output_path_yt_dlp_reserves() {
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("episode.opus.part");
    let binary = fake_binary(
        directory.path(),
        "target=''\n\
         while [ $# -gt 0 ]; do\n\
           if [ \"$1\" = '-o' ]; then target=$2; fi\n\
           shift\n\
         done\n\
         case \"$target\" in\n\
           *.part)\n\
             printf '%s\\n' 'ERROR: Postprocessing: WARNING: unable to obtain file audio codec with ffprobe' >&2\n\
             exit 1\n\
             ;;\n\
         esac\n\
         printf downloaded > \"$target\"\n\
         printf '%s\\n' \"$target\"",
    );
    let runner = YtDlp::with_binary_and_timeouts(binary, short_timeouts());

    let result = runner.download("https://www.youtube.com/watch?v=v1", &output);

    assert!(
        result.is_ok(),
        "yt-dlp must not be given a path whose suffix it reserves, got {result:?}"
    );
    assert_eq!(
        fs::read_to_string(&output).unwrap(),
        "downloaded",
        "the finished download must still be published at the temporary path \
         `download_atomically` waits for"
    );
}
