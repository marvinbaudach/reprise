//! yt-dlp subprocess boundary.

use std::{
    ffi::{OsStr, OsString},
    io::Read,
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    sync::mpsc::{self, Receiver, RecvTimeoutError},
    thread,
    time::{Duration, Instant},
};

use serde_json::Value;

#[path = "ytdlp_range.rs"]
mod range;

#[path = "ytdlp_failure.rs"]
mod failure;

pub use failure::YtDlpFailureKind;
use failure::*;

pub use super::ytdlp_search::YtDlpChannel;
use super::PodcastError;

const POLL_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct YtDlpTimeouts {
    pub version: Duration,
    pub update: Duration,
    pub list: Duration,
    pub search: Duration,
    pub resolve: Duration,
    pub download: Duration,
}

impl Default for YtDlpTimeouts {
    fn default() -> Self {
        Self {
            version: Duration::from_secs(10),
            update: Duration::from_secs(60),
            list: Duration::from_secs(60),
            search: Duration::from_secs(60),
            resolve: Duration::from_secs(45),
            download: Duration::from_secs(600),
        }
    }
}

#[derive(Clone, Debug)]
pub struct YtDlp {
    binary: PathBuf,
    timeouts: YtDlpTimeouts,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct YtDlpVideo {
    pub id: String,
    pub title: String,
    pub duration_secs: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct YtDlpPlaylist {
    pub title: Option<String>,
    /// Stable channel URL when yt-dlp reports a channel identity.
    pub source_url: Option<String>,
    pub image_url: Option<String>,
    pub entries: Vec<YtDlpVideo>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedAudio {
    pub stream_url: String,
    pub duration_secs: Option<i64>,
}

impl YtDlp {
    /// Discovers the executable without probing it.
    pub fn discover(setting_path: Option<&str>) -> Self {
        Self::with_binary(resolve_binary(
            std::env::var_os("REPRISE_YTDLP_BIN").as_deref(),
            setting_path,
        ))
    }

    pub fn with_binary(binary: impl Into<PathBuf>) -> Self {
        Self::with_binary_and_timeouts(binary, YtDlpTimeouts::default())
    }

    /// Constructs the boundary with operation-specific deadlines.
    ///
    /// The custom deadlines keep fixture-based tests fast and let callers use
    /// stricter policy without changing subprocess behavior.
    pub fn with_binary_and_timeouts(binary: impl Into<PathBuf>, timeouts: YtDlpTimeouts) -> Self {
        Self {
            binary: binary.into(),
            timeouts,
        }
    }

    pub fn probe_version(&self) -> Result<String, PodcastError> {
        let output = self.run(
            "probe_version",
            ["--no-warnings", "--version"],
            self.timeouts.version,
        )?;
        output
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .map(str::to_string)
            .ok_or_else(|| response_error("probe_version"))
    }

    pub fn update(&self) -> Result<String, PodcastError> {
        let output = self.run("update", ["--no-warnings", "-U"], self.timeouts.update)?;
        Ok(output.trim().to_owned())
    }

    pub fn list(&self, url: &str) -> Result<YtDlpPlaylist, PodcastError> {
        let output = self.run(
            "list",
            ["--no-warnings", "--flat-playlist", "-J", url],
            self.timeouts.list,
        )?;
        parse_playlist("list", &output)
    }

    pub fn search(&self, terms: &str) -> Result<YtDlpPlaylist, PodcastError> {
        let target = format!("ytsearch5:{terms}");
        let output = self.run(
            "search",
            [
                OsString::from("--no-warnings"),
                OsString::from("--flat-playlist"),
                OsString::from("-J"),
                OsString::from(target),
            ],
            self.timeouts.search,
        )?;
        parse_playlist("search", &output)
    }

    pub fn search_channels(&self, terms: &str) -> Result<Vec<YtDlpChannel>, PodcastError> {
        let target = format!("ytsearch20:{terms}");
        let output = self.run(
            "search_channels",
            [
                OsString::from("--no-warnings"),
                OsString::from("--flat-playlist"),
                OsString::from("-J"),
                OsString::from(target),
            ],
            self.timeouts.search,
        )?;
        super::ytdlp_search::parse_search_channels(&output)
            .map_err(|_| response_error("search_channels"))
    }

    pub fn resolve(&self, video_url: &str) -> Result<ResolvedAudio, PodcastError> {
        let output = self.run(
            "resolve",
            ["--no-warnings", "-f", "bestaudio", "-j", video_url],
            self.timeouts.resolve,
        )?;
        parse_resolved_audio("resolve", &output)
    }

    pub fn download(&self, video_url: &str, output: &Path) -> Result<(), PodcastError> {
        self.download_with_progress(video_url, output, &mut |_| {})
    }

    pub fn download_with_progress(
        &self,
        video_url: &str,
        output: &Path,
        on_progress: &mut dyn FnMut(super::download_state::DownloadProgress),
    ) -> Result<(), PodcastError> {
        super::ytdlp_download::download(
            &self.binary,
            self.timeouts.download,
            video_url,
            output,
            on_progress,
        )
    }

    fn run<I, S>(
        &self,
        operation: &'static str,
        arguments: I,
        timeout: Duration,
    ) -> Result<String, PodcastError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let mut command = Command::new(&self.binary);
        command
            .args(arguments)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        configure_process_group(&mut command);
        let mut child = command
            .spawn()
            .map_err(|error| logged_spawn_error(operation, &error))?;

        let process_group = child.id();
        let stdout = read_in_background(child.stdout.take().expect("piped stdout"));
        let stderr = read_in_background(child.stderr.take().expect("piped stderr"));
        let deadline = Instant::now() + timeout;

        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    terminate_process_group(process_group);
                    break status;
                }
                Ok(None) => {}
                Err(error) => {
                    terminate_process_tree(&mut child);
                    return Err(runtime_error("monitor_failed", operation, &error));
                }
            }
            let now = Instant::now();
            if now >= deadline {
                terminate_process_tree(&mut child);
                log_timeout(operation, timeout);
                return Err(PodcastError::YtDlpTimeout);
            }
            thread::sleep(POLL_INTERVAL.min(deadline.saturating_duration_since(now)));
        };

