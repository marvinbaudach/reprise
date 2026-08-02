//! Best-effort publication of downloaded album covers into local album folders.

use std::io;
use std::path::{Path, PathBuf};

use crate::library::source::{LibraryLinkMode, LibrarySource, UnixLibrarySource};
use crate::writeback_publish::{publish_with_source, Published};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CoverWrite {
    Written(PathBuf),
    AlreadyPresent,
    NotApplicable,
    Failed,
}

pub fn write_album_cover(album_dirs: &[PathBuf], bytes: &[u8], ext: &str) -> Vec<CoverWrite> {
    write_album_cover_with_source(&UnixLibrarySource, album_dirs, bytes, ext)
}

pub fn write_album_cover_with_source(
    source: &dyn LibrarySource,
    album_dirs: &[PathBuf],
    bytes: &[u8],
    ext: &str,
) -> Vec<CoverWrite> {
    let valid = crate::cover_download::validated_image_extension(bytes) == Some(ext)
        && crate::cover::IMAGE_EXTS.contains(&ext);
    album_dirs
        .iter()
        .map(|directory| {
            if !valid
                || !source
                    .probe(directory, LibraryLinkMode::Follow)
                    .is_some_and(|metadata| metadata.is_directory)
            {
                return CoverWrite::NotApplicable;
            }
            if album_has_artwork(source, directory) {
                return CoverWrite::AlreadyPresent;
            }
            write_one(source, directory, bytes, ext)
        })
        .collect()
}

/// Whether the album in `directory` already has artwork of its own — a
/// canonical folder image, or an embedded picture in any of its tracks.
///
/// The folder-image half is `COVER-1`'s original never-overwrite check. The
/// embedded half is what keeps the writeback to filling gaps: the download
/// worker also fetches when an album's tracks carry *different* embedded
/// pictures (`BROWSE-10`, routine for compilations), and that is a
/// canonicalization for Reprise's own cache, not a missing cover. Writing
/// `cover.jpg` there would put a file into a folder where every track already
/// had correct artwork, and every other player reads it too.
///
/// An unreadable directory counts as covered: nothing can be established
/// about it, and refusing is the conservative half of that.
fn album_has_artwork(source: &dyn LibrarySource, directory: &Path) -> bool {
    if crate::cover::folder_image_with_source(source, directory).is_some() {
        return true;
    }
    let Some(entries) = source.read_directory(directory) else {
        return true;
    };
    entries.into_iter().any(|entry| {
        let path = entry.path;
        crate::library::scanner::is_audio_file(&path)
            && crate::cover::read_cover_tag(&path).picture.is_some()
    })
}

fn write_one(source: &dyn LibrarySource, directory: &Path, bytes: &[u8], ext: &str) -> CoverWrite {
    let target = directory.join(format!("cover.{ext}"));
    match publish_with_source(source, &target, bytes) {
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
