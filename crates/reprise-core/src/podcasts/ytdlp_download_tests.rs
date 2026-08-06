//! Download-path behaviour of the yt-dlp boundary.

use std::fs;

use super::test_support::{fake_binary, short_timeouts, CapturedLogs};
use super::YtDlp;

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

    let metadata = runner
        .download("https://www.youtube.com/watch?v=v1", &output)
        .unwrap();

    let args = fs::read_to_string(log).unwrap();
    assert_eq!(
        args.lines().collect::<Vec<_>>(),
        vec![
            "--no-warnings",
            "--newline",
            // `--print` below implies `--quiet`, and a quiet yt-dlp emits no
            // progress at all — the UI then sat at `0 B` until the finished
            // size appeared. `--progress` prints it even when quiet.
            "--progress",
            "--progress-template",
            "download:reprise-progress:%(progress.downloaded_bytes)s\t%(progress.total_bytes)s\t%(progress.total_bytes_estimate)s",
            "-f",
            "bestaudio",
            "-x",
            "--audio-format",
            "opus",
            "--no-part",
            // Each print carries its own marker so the reader finds the field
            // by name, not by position — stdout also carries postprocess
            // lines in yt-dlp's own format.
            "--print",
            "after_move:reprise-file:%(filepath)s",
            "--print",
            "after_move:reprise-categories:%(categories)j",
            "-o",
            // Not `output` itself: a trailing `.part` is yt-dlp's own
            // marker and gets stripped, so the executor asks for a name it
            // honours literally and moves the result onto `output`
            // afterwards. See
            // `a_youtube_download_is_not_handed_an_output_path_yt_dlp_reserves`.
            &format!("{}.download", output.display()),
            "https://www.youtube.com/watch?v=v1",
        ]
    );
    assert_eq!(fs::read_to_string(&output).unwrap(), "downloaded");
    assert!(!postprocessed.exists());
    assert!(
        metadata.categories.is_empty(),
        "a missing metadata line must leave the category unknown"
    );
}

#[test]
fn download_returns_categories_without_mistaking_them_for_the_filepath() {
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("episode.audio");
    let postprocessed = directory.path().join("episode.opus");
    let binary = fake_binary(
        directory.path(),
        &format!(
            "printf downloaded > '{}'\nprintf '%s\\n' 'reprise-file:{}' 'reprise-categories:[\"Music\"]'",
            postprocessed.display(),
            postprocessed.display()
        ),
    );
    let runner = YtDlp::with_binary_and_timeouts(binary, short_timeouts());

    let metadata = runner
        .download("https://www.youtube.com/watch?v=v1", &output)
        .unwrap();

    assert_eq!(metadata.categories, ["Music"]);
    assert_eq!(fs::read_to_string(&output).unwrap(), "downloaded");
}

/// stdout is shared. `--progress` is on and only the `download:` phase carries
/// our template, so a postprocess line arrives in yt-dlp's own format — and
/// `-x --audio-format opus` means a postprocessor really does run. Reading the
/// first line as the path handed `finalize_download` that foreign line and
/// failed a download that had produced its file; reading the last two would
/// still be a guess. Each field is found by its own marker instead.
#[test]
fn download_finds_its_fields_by_marker_even_with_foreign_lines_on_stdout() {
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("episode.audio");
    let postprocessed = directory.path().join("episode.opus");
    let binary = fake_binary(
        directory.path(),
        &format!(
            "printf downloaded > '{}'\nprintf '%s\\n' \
             '[ExtractAudio] Destination: {}' \
             'reprise-file:{}' \
             '[Something] trailing chatter' \
             'reprise-categories:[\"News & Politics\"]'",
            postprocessed.display(),
            postprocessed.display(),
            postprocessed.display()
        ),
    );
    let runner = YtDlp::with_binary_and_timeouts(binary, short_timeouts());

    let metadata = runner
        .download("https://www.youtube.com/watch?v=v1", &output)
        .unwrap();

    assert_eq!(metadata.categories, ["News & Politics"]);
    assert_eq!(fs::read_to_string(&output).unwrap(), "downloaded");
}

#[test]
fn pod_22_download_passes_the_explicit_browser_session_to_ytdlp() {
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
    let runner =
        YtDlp::with_binary_and_timeouts(binary, short_timeouts()).with_browser_session("brave");

    runner
        .download("https://www.youtube.com/watch?v=v1", &output)
        .unwrap();

    let args = fs::read_to_string(log).unwrap();
    assert!(
        args.lines()
            .collect::<Vec<_>>()
            .windows(2)
            .any(|pair| pair == ["--cookies-from-browser", "brave"]),
        "the explicit browser session was not passed to yt-dlp: {args}"
    );
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

    let error = logs.capture(|| {
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

    let error = logs.capture(|| {
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
