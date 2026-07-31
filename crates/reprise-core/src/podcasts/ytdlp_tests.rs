use std::{
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use super::test_support::{fake_binary, short_timeouts, CapturedLogs, LogCapture};
use super::{classify_stderr, finalize_download, resolve_binary, YtDlp, YtDlpFailureKind};
use crate::podcasts::config::YoutubeBrowser;

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
fn browser_session_discovery_requires_an_explicit_nonempty_override() {
    assert_eq!(super::discovery::resolve_browser_session(None), None);
    assert_eq!(
        super::discovery::resolve_browser_session(Some(OsStr::new(" \t"))),
        None
    );
    assert_eq!(
        super::discovery::resolve_browser_session(Some(OsStr::new(" brave "))),
        Some("brave".into())
    );
}

#[test]
fn discover_forwards_the_environment_browser_session_to_download() {
    const CHILD_OUTPUT: &str = "REPRISE_YTDLP_DISCOVERY_TEST_OUTPUT";
    if let Some(output) = std::env::var_os(CHILD_OUTPUT) {
        YtDlp::discover(None)
            .download("https://www.youtube.com/watch?v=v1", Path::new(&output))
            .unwrap();
        return;
    }

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
    let status = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "podcasts::ytdlp::tests::discover_forwards_the_environment_browser_session_to_download",
            "--nocapture",
        ])
        .env("REPRISE_YTDLP_BIN", binary)
        .env("REPRISE_YTDLP_COOKIES_FROM_BROWSER", "brave")
        .env(CHILD_OUTPUT, &output)
        .status()
        .unwrap();

    assert!(status.success(), "isolated discovery child failed");
    let args = fs::read_to_string(log).unwrap();
    assert!(
        args.lines()
            .collect::<Vec<_>>()
            .windows(2)
            .any(|pair| pair == ["--cookies-from-browser", "brave"]),
        "discover did not forward the environment browser session: {args}"
    );
}

#[test]
fn pod_22_discover_forwards_the_configured_browser_session_to_download() {
    const CHILD_OUTPUT: &str = "REPRISE_YTDLP_CONFIG_DISCOVERY_TEST_OUTPUT";
    if let Some(output) = std::env::var_os(CHILD_OUTPUT) {
        YtDlp::discover_with_browser(None, Some(YoutubeBrowser::Brave))
            .download("https://www.youtube.com/watch?v=v1", Path::new(&output))
            .unwrap();
        return;
    }

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
    let status = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "podcasts::ytdlp::tests::pod_22_discover_forwards_the_configured_browser_session_to_download",
            "--nocapture",
        ])
        .env("REPRISE_YTDLP_BIN", binary)
        .env_remove("REPRISE_YTDLP_COOKIES_FROM_BROWSER")
        .env(CHILD_OUTPUT, &output)
        .status()
        .unwrap();

    assert!(status.success(), "isolated discovery child failed");
    let args = fs::read_to_string(log).unwrap();
    assert!(
        args.lines()
            .collect::<Vec<_>>()
            .windows(2)
            .any(|pair| pair == ["--cookies-from-browser", "brave"]),
        "discover did not forward the configured browser session: {args}"
    );
}

#[test]
fn pod_22_explicit_browser_opt_out_ignores_the_environment_override() {
    const CHILD_OUTPUT: &str = "REPRISE_YTDLP_CONFIG_OPTOUT_TEST_OUTPUT";
    if let Some(output) = std::env::var_os(CHILD_OUTPUT) {
        YtDlp::discover_with_browser(None, None)
            .download("https://www.youtube.com/watch?v=v1", Path::new(&output))
            .unwrap();
        return;
    }

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
    let status = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "podcasts::ytdlp::tests::pod_22_explicit_browser_opt_out_ignores_the_environment_override",
            "--nocapture",
        ])
        .env("REPRISE_YTDLP_BIN", binary)
        .env("REPRISE_YTDLP_COOKIES_FROM_BROWSER", "brave")
        .env(CHILD_OUTPUT, &output)
        .status()
        .unwrap();

    assert!(status.success(), "isolated discovery child failed");
    let args = fs::read_to_string(log).unwrap();
    assert!(
        !args.contains("--cookies-from-browser"),
        "the explicit privacy opt-out was overridden: {args}"
    );
}

