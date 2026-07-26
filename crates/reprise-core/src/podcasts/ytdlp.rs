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

use super::PodcastError;

const BLOCKED_MESSAGE: &str = "YouTube blocked the request — update yt-dlp (Preferences)";
const MISSING_MESSAGE: &str = "yt-dlp is not installed — YouTube sources are disabled";
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

    pub fn resolve(&self, video_url: &str) -> Result<ResolvedAudio, PodcastError> {
        let output = self.run(
            ["--no-warnings", "-f", "bestaudio", "-j", video_url],
            self.timeouts.resolve,
        )?;
        parse_resolved_audio(&output)
    }

    pub fn download(&self, video_url: &str, output: &Path) -> Result<(), PodcastError> {
        let produced_path = self.run(
            [
                OsString::from("--no-warnings"),
                OsString::from("-f"),
                OsString::from("bestaudio"),
                OsString::from("-x"),
                OsString::from("--print"),
                OsString::from("after_move:filepath"),
                OsString::from("-o"),
                output.as_os_str().to_os_string(),
                OsString::from(video_url),
            ],
            self.timeouts.download,
        )?;
        finalize_download(&produced_path, output)
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

fn read_in_background(
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

fn collect_output(
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
fn configure_process_group(command: &mut Command) {
    use std::os::unix::process::CommandExt;

    command.process_group(0);
}

#[cfg(not(unix))]
fn configure_process_group(_command: &mut Command) {}

fn terminate_process_tree(child: &mut Child) {
    terminate_process_group(child.id());
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(unix)]
fn terminate_process_group(process_group: u32) {
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
fn terminate_process_group(_process_group: u32) {}

fn map_spawn_error(error: &std::io::Error) -> PodcastError {
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

fn finalize_download(stdout: &str, destination: &Path) -> Result<(), PodcastError> {
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
mod tests {
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
    printf '%s\n' '{"title":"Channel title","entries":[{"id":"v1","title":"One","duration":12.8},{"id":"","title":"Blank ID"},{"id":"v2","title":"Two","duration":null},{"id":"blank-title","title":"   "}]}'
    ;;
  *) printf '%s\n' "unexpected arguments: $*" >&2; exit 2 ;;
esac
"#,
        );
        let runner = YtDlp::with_binary_and_timeouts(binary, short_timeouts());

        assert_eq!(runner.probe_version().unwrap(), "2026.07.26");

        let playlist = runner.list("https://youtube.test/@show").unwrap();
        assert_eq!(playlist.title.as_deref(), Some("Channel title"));
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
                "-f",
                "bestaudio",
                "-x",
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

    #[test]
    fn missing_binary_and_failed_process_are_readable() {
        let missing =
            YtDlp::with_binary_and_timeouts("/definitely/missing/reprise-yt-dlp", short_timeouts());
        assert_eq!(
            missing.probe_version().unwrap_err().to_string(),
            "yt-dlp is not installed — YouTube sources are disabled"
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
}
