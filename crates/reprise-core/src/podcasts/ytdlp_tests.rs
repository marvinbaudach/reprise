use std::{
    ffi::OsStr,
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use super::test_support::{fake_binary, short_timeouts, CapturedLogs, LogCapture};
use super::{classify_stderr, collect_output, resolve_binary, YtDlp, YtDlpTimeouts};

#[test]
fn pod_3_ytdlp_errors_are_actionable_and_never_expose_provider_details() {
    let cases = [
        (
            "ERROR: Sign in to confirm you’re not a bot",
            "YouTube requires verification — try again later or use another network",
        ),
        (
            "HTTP Error 429: Too Many Requests",
            "YouTube is rate-limiting requests — try again later",
        ),
        (
            "ERROR: Unsupported URL: https://example.test/watch?token=SECRET",
            "This YouTube URL is not supported",
        ),
        (
            "ERROR: Failed to resolve 'www.youtube.com'",
            "YouTube could not be reached — check your connection",
        ),
        (
            "ERROR: Unable to download webpage: HTTP Error 403: Forbidden",
            "YouTube refused the request — try again later",
        ),
        (
            "ERROR: Requested format is not available",
            "YouTube did not provide playable audio for this video",
        ),
        (
            "ERROR: Video unavailable. This video is private",
            "This YouTube video is unavailable or private",
        ),
        (
            "ERROR: Unable to download webpage: Video unavailable",
            "This YouTube video is unavailable or private",
        ),
        (
            "ERROR: Unable to extract initial player response",
            "YouTube changed its response — update yt-dlp and try again",
        ),
        (
            "ERROR: Postprocessing: ffmpeg not found",
            "Audio conversion is unavailable — install or repair FFmpeg",
        ),
        (
            "ERROR: [Errno 28] No space left on device",
            "YouTube download could not be saved — check available space and permissions",
        ),
        (
            "ERROR: extractor failed for https://example.test/watch?token=SECRET",
            "YouTube request failed — check the application log",
        ),
        (
            "ERROR:   \n",
            "YouTube request failed — check the application log",
        ),
        (" \n", "YouTube request failed — check the application log"),
    ];

    for (stderr, expected) in cases {
        assert_eq!(classify_stderr(stderr), expected);
    }
}

#[test]
fn binary_discovery_prefers_environment_then_setting_then_path() {
    assert_eq!(
        resolve_binary(Some(OsStr::new("/fixture/yt-dlp")), Some("/setting/yt-dlp")),
        PathBuf::from("/fixture/yt-dlp")
    );
    assert_eq!(
        resolve_binary(None, Some(" /setting/yt-dlp ")),
        PathBuf::from("/setting/yt-dlp")
    );
    assert_eq!(resolve_binary(None, Some(" ")), PathBuf::from("yt-dlp"));
}

#[test]
fn fake_binary_reports_version_and_projects_flat_playlist() {
    let directory = tempfile::tempdir().unwrap();
    let binary = fake_binary(
        directory.path(),
        r#"
case "$*" in
  "--no-warnings --version") printf '%s\n' '2026.07.26' ;;
  "--no-warnings --flat-playlist -J ytsearch5:rust audio")
    printf '%s\n' '{"title":"search","entries":[{"id":"s1","title":"Search hit","duration":30}]}'
    ;;
  "--no-warnings --flat-playlist -J https://youtube.test/@show")
    printf '%s\n' '{"title":"Channel title","channel_id":"UC-stable","channel_url":"https://youtube.test/@show","thumbnail":"https://img.test/channel.jpg","entries":[{"id":"v1","title":"One","duration":12.8},{"id":"","title":"Blank ID"},{"id":"v2","title":"Two","duration":null},{"id":"blank-title","title":"   "}]}'
    ;;
  *) printf '%s\n' "unexpected arguments: $*" >&2; exit 2 ;;