#[test]
fn pod_23_youtube_metadata_calls_prefer_the_requested_language() {
    let directory = tempfile::tempdir().unwrap();
    let log = directory.path().join("args");
    let binary = fake_binary(
        directory.path(),
        &format!(
            "printf '%s\\n' \"$@\" > '{}'\nprintf '%s\\n' '{{\"entries\":[]}}'",
            log.display()
        ),
    );
    let runner = YtDlp::with_binary_and_timeouts(binary, short_timeouts())
        .with_metadata_language(Some("de"));

    runner.list("https://youtube.test/@show").unwrap();
    assert_eq!(
        fs::read_to_string(&log)
            .unwrap()
            .lines()
            .collect::<Vec<_>>(),
        [
            "--no-warnings",
            "--flat-playlist",
            "--extractor-args",
            "youtubetab:approximate_date",
            "--extractor-args",
            "youtube:lang=de",
            "-J",
            "https://youtube.test/@show",
        ]
    );

    runner.search_channels("metal").unwrap();
    assert_eq!(
        fs::read_to_string(log).unwrap().lines().collect::<Vec<_>>(),
        [
            "--no-warnings",
            "--flat-playlist",
            "--extractor-args",
            "youtube:lang=de",
            "-J",
            "ytsearch20:metal",
        ]
    );

    let runner = runner.with_metadata_language(Some("zh_CN"));
    runner.list("https://youtube.test/@show").unwrap();
    assert_eq!(
        fs::read_to_string(directory.path().join("args"))
            .unwrap()
            .lines()
            .collect::<Vec<_>>(),
        [
            "--no-warnings",
            "--flat-playlist",
            "--extractor-args",
            "youtubetab:approximate_date",
            "--extractor-args",
            "youtube:lang=zh-CN",
            "-J",
            "https://youtube.test/@show",
        ]
    );
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
  "--no-warnings --flat-playlist --extractor-args youtubetab:approximate_date -J https://youtube.test/@show")
    printf '%s\n' '{"title":"Channel title","channel_url":"https://youtube.test/@show","thumbnail":"https://img.test/channel.jpg","entries":[{"id":"v1","title":"One","duration":12.8,"channel_id":"UC-stable","timestamp":1775001600,"thumbnail":"https://img.test/v1.jpg"},{"id":"","title":"Blank ID"},{"id":"v2","title":"Two","duration":null,"upload_date":"20260730"},{"id":"blank-title","title":"   "}]}'
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
    assert_eq!(playlist.entries[0].timestamp, Some(1_775_001_600));
    assert_eq!(
        playlist.entries[0].image_url.as_deref(),
        Some("https://img.test/v1.jpg")
    );
    assert_eq!(playlist.entries[1].duration_secs, None);
    assert_eq!(playlist.entries[1].upload_date.as_deref(), Some("20260730"));

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
fn pod_22_listing_search_and_resolve_pass_the_explicit_browser_session_to_ytdlp() {
    let directory = tempfile::tempdir().unwrap();
    let binary = fake_binary(
        directory.path(),
        r#"
case "$*" in
  "--cookies-from-browser brave --no-warnings --flat-playlist --extractor-args youtubetab:approximate_date -J https://youtube.test/@show")
    printf '%s\n' '{"title":"Channel","entries":[{"id":"v1","title":"One"}]}'
    ;;
  "--cookies-from-browser brave --no-warnings --flat-playlist --extractor-args youtubetab:approximate_date -I 1:40 -J https://youtube.test/@show")
    printf '%s\n' '{"title":"Channel","entries":[{"id":"v1","title":"One"}]}'
    ;;
  "--cookies-from-browser brave --no-warnings --flat-playlist -J ytsearch5:rust")
    printf '%s\n' '{"title":"Search","entries":[{"id":"v1","title":"One"}]}'
    ;;
  "--cookies-from-browser brave --no-warnings --flat-playlist -J ytsearch20:rust")
    printf '%s\n' '{"entries":[{"id":"v1","channel_id":"UC1","channel":"Channel"}]}'
    ;;
  "--cookies-from-browser brave --no-warnings -f bestaudio -j https://www.youtube.com/watch?v=v1")
    printf '%s\n' '{"url":"https://googlevideo.test/ephemeral","duration":93.4}'
    ;;
  *) printf '%s\n' "unexpected arguments: $*" >&2; exit 2 ;;
