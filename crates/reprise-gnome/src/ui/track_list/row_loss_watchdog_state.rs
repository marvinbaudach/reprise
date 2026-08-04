//! GTK-free row-loss decision state and diagnostic dump persistence.

use std::path::{Path, PathBuf};
use std::{fs, io::Write as _};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RecoveryOutcome {
    Worked,
    Failed,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct Recovery {
    pub after_ms: u64,
    pub rows: usize,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct TickDecision {
    pub confirmed: bool,
    pub request_self_heal: bool,
    pub self_heal_outcome: Option<RecoveryOutcome>,
    pub recovered: Option<Recovery>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TickInput {
    pub suspicious: bool,
    pub rows: usize,
    pub now_ms: u64,
}

#[derive(Debug, Default)]
pub(crate) struct WatchdogState {
    consecutive_suspicious: u8,
    episode_started_ms: Option<u64>,
    confirmed: bool,
    self_heal_pending: bool,
}

impl WatchdogState {
    pub(crate) fn tick(&mut self, input: TickInput, self_heal: bool) -> TickDecision {
        if self.confirmed {
            let self_heal_outcome = self.self_heal_pending.then(|| {
                self.self_heal_pending = false;
                if input.rows > 0 {
                    RecoveryOutcome::Worked
                } else {
                    RecoveryOutcome::Failed
                }
            });
            let recovered = (input.rows > 0).then(|| Recovery {
                after_ms: input
                    .now_ms
                    .saturating_sub(self.episode_started_ms.unwrap_or(input.now_ms)),
                rows: input.rows,
            });
            if recovered.is_some() {
                *self = Self::default();
            }
            return TickDecision {
                self_heal_outcome,
                recovered,
                ..TickDecision::default()
            };
        }

        if !input.suspicious {
            self.consecutive_suspicious = 0;
            self.episode_started_ms = None;
            return TickDecision::default();
        }

        if self.consecutive_suspicious == 0 {
            self.episode_started_ms = Some(input.now_ms);
        }
        self.consecutive_suspicious = self.consecutive_suspicious.saturating_add(1);
        if self.consecutive_suspicious < 2 {
            return TickDecision::default();
        }

        self.confirmed = true;
        self.self_heal_pending = self_heal;
        TickDecision {
            confirmed: true,
            request_self_heal: self_heal,
            ..TickDecision::default()
        }
    }
}

pub(crate) fn self_heal_enabled(value: Option<&str>) -> bool {
    value != Some("diagnose-only")
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DumpSnapshot {
    pub app_version: String,
    pub git_sha: String,
    pub wall_clock: String,
    pub n_items: u32,
    pub stack_page: String,
    pub source: String,
    pub sort_field: String,
    pub sort_dir: String,
    pub filter: String,
    pub browse: String,
    pub exclude_ai: bool,
    pub adjustment_value: f64,
    pub adjustment_lower: f64,
    pub adjustment_upper: f64,
    pub adjustment_page_size: f64,
    pub column_mapped: bool,
    pub column_realized: bool,
    pub column_visible: bool,
    pub column_opacity: f64,
    pub column_width: i32,
    pub column_height: i32,
    pub scrolled_width: i32,
    pub scrolled_height: i32,
    pub window_query_error_count: u64,
    pub last_window_query_error: Option<String>,
    pub gdk_backend: String,
    pub gsk_renderer: String,
    pub animations_enabled: bool,
    pub trail: Vec<String>,
}

pub(crate) fn render_dump(snapshot: &DumpSnapshot) -> String {
    use std::fmt::Write as _;

    let mut output = String::new();
    let fields = [
        ("app_version", snapshot.app_version.as_str()),
        ("git_sha", snapshot.git_sha.as_str()),
        ("timestamp", snapshot.wall_clock.as_str()),
        ("stack_page", snapshot.stack_page.as_str()),
        ("source", snapshot.source.as_str()),
        ("sort_field", snapshot.sort_field.as_str()),
        ("sort_dir", snapshot.sort_dir.as_str()),
        ("filter", snapshot.filter.as_str()),
        ("browse", snapshot.browse.as_str()),
    ];
    for (name, value) in fields {
        let _ = writeln!(output, "{name}={}", single_line(value));
    }
    let _ = writeln!(output, "n_items={}", snapshot.n_items);
    let _ = writeln!(output, "exclude_ai={}", snapshot.exclude_ai);
    let _ = writeln!(output, "vadjustment.value={:.3}", snapshot.adjustment_value);
    let _ = writeln!(output, "vadjustment.lower={:.3}", snapshot.adjustment_lower);
    let _ = writeln!(output, "vadjustment.upper={:.3}", snapshot.adjustment_upper);
    let _ = writeln!(
        output,
        "vadjustment.page_size={:.3}",
        snapshot.adjustment_page_size
    );
    let _ = writeln!(output, "column_view.is_mapped={}", snapshot.column_mapped);
    let _ = writeln!(
        output,
        "column_view.is_realized={}",
        snapshot.column_realized
    );
    let _ = writeln!(output, "column_view.is_visible={}", snapshot.column_visible);
    let _ = writeln!(output, "column_view.opacity={:.3}", snapshot.column_opacity);
    let _ = writeln!(output, "column_view.width={}", snapshot.column_width);
    let _ = writeln!(output, "column_view.height={}", snapshot.column_height);
    let _ = writeln!(output, "scrolled_window.width={}", snapshot.scrolled_width);
    let _ = writeln!(
        output,
        "scrolled_window.height={}",
        snapshot.scrolled_height
    );
    let _ = writeln!(
        output,
        "window_query_error.count={}",
        snapshot.window_query_error_count
    );
    let _ = writeln!(
        output,
        "window_query_error.last={}",
        single_line(
            snapshot
                .last_window_query_error
                .as_deref()
                .unwrap_or("<none>")
        )
    );
    let _ = writeln!(output, "GDK_BACKEND={}", snapshot.gdk_backend);
    let _ = writeln!(output, "GSK_RENDERER={}", snapshot.gsk_renderer);
    let _ = writeln!(output, "animations_enabled={}", snapshot.animations_enabled);
    output.push_str("trail:\n");
    for line in &snapshot.trail {
        let _ = writeln!(output, "{line}");
    }
    output
}

fn single_line(value: &str) -> String {
    value.replace('\n', "\\n").replace('\r', "\\r")
}

pub(crate) fn write_dump_file(
    directory: &Path,
    filename_stamp: &str,
    snapshot: &DumpSnapshot,
) -> std::io::Result<PathBuf> {
    fs::create_dir_all(directory)?;
    let path = directory.join(format!("row-loss-{filename_stamp}.log"));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;
    file.write_all(render_dump(snapshot).as_bytes())?;
    Ok(path)
}

pub(crate) fn append_self_heal_outcome(
    path: &Path,
    outcome: RecoveryOutcome,
    trail_line: &str,
) -> std::io::Result<()> {
    let mut file = fs::OpenOptions::new().append(true).open(path)?;
    writeln!(file, "self_heal.recovery={}", outcome.as_str())?;
    writeln!(file, "{trail_line}")
}

pub(crate) fn retain_newest(directory: &Path, keep: usize) -> std::io::Result<()> {
    let mut dumps = fs::read_dir(directory)?
        .filter_map(Result::ok)
        .filter(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            name.starts_with("row-loss-") && name.ends_with(".log")
        })
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    dumps.sort();
    let remove_count = dumps.len().saturating_sub(keep);
    for path in dumps.into_iter().take(remove_count) {
        fs::remove_file(path)?;
    }
    Ok(())
}

impl RecoveryOutcome {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Worked => "worked",
            Self::Failed => "failed",
        }
    }
}

#[cfg(test)]
#[path = "row_loss_watchdog_state_tests.rs"]
mod tests;
