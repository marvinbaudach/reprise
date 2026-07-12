//! Cover-art resolution and thumbnailing (portable, GUI-free). Reads covers
//! from the user's files (embedded picture, or a sidecar image in the album
//! folder) and produces cached thumbnails under the XDG cache dir. NEVER
//! writes into the user's library — see `thumbnail`'s cache path.

use std::path::{Path, PathBuf};

/// Where a cover for a track comes from.
#[derive(Debug)]
pub enum CoverSource {
    /// A picture embedded in the audio file (via lofty).
    Embedded(Vec<u8>),
    /// An image file sitting in the album folder (cover.*, folder.*).
    FolderImage(PathBuf),
}

/// Canonical sidecar cover file stems and extensions, in priority order.
const FOLDER_STEMS: &[&str] = &["cover", "folder", "front", "album"];
const IMAGE_EXTS: &[&str] = &["jpg", "jpeg", "png", "webp", "gif", "bmp"];

/// Resolves the best available cover source for a track: first an embedded
/// picture, otherwise the first matching sidecar image in the track's folder,
/// otherwise None. Pure read — touches no cache and writes nothing.
pub fn resolve_source(track_path: &Path) -> Option<CoverSource> {
    if let Some(bytes) = embedded_picture(track_path) {
        return Some(CoverSource::Embedded(bytes));
    }
    let dir = track_path.parent()?;
    folder_image(dir).map(CoverSource::FolderImage)
}

/// Reads the first embedded picture's bytes from an audio file, if any.
fn embedded_picture(track_path: &Path) -> Option<Vec<u8>> {
    use lofty::prelude::*;
    let tagged = lofty::read_from_path(track_path).ok()?;
    let tag = tagged.primary_tag().or_else(|| tagged.first_tag())?;
    let picture = tag.pictures().first()?;
    Some(picture.data().to_vec())
}

/// Finds a sidecar cover image in `dir` by canonical stem + known extension,
/// case-insensitively, deterministically (stem-then-ext priority).
fn folder_image(dir: &Path) -> Option<PathBuf> {
    let entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    for stem in FOLDER_STEMS {
        for ext in IMAGE_EXTS {
            for path in &entries {
                let matches_stem = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .is_some_and(|s| s.eq_ignore_ascii_case(stem));
                let matches_ext = path
                    .extension()
                    .and_then(|s| s.to_str())
                    .is_some_and(|s| s.eq_ignore_ascii_case(ext));
                if matches_stem && matches_ext {
                    return Some(path.clone());
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    // A 1x1 PNG, enough for source-resolution tests (no decode here).
    const TINY_PNG: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F,
        0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x62, 0x00,
        0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49,
        0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];

    fn write(dir: &std::path::Path, name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let p = dir.join(name);
        let mut f = std::fs::File::create(&p).unwrap();
        f.write_all(bytes).unwrap();
        p
    }

    #[test]
    fn returns_none_when_no_embedded_and_no_folder_image() {
        let dir = tempfile::tempdir().unwrap();
        let track = write(dir.path(), "song.flac", b"not audio, no tag");
        assert!(resolve_source(&track).is_none());
    }

    #[test]
    fn falls_back_to_folder_image_cover_jpg() {
        let dir = tempfile::tempdir().unwrap();
        let track = write(dir.path(), "song.flac", b"not audio");
        let cover = write(dir.path(), "cover.jpg", TINY_PNG); // content irrelevant here
        match resolve_source(&track) {
            Some(CoverSource::FolderImage(p)) => assert_eq!(p, cover),
            other => panic!("expected FolderImage, got {other:?}"),
        }
    }

    #[test]
    fn folder_image_matches_are_case_insensitive_and_prioritized() {
        // "Folder.PNG" must be found; the first canonical name wins deterministically.
        let dir = tempfile::tempdir().unwrap();
        let track = write(dir.path(), "song.flac", b"x");
        let _ = write(dir.path(), "Folder.PNG", TINY_PNG);
        assert!(matches!(
            resolve_source(&track),
            Some(CoverSource::FolderImage(_))
        ));
    }
}
