use std::{
    ffi::OsStr,
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use super::{classify_stderr, resolve_binary, YtDlp, YtDlpTimeouts};
use crate::podcasts::PodcastError;

fn fake_binary(directory: &Path, body: &str) -> PathBuf {
    let path = directory.join("fake-yt-dlp");
    fs::write(&path, format!("#!/bin/sh\nset -eu\n{body}\n")).unwrap();
    let mut permissions = fs::metadata(&path).unwrap().permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(&path, permissions).unwrap();
    path
}

fn short_timeouts() -> YtDlpTimeouts {
    YtDlpTimeouts {
        version: Duration::from_millis(300),
        update: Duration::from_millis(300),
        list: Duration::from_millis(300),
        search: Duration::from_millis(300),
        resolve: Duration::from_millis(300),
        download: Duration::from_millis(300),
    }
}

#[test]
fn pod_3_ytdlp_errors_are_readable_never_panic() {
    let cases = [
        (
            "ERROR: Sign in to confirm you’re not a bot",
            "YouTube blocked the request — update yt-dlp (Preferences)",
        ),
        (
            "HTTP Error 429: Too Many Requests",
            "YouTube blocked the request — update yt-dlp (Preferences)",
        ),
        (
            "ERROR: Unsupported URL: https://example.test/watch",
            "Unsupported URL: https://example.test/watch",
        ),
        ("ERROR:   \n", "yt-dlp failed"),
        (" \n", "yt-dlp failed"),
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
        "YouTube blocked the request — update yt-dlp (Preferences)"
    );
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

    assert!(matches!(runner.probe_version(), Err(PodcastError::Timeout)));
    assert!(started.elapsed() < Duration::from_secs(2));
    assert_process_exits(read_pid(&descendant_pid));
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