esac
"#,
    );
    let runner =
        YtDlp::with_binary_and_timeouts(binary, short_timeouts()).with_browser_session("brave");

    runner.list("https://youtube.test/@show").unwrap();
    runner.list_range("https://youtube.test/@show", 40).unwrap();
    runner.search("rust").unwrap();
    runner.search_channels("rust").unwrap();
    runner
        .resolve("https://www.youtube.com/watch?v=v1")
        .unwrap();
}

#[test]
fn pod_22_probe_and_update_do_not_read_the_browser_session() {
    let directory = tempfile::tempdir().unwrap();
    let binary = fake_binary(
        directory.path(),
        r#"
case "$*" in
  "--no-warnings --version") printf '%s\n' '2026.07.26' ;;
  "--no-warnings -U") printf '%s\n' 'yt-dlp is up to date' ;;
  *) printf '%s\n' "unexpected arguments: $*" >&2; exit 2 ;;
esac
"#,
    );
    let runner =
        YtDlp::with_binary_and_timeouts(binary, short_timeouts()).with_browser_session("brave");

    assert_eq!(runner.probe_version().unwrap(), "2026.07.26");
    assert_eq!(runner.update().unwrap(), "yt-dlp is up to date");
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
fn failed_resolve_keeps_sanitized_stderr_only_in_explicit_details() {
    let directory = tempfile::tempdir().unwrap();
    let binary = fake_binary(
        directory.path(),
        "printf '%s\\n' \
         'ERROR: Sign in to confirm you are not a bot for \
         https://youtube.test/watch?token=SECRET while reading \
         /home/user/private/cookies.txt with access_token=ALSO-SECRET' >&2\n\
         exit 9",
    );
    let runner = YtDlp::with_binary_and_timeouts(binary, short_timeouts());

    let error = runner
        .resolve("https://youtube.test/watch?v=private")
        .unwrap_err();
    let source_error = crate::source_error::SourceError::from(error);

    assert!(matches!(
        source_error.kind(),
        crate::source_error::SourceErrorKind::RateLimited { retry_after: None }
    ));
    assert_eq!(
        source_error.to_string(),
        "This source is limiting requests. Reprise will try again."
    );
    let details = source_error.details("2026-07-30 16:55").to_string();
    assert!(details.contains("Sign in to confirm you are not a bot"));
    assert!(!details.contains("youtube.test"), "{details}");
    assert!(!details.contains("SECRET"), "{details}");
    assert!(!details.contains("/home/user"), "{details}");
    assert!(!details.contains("cookies.txt"), "{details}");
    assert_ne!(
        details.lines().nth(1),
        Some("YouTube requires verification — try again later or use another network")
    );
}

#[test]
fn finalize_failure_keeps_a_private_path_out_of_explicit_details() {
    let directory = tempfile::tempdir().unwrap();
    let produced = directory
        .path()
        .join("private-library")
        .join("missing.opus");
    let destination = directory.path().join("episode.opus");

    let error = finalize_download(&produced.to_string_lossy(), &destination).unwrap_err();

    assert!(matches!(
        &error,
        crate::podcasts::PodcastError::YtDlpFailure {
            kind: YtDlpFailureKind::DownloadStorage,
            ..
        }
    ));
    let details = crate::source_error::SourceError::from(error)
        .details("2026-07-30 17:20")
        .to_string();
    assert!(details.contains("yt-dlp did not create"), "{details}");
    assert!(details.contains("[redacted path]"), "{details}");
    assert!(
        !details.contains(directory.path().to_string_lossy().as_ref()),
        "{details}"
    );
    assert!(!details.contains("private-library"), "{details}");
}