esac
"#,
    );
    let runner = YtDlp::with_binary_and_timeouts(binary, short_timeouts());

    assert_eq!(runner.probe_version().unwrap(), "2026.07.26");

    let playlist = runner.list("https://youtube.test/@show").unwrap();
    assert_eq!(playlist.title.as_deref(), Some("Channel title"));
    assert_eq!(
        playlist.source_url.as_deref(),
        Some("https://www.youtube.com/channel/UC-stable")
    );
    assert_eq!(
        playlist.image_url.as_deref(),
        Some("https://img.test/channel.jpg")
    );
    assert_eq!(playlist.entries.len(), 2);
    assert_eq!(playlist.entries[0].id, "v1");
    assert_eq!(playlist.entries[0].duration_secs, Some(12));
    assert_eq!(playlist.entries[1].duration_secs, None);

    let results = runner.search("rust audio").unwrap();
    assert_eq!(results.entries[0].id, "s1");
    assert_eq!(results.entries[0].title, "Search hit");
}

#[test]
fn resolve_returns_ephemeral_audio_url_and_duration() {
    let directory = tempfile::tempdir().unwrap();
    let binary = fake_binary(
        directory.path(),
        r#"
test "$*" = "--no-warnings -f bestaudio -j https://www.youtube.com/watch?v=v1"
printf '%s\n' '{"url":"https://googlevideo.test/ephemeral","duration":93.4}'
"#,
    );
    let runner = YtDlp::with_binary_and_timeouts(binary, short_timeouts());

    let resolved = runner
        .resolve("https://www.youtube.com/watch?v=v1")
        .unwrap();

    assert_eq!(resolved.stream_url, "https://googlevideo.test/ephemeral");
    assert_eq!(resolved.duration_secs, Some(93));
}

#[test]
fn malformed_resolve_response_is_actionable_and_logged_without_response_body() {
    let directory = tempfile::tempdir().unwrap();
    let binary = fake_binary(
        directory.path(),
        "printf '%s\\n' '{\"url\":\"https://cdn.test/audio?token=SECRET\"'",
    );
    let runner = YtDlp::with_binary_and_timeouts(binary, short_timeouts());
    let logs = CapturedLogs::default();
    let subscriber = LogCapture(logs.clone());

    let error = tracing::subscriber::with_default(subscriber, || {
        runner
            .resolve("https://youtube.test/watch?v=private")
            .unwrap_err()
    });

    assert_eq!(
        error.to_string(),
        "YouTube returned an unreadable response — update yt-dlp and try again"
    );
    let logged = logs.joined();
    for expected in [
        "message=yt-dlp response could not be parsed",
        "operation=\"resolve\"",
        "failure_kind=\"response_invalid\"",
    ] {
        assert!(
            logged.contains(expected),
            "yt-dlp response log omitted {expected:?}: {logged}"
        );
    }
    for secret in ["cdn.test", "token", "SECRET", "private"] {
        assert!(
            !logged.contains(secret),
            "yt-dlp response log leaked {secret:?}: {logged}"
        );
    }
}

#[test]
fn resolved_response_without_audio_is_actionable_and_logged() {
    let directory = tempfile::tempdir().unwrap();
    let binary = fake_binary(directory.path(), "printf '%s\\n' '{\"duration\":42}'");
    let runner = YtDlp::with_binary_and_timeouts(binary, short_timeouts());
    let logs = CapturedLogs::default();
    let subscriber = LogCapture(logs.clone());

    let error = tracing::subscriber::with_default(subscriber, || {
        runner
            .resolve("https://youtube.test/watch?v=private")
            .unwrap_err()
    });

    assert_eq!(
        error.to_string(),
        "YouTube did not provide playable audio for this video"
    );
    let logged = logs.joined();
    for expected in [
        "message=yt-dlp response omitted playable audio",
        "operation=\"resolve\"",
        "failure_kind=\"audio_unavailable\"",
    ] {
        assert!(
            logged.contains(expected),
            "yt-dlp audio response log omitted {expected:?}: {logged}"
        );
    }
    assert!(!logged.contains("private"), "{logged}");
}