        let stdout =
            collect_output(operation, "stdout", &stdout, deadline, timeout).inspect_err(|_| {
                terminate_process_group(process_group);
            })?;
        let stderr =
            collect_output(operation, "stderr", &stderr, deadline, timeout).inspect_err(|_| {
                terminate_process_group(process_group);
            })?;
        output_from_status(operation, status, &stdout, &stderr)
    }
}

pub fn resolve_binary(environment_override: Option<&OsStr>, setting_path: Option<&str>) -> PathBuf {
    environment_override
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            setting_path
                .map(str::trim)
                .filter(|path| !path.is_empty())
                .map(PathBuf::from)
        })
        .unwrap_or_else(|| PathBuf::from("yt-dlp"))
}

/// Maps yt-dlp's unstable diagnostic text to a short message suitable for UI use.
pub fn classify_stderr(stderr: &str) -> String {
    classify_failure(stderr).user_message().to_string()
}

fn classify_failure(stderr: &str) -> YtDlpFailureKind {
    let lowercase = stderr.to_ascii_lowercase();
    if lowercase.contains("429") || lowercase.contains("too many requests") {
        return YtDlpFailureKind::RateLimited;
    }
    if lowercase.contains("sign in to confirm") || lowercase.contains("not a bot") {
        return YtDlpFailureKind::VerificationRequired;
    }
    if lowercase.contains("unsupported url") {
        return YtDlpFailureKind::UnsupportedUrl;
    }
    if lowercase.contains("http error 401") || lowercase.contains("http error 403") {
        return YtDlpFailureKind::AccessRefused;
    }
    if lowercase.contains("video unavailable")
        || lowercase.contains("video is private")
        || lowercase.contains("private video")
        || lowercase.contains("members-only")
        || lowercase.contains("members only")
    {
        return YtDlpFailureKind::VideoUnavailable;
    }
    if lowercase.contains("unable to extract")
        || lowercase.contains("signature extraction failed")
        || lowercase.contains("nsig extraction failed")
    {
        return YtDlpFailureKind::ExtractorOutdated;
    }
    if lowercase.contains("ffmpeg not found")
        || lowercase.contains("ffprobe not found")
        || lowercase.contains("ffmpeg-location")
    {
        return YtDlpFailureKind::ConversionUnavailable;
    }
    if lowercase.contains("no space left on device")
        || lowercase.contains("disk quota exceeded")
        || lowercase.contains("read-only file system")
    {
        return YtDlpFailureKind::DownloadStorage;
    }
    if lowercase.contains("requested format is not available")
        || lowercase.contains("no video formats found")
    {
        return YtDlpFailureKind::AudioUnavailable;
    }
    if lowercase.contains("failed to resolve")
        || lowercase.contains("name or service not known")
        || lowercase.contains("unable to download webpage")
        || lowercase.contains("connection refused")
        || lowercase.contains("network is unreachable")
    {
        return YtDlpFailureKind::Unreachable;
    }
    YtDlpFailureKind::Other
}

