use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use super::TimedLine;

const TEMP_CREATE_ATTEMPTS: usize = 16;

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
        Err(error) => return failed(&target, None, &error),
    }

    let contents = render_lrc(lines);
    let (temporary, mut file) = match create_temporary(&target) {
        Ok(temporary) => temporary,
        Err(error) => return failed(&target, None, &error),
    };
    if let Err(error) = file
        .write_all(contents.as_bytes())
        .and_then(|()| file.sync_all())
    {
        return failed(&target, Some(&temporary), &error);
    }
    drop(file);

    match fs::hard_link(&temporary, &target) {
        Ok(()) => {
            if let Err(error) = fs::remove_file(&temporary) {
                tracing::warn!(
                    path = %temporary.display(),
                    %error,
                    "could not remove published lyrics sidecar temporary file"
                );
            }
            SidecarWrite::Written
        }
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            let _ = fs::remove_file(&temporary);
            SidecarWrite::AlreadyPresent
        }
        Err(error) => failed(&target, Some(&temporary), &error),
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

fn create_temporary(target: &Path) -> io::Result<(PathBuf, File)> {
    let name = target
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("lyrics.lrc");
    for _ in 0..TEMP_CREATE_ATTEMPTS {
        let temporary =
            target.with_file_name(format!(".{name}.reprise-{:016x}.tmp", fastrand::u64(..)));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
        {
            Ok(file) => return Ok((temporary, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "could not reserve a unique lyrics sidecar temporary file",
    ))
}

fn failed(target: &Path, temporary: Option<&Path>, error: &io::Error) -> SidecarWrite {
    if let Some(temporary) = temporary {
        let _ = fs::remove_file(temporary);
    }
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
