//! Machine-readable yt-dlp download progress boundary.

use std::{
    ffi::OsString,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::mpsc::{self, Receiver, RecvTimeoutError},
    thread,
    time::{Duration, Instant},
};

use super::download_state::DownloadProgress;
use super::PodcastError;

const PREFIX: &str = "reprise-progress:";
const POLL_INTERVAL: Duration = Duration::from_millis(50);
const PROGRESS_TEMPLATE: &str =
    "download:reprise-progress:%(progress.downloaded_bytes)s\t%(progress.total_bytes)s\t%(progress.total_bytes_estimate)s";

pub(super) fn download(
    binary: &Path,
    timeout: Duration,
    video_url: &str,
    output: &Path,
    on_progress: &mut dyn FnMut(DownloadProgress),
) -> Result<(), PodcastError> {
    // `output` is `downloads::partial_path`'s temporary name, so it ends in
    // `.part` — a suffix yt-dlp reserves for its own partial downloads and
    // strips from the output template. It then writes the media under the
    // shortened name and its post-processor fails to find it, ending the run
    // in `exit 1`. Give yt-dlp a name it does not reserve; it reports what it
    // actually produced through `--print after_move:filepath`, and
    // `finalize_download` moves that onto `output` as before. The name stays
    // prefixed with `output`, so `cleanup_artifacts` still sweeps it.
    let requested_output = reserved_free_output(output);
    let mut command = Command::new(binary);
    command
        .args([
            OsString::from("--no-warnings"),
            OsString::from("--newline"),
            OsString::from("--progress-template"),
            OsString::from(PROGRESS_TEMPLATE),
            OsString::from("-f"),
            OsString::from("bestaudio"),
            OsString::from("-x"),
            OsString::from("--audio-format"),
            OsString::from("opus"),
            OsString::from("--no-part"),
            OsString::from("--print"),
            OsString::from("after_move:filepath"),
            OsString::from("-o"),
            requested_output.as_os_str().to_os_string(),
            OsString::from(video_url),
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    super::ytdlp::configure_process_group(&mut command);
    let mut child = command
        .spawn()
        .map_err(|error| super::ytdlp::logged_spawn_error("download", &error))?;
    let process_group = child.id();
    let stdout = read_lines(child.stdout.take().expect("piped stdout"));
    let stderr = super::ytdlp::read_in_background(child.stderr.take().expect("piped stderr"));
    let deadline = Instant::now() + timeout;
    let mut output_lines = Vec::new();
    let mut received_bytes = 0;
    let mut total_bytes = None;

    let status = loop {
        if let Err(error) = drain_available(
            &stdout,
            &mut output_lines,
            &mut received_bytes,
            &mut total_bytes,
            on_progress,
        ) {
            super::ytdlp::terminate_process_tree(&mut child);
            cleanup_artifacts(output);
            return Err(error);
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                super::ytdlp::terminate_process_group(process_group);
                break status;
            }
            Ok(None) => {}
            Err(error) => {
                super::ytdlp::terminate_process_tree(&mut child);
                cleanup_artifacts(output);
                return Err(super::ytdlp::runtime_error(
                    "monitor_failed",
                    "download",
                    &error,
                ));
            }
        }
        let now = Instant::now();
        if now >= deadline {
            super::ytdlp::terminate_process_tree(&mut child);
            cleanup_artifacts(output);
            super::ytdlp::log_timeout("download", timeout);
            return Err(PodcastError::YtDlpTimeout);
        }
        thread::sleep(POLL_INTERVAL.min(deadline.saturating_duration_since(now)));
    };

    if let Err(error) = collect_remaining(
        &stdout,
        deadline,
        timeout,
        &mut output_lines,
        &mut received_bytes,
        &mut total_bytes,
        on_progress,
    ) {
        cleanup_artifacts(output);
        return Err(error);
    }
    let stderr =
        match super::ytdlp::collect_output("download", "stderr", &stderr, deadline, timeout) {
            Ok(stderr) => stderr,
            Err(error) => {
                cleanup_artifacts(output);
                return Err(error);
            }
        };
    if !status.success() {
        cleanup_artifacts(output);
        return Err(super::ytdlp::error_from_status("download", status, &stderr));
    }
    match super::ytdlp::finalize_download(&output_lines.join("\n"), output) {
        Ok(()) => Ok(()),
        Err(_) => {
            cleanup_artifacts(output);
            Err(super::ytdlp::download_finalize_error())
        }
    }
}

fn read_lines(stream: impl std::io::Read + Send + 'static) -> Receiver<std::io::Result<String>> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        for line in BufReader::new(stream).lines() {
            let failed = line.is_err();
            if sender.send(line).is_err() || failed {
                break;
            }
        }
    });
    receiver
}