pub(super) fn read_in_background(
    mut stream: impl Read + Send + 'static,
) -> Receiver<std::io::Result<Vec<u8>>> {
    let (sender, receiver) = mpsc::sync_channel(1);
    thread::spawn(move || {
        let mut bytes = Vec::new();
        let result = stream.read_to_end(&mut bytes).map(|_| bytes);
        let _ = sender.send(result);
    });
    receiver
}

pub(super) fn collect_output(
    operation: &'static str,
    stream: &'static str,
    reader: &Receiver<std::io::Result<Vec<u8>>>,
    deadline: Instant,
    timeout: Duration,
) -> Result<Vec<u8>, PodcastError> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    match reader.recv_timeout(remaining) {
        Ok(Ok(output)) => Ok(output),
        Ok(Err(error)) => Err(output_read_error(operation, stream, &error)),
        Err(RecvTimeoutError::Timeout) => {
            log_output_timeout(operation, stream, timeout);
            Err(PodcastError::YtDlpTimeout)
        }
        Err(RecvTimeoutError::Disconnected) => {
            tracing::warn!(
                operation,
                failure_kind = "output_reader_disconnected",
                stream,
                "yt-dlp output reader disconnected"
            );
            Err(diagnostic_error(YtDlpFailureKind::Other, GENERIC_FAILURE))
        }
    }
}

#[cfg(unix)]
pub(super) fn configure_process_group(command: &mut Command) {
    use std::os::unix::process::CommandExt;

    command.process_group(0);
}

#[cfg(not(unix))]
pub(super) fn configure_process_group(_command: &mut Command) {}

pub(super) fn terminate_process_tree(child: &mut Child) {
    terminate_process_group(child.id());
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(unix)]
pub(super) fn terminate_process_group(process_group: u32) {
    const SIGKILL: i32 = 9;

    unsafe extern "C" {
        #[link_name = "kill"]
        fn kill_process(process_id: i32, signal: i32) -> i32;
    }

    let Ok(process_group) = i32::try_from(process_group) else {
        return;
    };
    // SAFETY: a negative PID addresses the dedicated child process group
    // created above. SIGKILL has no borrowed-memory or lifetime contract.
    let _ = unsafe { kill_process(-process_group, SIGKILL) };
}

#[cfg(not(unix))]
pub(super) fn terminate_process_group(_process_group: u32) {}

pub(super) fn map_spawn_error(error: &std::io::Error) -> PodcastError {
    if error.kind() == std::io::ErrorKind::NotFound {
        diagnostic_error(YtDlpFailureKind::HelperMissing, MISSING_MESSAGE)
    } else {
        diagnostic_error(YtDlpFailureKind::HelperStartFailed, START_FAILED_MESSAGE)
    }
}

pub(super) fn logged_spawn_error(operation: &'static str, error: &std::io::Error) -> PodcastError {
    let failure_kind = if error.kind() == std::io::ErrorKind::NotFound {
        "component_missing"
    } else {
        "start_failed"
    };
    tracing::warn!(
        operation,
        failure_kind,
        error_kind = ?error.kind(),
        "could not start yt-dlp operation"
    );
    map_spawn_error(error)
}

pub(super) fn log_timeout(operation: &'static str, timeout: Duration) {
    tracing::warn!(
        operation,
        failure_kind = "timeout",
        timeout_ms = u64::try_from(timeout.as_millis()).unwrap_or(u64::MAX),
        "yt-dlp operation timed out"
    );
}

pub(super) fn log_output_timeout(operation: &'static str, stream: &'static str, timeout: Duration) {
    tracing::warn!(
        operation,
        failure_kind = "output_read_timeout",
        stream,
        timeout_ms = u64::try_from(timeout.as_millis()).unwrap_or(u64::MAX),
        "timed out while reading yt-dlp output"
    );
}