#[test]
fn failed_resolve_logs_operation_category_and_exit_code_without_provider_details() {
    let directory = tempfile::tempdir().unwrap();
    let binary = fake_binary(
        directory.path(),
        "printf '%s\\n' \
         'ERROR: Sign in to confirm you are not a bot at \
         https://youtube.test/watch?token=SECRET while using /home/user/cookies.txt' >&2\n\
         exit 9",
    );
    let runner = YtDlp::with_binary_and_timeouts(binary, short_timeouts());
    let logs = CapturedLogs::default();
    let subscriber = LogCapture(logs.clone());

    let error = tracing::subscriber::with_default(subscriber, || {
        runner
            .resolve("https://youtube.test/watch?v=private")
            .unwrap_err()
    });

    assert_eq!(
        error.to_string(),
        "YouTube requires verification — try again later or use another network"
    );
    let logged = logs.joined();
    for expected in [
        "message=yt-dlp operation failed",
        "operation=\"resolve\"",
        "failure_kind=\"verification_required\"",
        "exit_code=9",
    ] {
        assert!(
            logged.contains(expected),
            "yt-dlp diagnostic log omitted {expected:?}: {logged}"
        );
    }
    for secret in [
        "youtube.test",
        "token",
        "SECRET",
        "/home/user",
        "cookies.txt",
        "private",
    ] {
        assert!(
            !logged.contains(secret),
            "yt-dlp diagnostic log leaked {secret:?}: {logged}"
        );
    }
}

#[test]
fn update_uses_the_same_guarded_subprocess_boundary() {
    let directory = tempfile::tempdir().unwrap();
    let log = directory.path().join("args");
    let binary = fake_binary(
        directory.path(),
        &format!(
            "printf '%s\\n' \"$@\" > '{}'\nprintf '%s\\n' 'Latest version'",
            log.display()
        ),
    );
    let runner = YtDlp::with_binary_and_timeouts(binary, short_timeouts());

    assert_eq!(runner.update().unwrap(), "Latest version");
    assert_eq!(fs::read_to_string(log).unwrap(), "--no-warnings\n-U\n");
}

#[test]
fn download_passes_audio_only_output_arguments() {
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("episode.audio");
    let postprocessed = directory.path().join("episode.opus");
    let log = directory.path().join("args");
    let binary = fake_binary(
        directory.path(),
        &format!(
            "printf '%s\\n' \"$@\" > '{}'\nprintf downloaded > '{}'\nprintf '%s\\n' '{}'",
            log.display(),
            postprocessed.display(),
            postprocessed.display()
        ),
    );
    let runner = YtDlp::with_binary_and_timeouts(binary, short_timeouts());

    runner
        .download("https://www.youtube.com/watch?v=v1", &output)
        .unwrap();

    let args = fs::read_to_string(log).unwrap();
    assert_eq!(
        args.lines().collect::<Vec<_>>(),
        vec![
            "--no-warnings",
            "--newline",
            "--progress-template",
            "download:reprise-progress:%(progress.downloaded_bytes)s\t%(progress.total_bytes)s\t%(progress.total_bytes_estimate)s",
            "-f",
            "bestaudio",
            "-x",
            "--audio-format",
            "opus",
            "--no-part",
            "--print",
            "after_move:filepath",
            "-o",
            output.to_str().unwrap(),
            "https://www.youtube.com/watch?v=v1",
        ]
    );
    assert_eq!(fs::read_to_string(&output).unwrap(), "downloaded");
    assert!(!postprocessed.exists());
}

/// `POD-13`: a failed YouTube download must not leave a `.part` file or
/// a yt-dlp postprocessor leftover behind — `output` here already
/// carries the `.part` suffix `downloads::partial_path` would give it,
/// and `leftover` stands in for an intermediate file (e.g. a
/// pre-conversion `.webm`) yt-dlp wrote before failing.
/// `ytdlp_download::cleanup_artifacts` sweeps every file in the parent
/// directory whose name starts with `output`'s — deleting that call
/// turns this red, since the leftover would then survive the failure.
#[test]
fn pod_13_a_failed_download_removes_part_and_postprocessor_leftovers() {
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("episode.opus.part");
    let leftover = directory.path().join("episode.opus.part.webm");
    let binary = fake_binary(
        directory.path(),
        &format!(
            "printf leftover > '{}'\nprintf '%s\\n' 'ERROR: unable to download video data' >&2\nexit 1",
            leftover.display()
        ),
    );
    let runner = YtDlp::with_binary_and_timeouts(binary, short_timeouts());

    let result = runner.download("https://www.youtube.com/watch?v=v1", &output);

    assert!(result.is_err());
    assert!(
        !leftover.exists(),
        "a yt-dlp postprocessor leftover must not survive a failed download"
    );
    assert!(!output.exists());
}

