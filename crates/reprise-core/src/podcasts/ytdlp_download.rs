//! Machine-readable yt-dlp download progress boundary.

use std::{
    ffi::OsString,
    io::{BufRead, BufReader},
    path::Path,
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
            OsString::from("--no-part"),
            OsString::from("--print"),
            OsString::from("after_move:filepath"),
            OsString::from("-o"),
            output.as_os_str().to_os_string(),
            OsString::from(video_url),
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    super::ytdlp::configure_process_group(&mut command);
    let mut child = command
        .spawn()
        .map_err(|error| super::ytdlp::map_spawn_error(&error))?;
    let process_group = child.id();
    let stdout = read_lines(child.stdout.take().expect("piped stdout"));
    let stderr = super::ytdlp::read_in_background(child.stderr.take().expect("piped stderr"));
    let deadline = Instant::now() + timeout;
    let mut output_lines = Vec::new();
    let mut received_bytes = 0;
    let mut total_bytes = None;

    let status = loop {
        drain_available(
            &stdout,
            &mut output_lines,
            &mut received_bytes,
            &mut total_bytes,
            on_progress,
        );
        match child.try_wait() {
            Ok(Some(status)) => {
                super::ytdlp::terminate_process_group(process_group);
                break status;
            }
            Ok(None) => {}
            Err(error) => {
                super::ytdlp::terminate_process_tree(&mut child);
                cleanup_artifacts(output);
                return Err(PodcastError::YtDlp(format!(
                    "could not monitor yt-dlp: {error}"
                )));
            }
        }
        let now = Instant::now();
        if now >= deadline {
            super::ytdlp::terminate_process_tree(&mut child);
            cleanup_artifacts(output);
            return Err(PodcastError::Timeout);
        }
        thread::sleep(POLL_INTERVAL.min(deadline.saturating_duration_since(now)));
    };

    if let Err(error) = collect_remaining(
        &stdout,
        deadline,
        &mut output_lines,
        &mut received_bytes,
        &mut total_bytes,
        on_progress,
    ) {
        cleanup_artifacts(output);
        return Err(error);
    }
    let stderr = match super::ytdlp::collect_output(&stderr, deadline) {
        Ok(stderr) => stderr,
        Err(error) => {
            cleanup_artifacts(output);
            return Err(error);
        }
    };
    if !status.success() {
        cleanup_artifacts(output);
        return Err(PodcastError::YtDlp(super::ytdlp::classify_stderr(
            &String::from_utf8_lossy(&stderr),
        )));
    }
    match super::ytdlp::finalize_download(&output_lines.join("\n"), output) {
        Ok(()) => Ok(()),
        Err(error) => {
            cleanup_artifacts(output);
            Err(error)
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
) {
    while let Ok(line) = receiver.try_recv() {
        apply_line(line, output, received_bytes, total_bytes, on_progress);
    }
}

fn collect_remaining(
    receiver: &Receiver<std::io::Result<String>>,
    deadline: Instant,
    output: &mut Vec<String>,
    received_bytes: &mut u64,
    total_bytes: &mut Option<u64>,
    on_progress: &mut dyn FnMut(DownloadProgress),
) -> Result<(), PodcastError> {
    loop {
        match receiver.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            Ok(line) => apply_line(line, output, received_bytes, total_bytes, on_progress),
            Err(RecvTimeoutError::Disconnected) => return Ok(()),
            Err(RecvTimeoutError::Timeout) => return Err(PodcastError::Timeout),
        }
    }
}

fn apply_line(
    line: std::io::Result<String>,
    output: &mut Vec<String>,
    received_bytes: &mut u64,
    total_bytes: &mut Option<u64>,
    on_progress: &mut dyn FnMut(DownloadProgress),
) {
    let Ok(line) = line else {
        return;
    };
    let Some(progress) = parse_progress(&line) else {
        output.push(line);
        return;
    };
    *received_bytes = (*received_bytes).max(progress.received_bytes);
    *total_bytes = progress.total_bytes.or(*total_bytes);
    on_progress(DownloadProgress {
        received_bytes: *received_bytes,
        total_bytes: *total_bytes,
    });
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