pub(super) fn output_read_error(
    operation: &'static str,
    stream: &'static str,
    error: &std::io::Error,
) -> PodcastError {
    tracing::warn!(
        operation,
        failure_kind = "output_read_failed",
        stream,
        error_kind = ?error.kind(),
        "could not read yt-dlp output"
    );
    diagnostic_error(YtDlpFailureKind::Other, GENERIC_FAILURE)
}

pub(super) fn runtime_error(
    failure_kind: &'static str,
    operation: &'static str,
    error: &std::io::Error,
) -> PodcastError {
    tracing::warn!(
        operation,
        failure_kind,
        error_kind = ?error.kind(),
        "yt-dlp operation could not be completed"
    );
    diagnostic_error(YtDlpFailureKind::Other, GENERIC_FAILURE)
}

fn response_error(operation: &'static str) -> PodcastError {
    tracing::warn!(
        operation,
        failure_kind = "response_invalid",
        "yt-dlp response could not be parsed"
    );
    diagnostic_error(
        YtDlpFailureKind::ResponseUnreadable,
        INVALID_RESPONSE_MESSAGE,
    )
}

fn audio_unavailable_error(operation: &'static str) -> PodcastError {
    tracing::warn!(
        operation,
        failure_kind = "audio_unavailable",
        "yt-dlp response omitted playable audio"
    );
    diagnostic_error(
        YtDlpFailureKind::AudioUnavailable,
        AUDIO_UNAVAILABLE_MESSAGE,
    )
}

pub(super) fn download_finalize_error() -> PodcastError {
    tracing::warn!(
        operation = "download",
        failure_kind = "finalize_failed",
        "yt-dlp download could not be finalized"
    );
    diagnostic_error(YtDlpFailureKind::DownloadStorage, DOWNLOAD_SAVE_MESSAGE)
}

pub(super) fn error_from_status(
    operation: &'static str,
    status: ExitStatus,
    stderr: &[u8],
) -> PodcastError {
    let stderr = String::from_utf8_lossy(stderr);
    let failure = classify_failure(&stderr);
    tracing::warn!(
        operation,
        failure_kind = failure.diagnostic_name(),
        exit_code = status.code().unwrap_or(-1),
        "yt-dlp operation failed"
    );
    diagnostic_error(failure, &stderr)
}

fn diagnostic_error(kind: YtDlpFailureKind, diagnostic: &str) -> PodcastError {
    PodcastError::YtDlpFailure {
        kind,
        stderr: sanitize_diagnostic(diagnostic),
    }
}

