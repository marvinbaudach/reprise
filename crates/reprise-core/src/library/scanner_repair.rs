//! In-place repair of files whose tag container the strict reader can't parse,
//! run during import. Split out of `scanner.rs` to keep it under the 800-line
//! rule; it is the scanner's only tag-writing seam (all other scan reads are
//! read-only).

use std::path::Path;

use crate::models::ImportErrorKind;

use super::track_meta;

/// Strips the damaged ID3v2/APE/ID3v1 containers of `path` and writes a fresh
/// ID3v2 carrying the file name as title and the parent folder as album, then
/// re-reads it as a normal tagged import. Returns `None` when the repair
/// doesn't apply (only `UnreadableTags` is repairable this way) or fails (e.g.
/// a read-only file), so the caller keeps the plain untagged import rather than
/// aborting the scan.
pub(super) fn repair_damaged_tags(
    path: &Path,
    meta: &track_meta::TrackMeta,
    kind: ImportErrorKind,
) -> Option<track_meta::TrackMeta> {
    if kind != ImportErrorKind::UnreadableTags {
        return None;
    }
    let title = path.file_stem().and_then(|stem| stem.to_str())?;
    if title.is_empty() {
        return None;
    }
    let patch = crate::library::tag_edit::TagPatch {
        title: Some(title.to_string()),
        album: Some(meta.album.clone()),
        ..crate::library::tag_edit::TagPatch::default()
    };
    // Suppress the watcher for the write we are about to make.
    crate::library::watcher::ignore_path(path, crate::library::tag_mutation::IGNORE_DURATION);
    crate::library::tag_mutation::strip_and_rewrite_tag(path, &patch).ok()?;
    track_meta::read_meta(path).ok()
}