#[test]
fn failed_download_logs_operation_category_and_exit_code_without_provider_details() {
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("private-episode.audio");
    let binary = fake_binary(
        directory.path(),
        "printf '%s\\n' \
         'ERROR: HTTP Error 429 for https://youtube.test/watch?token=SECRET \
         while using /home/user/cookies.txt' >&2\n\
         exit 29",
    );
    let runner = YtDlp::with_binary_and_timeouts(binary, short_timeouts());
    let logs = CapturedLogs::default();
    let subscriber = LogCapture(logs.clone());

    let error = tracing::subscriber::with_default(subscriber, || {
        runner
            .download("https://youtube.test/watch?v=private", &output)
            .unwrap_err()
    });

    assert_eq!(
        error.to_string(),
        "YouTube is rate-limiting requests — try again later"
    );
    let logged = logs.joined();
    for expected in [
        "message=yt-dlp operation failed",
        "operation=\"download\"",
        "failure_kind=\"rate_limited\"",
        "exit_code=29",
    ] {
        assert!(
            logged.contains(expected),
            "yt-dlp download log omitted {expected:?}: {logged}"
        );
    }
    for secret in [
        "youtube.test",
        "token",
        "SECRET",
        "/home/user",
        "cookies.txt",
        "private",
    ] {
        assert!(
            !logged.contains(secret),
            "yt-dlp download log leaked {secret:?}: {logged}"
        );
    }
}

#[test]
fn unreported_download_file_is_actionable_and_logged_without_local_paths() {
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("private-episode.audio");
    let binary = fake_binary(
        directory.path(),
        "printf '%s\\n' '/home/user/secret-download.opus'",
    );
    let runner = YtDlp::with_binary_and_timeouts(binary, short_timeouts());
    let logs = CapturedLogs::default();
    let subscriber = LogCapture(logs.clone());

    let error = tracing::subscriber::with_default(subscriber, || {
        runner
            .download("https://youtube.test/watch?v=private", &output)
            .unwrap_err()
    });

    assert_eq!(
        error.to_string(),
        "YouTube download could not be saved — check available space and permissions"
    );
    let logged = logs.joined();
    for expected in [
        "message=yt-dlp download could not be finalized",
        "operation=\"download\"",
        "failure_kind=\"finalize_failed\"",
    ] {
        assert!(
            logged.contains(expected),
            "yt-dlp finalization log omitted {expected:?}: {logged}"
        );
    }
    for private in [
        "/home/user",
        "secret-download",
        "private-episode",
        "private",
    ] {
        assert!(
            !logged.contains(private),
            "yt-dlp finalization log leaked {private:?}: {logged}"
        );
    }
}

#[test]
fn pod_7_download_reports_machine_readable_progress_with_unknown_totals() {
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("episode.audio");
    let binary = fake_binary(
        directory.path(),
        &format!(
            "printf '%s\\n' 'reprise-progress:5\tNA\tNA'\n\
             printf '%s\\n' 'reprise-progress:8\t10\tNA'\n\
             printf downloaded > '{}'\n\
             printf '%s\\n' '{}'",
            output.display(),
            output.display()
        ),
    );
    let runner = YtDlp::with_binary_and_timeouts(binary, short_timeouts());
    let mut progress = Vec::new();

    runner
        .download_with_progress(
            "https://www.youtube.com/watch?v=v1",
            &output,
            &mut |event| progress.push(event),
        )
        .unwrap();

    assert_eq!(
        progress,
        [
            crate::podcasts::download_state::DownloadProgress {
                received_bytes: 5,
                total_bytes: None,
            },
            crate::podcasts::download_state::DownloadProgress {
                received_bytes: 8,
                total_bytes: Some(10),
            },
        ]
    );
}

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
