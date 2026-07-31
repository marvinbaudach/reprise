//! Best-effort publication of downloaded album covers into local album folders.

use std::io;
use std::path::{Path, PathBuf};

use crate::writeback_publish::{publish, Published};

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
    match publish(&target, bytes) {
        Ok(Published::Written) => CoverWrite::Written(target),
        Ok(Published::AlreadyPresent) => CoverWrite::AlreadyPresent,
        Err(error) => failed(&target, &error),
    }
}

fn failed(target: &Path, error: &io::Error) -> CoverWrite {
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
