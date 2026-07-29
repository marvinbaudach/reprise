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

pub use super::ytdlp_search::YtDlpChannel;
use super::PodcastError;

const BLOCKED_MESSAGE: &str = "YouTube blocked the request — update yt-dlp (Preferences)";
const MISSING_MESSAGE: &str = "YouTube component is unavailable — reinstall or repair Reprise";
const GENERIC_FAILURE: &str = "yt-dlp failed";
const MAX_ERROR_CHARS: usize = 180;
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
        let output = self.run(["--no-warnings", "--version"], self.timeouts.version)?;
        output
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .map(str::to_string)
            .ok_or_else(|| PodcastError::Parse("yt-dlp returned no version".to_string()))
    }

    pub fn update(&self) -> Result<String, PodcastError> {
        let output = self.run(["--no-warnings", "-U"], self.timeouts.update)?;
        Ok(output.trim().to_owned())
    }

    pub fn list(&self, url: &str) -> Result<YtDlpPlaylist, PodcastError> {
        let output = self.run(
            ["--no-warnings", "--flat-playlist", "-J", url],
            self.timeouts.list,
        )?;
        parse_playlist(&output)
    }

    pub fn search(&self, terms: &str) -> Result<YtDlpPlaylist, PodcastError> {
        let target = format!("ytsearch5:{terms}");
        let output = self.run(
            [
                OsString::from("--no-warnings"),
                OsString::from("--flat-playlist"),
                OsString::from("-J"),
                OsString::from(target),
            ],
            self.timeouts.search,
        )?;
        parse_playlist(&output)
    }

    pub fn search_channels(&self, terms: &str) -> Result<Vec<YtDlpChannel>, PodcastError> {
        let target = format!("ytsearch20:{terms}");
        let output = self.run(
            [
                OsString::from("--no-warnings"),
                OsString::from("--flat-playlist"),
                OsString::from("-J"),
                OsString::from(target),
            ],
            self.timeouts.search,
        )?;
        super::ytdlp_search::parse_search_channels(&output)
    }

    pub fn resolve(&self, video_url: &str) -> Result<ResolvedAudio, PodcastError> {
        let output = self.run(
            ["--no-warnings", "-f", "bestaudio", "-j", video_url],
            self.timeouts.resolve,
        )?;
        parse_resolved_audio(&output)
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

    fn run<I, S>(&self, arguments: I, timeout: Duration) -> Result<String, PodcastError>
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
        let mut child = command.spawn().map_err(|error| map_spawn_error(&error))?;

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
                    return Err(PodcastError::YtDlp(format!(
                        "could not monitor yt-dlp: {error}"
                    )));
                }
            }
            let now = Instant::now();
            if now >= deadline {
                terminate_process_tree(&mut child);
                return Err(PodcastError::Timeout);
            }
            thread::sleep(POLL_INTERVAL.min(deadline.saturating_duration_since(now)));
        };

        let stdout = collect_output(&stdout, deadline).inspect_err(|_| {
            terminate_process_group(process_group);
        })?;
        let stderr = collect_output(&stderr, deadline).inspect_err(|_| {
            terminate_process_group(process_group);
        })?;
        output_from_status(status, &stdout, &stderr)
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
    let lowercase = stderr.to_ascii_lowercase();
    if lowercase.contains("sign in to confirm")
        || lowercase.contains("not a bot")
        || lowercase.contains("429")
    {
        return BLOCKED_MESSAGE.to_string();
    }

    let Some(line) = stderr.lines().map(str::trim).find(|line| !line.is_empty()) else {
        return GENERIC_FAILURE.to_string();
    };
    let line = line.strip_prefix("ERROR:").map_or(line, str::trim);
    if line.is_empty() {
        return GENERIC_FAILURE.to_string();
    }
    line.chars().take(MAX_ERROR_CHARS).collect()
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
    reader: &Receiver<std::io::Result<Vec<u8>>>,
    deadline: Instant,
) -> Result<Vec<u8>, PodcastError> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    match reader.recv_timeout(remaining) {
        Ok(result) => result.map_err(|error| PodcastError::YtDlp(error.to_string())),
        Err(RecvTimeoutError::Timeout) => Err(PodcastError::Timeout),
        Err(RecvTimeoutError::Disconnected) => Err(PodcastError::YtDlp(
            "yt-dlp output reader failed".to_string(),
        )),
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
        PodcastError::YtDlp(MISSING_MESSAGE.to_string())
    } else {
        PodcastError::YtDlp(format!("could not start yt-dlp: {error}"))
    }
}

fn output_from_status(
    status: ExitStatus,
    stdout: &[u8],
    stderr: &[u8],
) -> Result<String, PodcastError> {
    if status.success() {
        Ok(String::from_utf8_lossy(stdout).into_owned())
    } else {
        Err(PodcastError::YtDlp(classify_stderr(
            &String::from_utf8_lossy(stderr),
        )))
    }
}

pub(super) fn finalize_download(stdout: &str, destination: &Path) -> Result<(), PodcastError> {
    let produced = stdout
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| {
            PodcastError::YtDlp("yt-dlp did not report the downloaded file".to_string())
        })?;
    let produced_is_regular_file = produced
        .symlink_metadata()
        .is_ok_and(|metadata| metadata.file_type().is_file());
    if !produced_is_regular_file {
        return Err(PodcastError::YtDlp(format!(
            "yt-dlp did not create {}",
            produced.display()
        )));
    }
    if produced == destination {
        return Ok(());
    }
    if destination.exists() {
        return Err(PodcastError::YtDlp(format!(
            "download destination already exists: {}",
            destination.display()
        )));
    }

    let produced_parent = canonical_parent(&produced)?;
    let destination_parent = canonical_parent(destination)?;
    if produced_parent != destination_parent {
        return Err(PodcastError::YtDlp(
            "yt-dlp reported a file outside the download destination".to_string(),
        ));
    }
    std::fs::rename(&produced, destination).map_err(|error| {
        PodcastError::YtDlp(format!(
            "could not finalize podcast download at {}: {error}",
            destination.display()
        ))
    })
}

fn canonical_parent(path: &Path) -> Result<PathBuf, PodcastError> {
    path.parent()
        .unwrap_or_else(|| Path::new("."))
        .canonicalize()
        .map_err(|error| {
            PodcastError::YtDlp(format!(
                "could not resolve podcast download directory: {error}"
            ))
        })
}

fn parse_playlist(body: &str) -> Result<YtDlpPlaylist, PodcastError> {
    let value: Value =
        serde_json::from_str(body).map_err(|error| PodcastError::Parse(error.to_string()))?;
    let entries = value
        .get("entries")
        .and_then(Value::as_array)
        .ok_or_else(|| PodcastError::Parse("yt-dlp response has no entries".to_string()))?;
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

fn parse_resolved_audio(body: &str) -> Result<ResolvedAudio, PodcastError> {
    let value: Value =
        serde_json::from_str(body).map_err(|error| PodcastError::Parse(error.to_string()))?;
    let stream_url = value
        .get("url")
        .and_then(Value::as_str)
        .filter(|url| !url.is_empty())
        .ok_or_else(|| PodcastError::Parse("yt-dlp response has no audio URL".to_string()))?;
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
#[path = "ytdlp_tests.rs"]
mod tests;
