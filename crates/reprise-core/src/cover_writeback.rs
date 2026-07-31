//! Best-effort publication of downloaded album covers into local album folders.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

const TEMP_CREATE_ATTEMPTS: usize = 16;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CoverWrite {
    Written(PathBuf),
    AlreadyPresent,
    NotApplicable,
    Failed,
}

pub fn write_album_cover(album_dirs: &[PathBuf], bytes: &[u8], ext: &str) -> Vec<CoverWrite> {
    let valid = crate::cover_download::validated_image_extension(bytes) == Some(ext)
        && crate::cover::IMAGE_EXTS.contains(&ext);
    album_dirs
        .iter()
        .map(|directory| {
            if !valid || !directory.is_dir() {
                return CoverWrite::NotApplicable;
            }
            if crate::cover::folder_image(directory).is_some() {
                return CoverWrite::AlreadyPresent;
            }
            write_one(directory, bytes, ext)
        })
        .collect()
}

fn write_one(directory: &Path, bytes: &[u8], ext: &str) -> CoverWrite {
    let target = directory.join(format!("cover.{ext}"));
    let (temporary, mut file) = match create_temporary(&target) {
        Ok(temporary) => temporary,
        Err(error) => return failed(&target, None, &error),
    };
    if let Err(error) = file.write_all(bytes).and_then(|()| file.sync_all()) {
        return failed(&target, Some(&temporary), &error);
    }
    drop(file);

    match fs::hard_link(&temporary, &target) {
        Ok(()) => {
            if let Err(error) = fs::remove_file(&temporary) {
                tracing::warn!(
                    path = %temporary.display(),
                    %error,
                    "could not remove published album-cover temporary file"
                );
            }
            CoverWrite::Written(target)
        }
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            let _ = fs::remove_file(&temporary);
            CoverWrite::AlreadyPresent
        }
        Err(error) => failed(&target, Some(&temporary), &error),
    }
}

fn create_temporary(target: &Path) -> io::Result<(PathBuf, File)> {
    let name = target
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("cover.image");
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
        "could not reserve a unique album-cover temporary file",
    ))
}

fn failed(target: &Path, temporary: Option<&Path>, error: &io::Error) -> CoverWrite {
    if let Some(temporary) = temporary {
        let _ = fs::remove_file(temporary);
    }
    tracing::warn!(
        path = %target.display(),
        %error,
        "could not write album cover into the library"
    );
    CoverWrite::Failed
}

#[cfg(test)]
#[path = "cover_writeback_tests.rs"]
mod tests;