fn drain_available(
    receiver: &Receiver<std::io::Result<String>>,
    output: &mut Vec<String>,
    received_bytes: &mut u64,
    total_bytes: &mut Option<u64>,
    on_progress: &mut dyn FnMut(DownloadProgress),
) -> Result<(), PodcastError> {
    while let Ok(line) = receiver.try_recv() {
        apply_line(line, output, received_bytes, total_bytes, on_progress)?;
    }
    Ok(())
}

fn collect_remaining(
    receiver: &Receiver<std::io::Result<String>>,
    deadline: Instant,
    timeout: Duration,
    output: &mut Vec<String>,
    received_bytes: &mut u64,
    total_bytes: &mut Option<u64>,
    on_progress: &mut dyn FnMut(DownloadProgress),
) -> Result<(), PodcastError> {
    loop {
        match receiver.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            Ok(line) => apply_line(line, output, received_bytes, total_bytes, on_progress)?,
            Err(RecvTimeoutError::Disconnected) => return Ok(()),
            Err(RecvTimeoutError::Timeout) => {
                super::ytdlp::log_output_timeout("download", "stdout", timeout);
                return Err(PodcastError::YtDlpTimeout);
            }
        }
    }
}

fn apply_line(
    line: std::io::Result<String>,
    output: &mut Vec<String>,
    received_bytes: &mut u64,
    total_bytes: &mut Option<u64>,
    on_progress: &mut dyn FnMut(DownloadProgress),
) -> Result<(), PodcastError> {
    let line = match line {
        Ok(line) => line,
        Err(error) => {
            return Err(super::ytdlp::output_read_error(
                "download", "stdout", &error,
            ));
        }
    };
    let Some(progress) = parse_progress(&line) else {
        output.push(line);
        return Ok(());
    };
    *received_bytes = (*received_bytes).max(progress.received_bytes);
    *total_bytes = progress.total_bytes.or(*total_bytes);
    on_progress(DownloadProgress {
        received_bytes: *received_bytes,
        total_bytes: *total_bytes,
    });
    Ok(())
}

fn parse_progress(line: &str) -> Option<DownloadProgress> {
    let mut values = line.strip_prefix(PREFIX)?.split('\t');
    let received_bytes = parse_bytes(values.next()?)?;
    let total_bytes = parse_bytes(values.next().unwrap_or_default())
        .or_else(|| parse_bytes(values.next().unwrap_or_default()));
    Some(DownloadProgress {
        received_bytes,
        total_bytes,
    })
}

fn parse_bytes(value: &str) -> Option<u64> {
    value
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite() && *value >= 0.0)
        .map(|value| value as u64)
}

/// An output template yt-dlp will honour literally.
///
/// A trailing `.part` is yt-dlp's own marker for an unfinished download and is
/// removed from the output name, which breaks the post-processing step that
/// follows. Appending rather than replacing keeps the result a strict
/// extension of `output`, which is what [`cleanup_artifacts`] matches on.
fn reserved_free_output(output: &Path) -> PathBuf {
    let mut name = output.as_os_str().to_os_string();
    name.push(".download");
    PathBuf::from(name)
}

fn cleanup_artifacts(output: &Path) {
    let Some(parent) = output.parent() else {
        return;
    };
    let Some(prefix) = output.file_name().and_then(|name| name.to_str()) else {
        return;
    };
    let Ok(entries) = std::fs::read_dir(parent) else {
        return;
    };
    for entry in entries.flatten() {
        if entry
            .file_name()
            .to_str()
            .is_some_and(|name| name.starts_with(prefix))
        {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}
