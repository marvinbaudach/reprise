use std::io;
use std::path::Path;

use crate::writeback_publish::{publish, Published};

use super::TimedLine;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SidecarWrite {
    Written,
    AlreadyPresent,
    NotApplicable,
    Failed,
}

pub(super) fn write_sidecar(track_path: &Path, lines: &[TimedLine]) -> SidecarWrite {
    if lines.is_empty() || !track_path.is_file() {
        return SidecarWrite::NotApplicable;
    }
    let target = track_path.with_extension("lrc");
    match target.try_exists() {
        Ok(true) => return SidecarWrite::AlreadyPresent,
        Ok(false) => {}
        Err(error) => return failed(&target, &error),
    }

    match publish(&target, render_lrc(lines).as_bytes()) {
        Ok(Published::Written) => SidecarWrite::Written,
        Ok(Published::AlreadyPresent) => SidecarWrite::AlreadyPresent,
        Err(error) => failed(&target, &error),
    }
}

fn render_lrc(lines: &[TimedLine]) -> String {
    let mut output = String::new();
    for line in lines {
        let start_ms = line.start_ms.max(0);
        let minutes = start_ms / 60_000;
        let seconds = (start_ms % 60_000) / 1_000;
        let centiseconds = (start_ms % 1_000) / 10;
        output.push_str(&format!(
            "[{minutes:02}:{seconds:02}.{centiseconds:02}]{}\n",
            line.text
        ));
    }
    output
}

fn failed(target: &Path, error: &io::Error) -> SidecarWrite {
    tracing::warn!(
        path = %target.display(),
        %error,
        "could not write lyrics sidecar"
    );
    SidecarWrite::Failed
}

#[cfg(test)]
#[path = "sidecar_write_tests.rs"]
mod tests;