fn sanitize_diagnostic(stderr: &str) -> String {
    stderr
        .split_whitespace()
        .map(|token| {
            let normalized = token.to_ascii_lowercase();
            if normalized.contains("http://")
                || normalized.contains("https://")
                || normalized.contains("file://")
            {
                "[redacted URL]"
            } else if is_private_path(token) {
                "[redacted path]"
            } else if contains_secret(&normalized) {
                "[redacted secret]"
            } else {
                token
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn is_private_path(token: &str) -> bool {
    let token = token.trim_start_matches(['\'', '"', '(', '[', '{']);
    token.starts_with('/')
        || (token.len() >= 3
            && token.as_bytes()[0].is_ascii_alphabetic()
            && token.as_bytes()[1] == b':'
            && matches!(token.as_bytes()[2], b'/' | b'\\'))
}

fn contains_secret(normalized: &str) -> bool {
    const SECRET_KEYS: [&str; 8] = [
        "token",
        "signature",
        "sig",
        "credential",
        "authorization",
        "auth",
        "cookie",
        "key",
    ];
    let normalized = normalized.trim_start_matches(['\'', '"', '(', '[', '{', '?', '&', '-', ':']);
    SECRET_KEYS.iter().any(|key| {
        [format!("{key}="), format!("{key}:")]
            .iter()
            .any(|marker| normalized.contains(marker))
    }) || normalized.contains("akia")
}

fn output_from_status(
    operation: &'static str,
    status: ExitStatus,
    stdout: &[u8],
    stderr: &[u8],
) -> Result<String, PodcastError> {
    if status.success() {
        Ok(String::from_utf8_lossy(stdout).into_owned())
    } else {
        Err(error_from_status(operation, status, stderr))
    }
}

pub(super) fn finalize_download(stdout: &str, destination: &Path) -> Result<(), PodcastError> {
    let produced = stdout
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| {
            diagnostic_error(
                YtDlpFailureKind::DownloadStorage,
                "yt-dlp did not report the downloaded file",
            )
        })?;
    let produced_is_regular_file = produced
        .symlink_metadata()
        .is_ok_and(|metadata| metadata.file_type().is_file());
    if !produced_is_regular_file {
        return Err(diagnostic_error(
            YtDlpFailureKind::DownloadStorage,
            &format!("yt-dlp did not create {}", produced.display()),
        ));
    }
    if produced == destination {
        return Ok(());
    }
    if destination.exists() {
        return Err(diagnostic_error(
            YtDlpFailureKind::DownloadStorage,
            &format!(
                "download destination already exists: {}",
                destination.display()
            ),
        ));
    }

    let produced_parent = canonical_parent(&produced)?;
    let destination_parent = canonical_parent(destination)?;
    if produced_parent != destination_parent {
        return Err(diagnostic_error(
            YtDlpFailureKind::DownloadStorage,
            "yt-dlp reported a file outside the download destination",
        ));
    }
    std::fs::rename(&produced, destination).map_err(|error| {
        diagnostic_error(
            YtDlpFailureKind::DownloadStorage,
            &format!(
                "could not finalize podcast download at {}: {error}",
                destination.display()
            ),
        )
    })
}

fn canonical_parent(path: &Path) -> Result<PathBuf, PodcastError> {
    path.parent()
        .unwrap_or_else(|| Path::new("."))
        .canonicalize()
        .map_err(|error| {
            diagnostic_error(
                YtDlpFailureKind::DownloadStorage,
                &format!("could not resolve podcast download directory: {error}"),
            )
        })
}

fn parse_playlist(operation: &'static str, body: &str) -> Result<YtDlpPlaylist, PodcastError> {
    let value: Value = serde_json::from_str(body).map_err(|_| response_error(operation))?;
    let entries = value
        .get("entries")
        .and_then(Value::as_array)
        .ok_or_else(|| response_error(operation))?;
    let entries = entries
        .iter()
        .filter_map(|entry| {
            let id = entry.get("id")?.as_str()?.trim().to_string();
            let title = entry.get("title")?.as_str()?.trim().to_string();
            if id.is_empty() || title.is_empty() {
                return None;
            }
            Some(YtDlpVideo {
                id,
                title,
                duration_secs: duration_secs(entry.get("duration")),
            })
        })
        .collect();

    Ok(YtDlpPlaylist {
        title: value
            .get("title")
            .and_then(Value::as_str)
            .map(str::to_string),
        source_url: super::ytdlp_search::stable_source_url(&value),
        image_url: super::ytdlp_search::entry_image_url(&value),
        entries,
    })
}

fn parse_resolved_audio(
    operation: &'static str,
    body: &str,
) -> Result<ResolvedAudio, PodcastError> {
    let value: Value = serde_json::from_str(body).map_err(|_| response_error(operation))?;
    let stream_url = value
        .get("url")
        .and_then(Value::as_str)
        .filter(|url| !url.is_empty())
        .ok_or_else(|| audio_unavailable_error(operation))?;
    Ok(ResolvedAudio {
        stream_url: stream_url.to_string(),
        duration_secs: duration_secs(value.get("duration")),
    })
}

fn duration_secs(value: Option<&Value>) -> Option<i64> {
    value
        .and_then(|value| {
            value
                .as_f64()
                .or_else(|| value.as_str()?.parse::<f64>().ok())
        })
        .filter(|duration| duration.is_finite() && *duration >= 0.0)
        .map(|duration| duration as i64)
}

#[cfg(all(test, unix))]
#[path = "ytdlp_range_tests.rs"]
mod range_tests;

#[cfg(all(test, unix))]
#[path = "ytdlp_test_support.rs"]
mod test_support;

#[cfg(all(test, unix))]
#[path = "ytdlp_tests.rs"]
mod tests;

#[cfg(all(test, unix))]
#[path = "ytdlp_failure_tests.rs"]
mod failure_tests;
