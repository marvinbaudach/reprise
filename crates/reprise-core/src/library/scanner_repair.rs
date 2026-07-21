//! In-place repair of files whose tag container the strict reader can't parse,
//! run during import. Split out of `scanner.rs` to keep it under the 800-line
//! rule; it is the scanner's only tag-writing seam (all other scan reads are
//! read-only).

use std::path::Path;

use crate::models::ImportErrorKind;

use super::track_meta;

/// Repairs a file whose tag container lofty couldn't parse, preferring to
/// PRESERVE its real metadata.
///
/// The overwhelmingly common cause of an "unreadable" MP3 is a damaged trailing
/// APEv2/ID3v1 footer that aborts lofty's read ("invalid item size") while the
/// ID3v2 up front — carrying the real title/artist/album/… — is perfectly
/// intact. So the first, preferred repair strips ONLY that tail and re-reads:
/// on success the file imports as a normal tagged track with its real metadata
/// recovered, no data lost. Only when even that fails (the front container is
/// unusable too) does it fall back to stripping everything and synthesizing
/// minimal tags from the file name / parent folder, so the file at least
/// imports as an editable track.
///
/// Returns `None` when the repair doesn't apply (only `UnreadableTags` is
/// repairable this way) or every attempt fails (e.g. a read-only file), so the
/// caller keeps the plain untagged import rather than aborting the scan.
pub(super) fn repair_damaged_tags(
    path: &Path,
    meta: &track_meta::TrackMeta,
    kind: ImportErrorKind,
) -> Option<track_meta::TrackMeta> {
    if kind != ImportErrorKind::UnreadableTags {
        return None;
    }
    // Suppress the watcher for the write(s) we are about to make.
    crate::library::watcher::ignore_path(path, crate::library::tag_mutation::IGNORE_DURATION);

    // Preferred: strip only the damaged tail, keeping the front ID3v2, then
    // re-read. Recovers the real tags for the common APE-footer-corruption case.
    // Only accept it when real tags actually came back: if the tags lived
    // solely in the stripped tail (no front ID3v2), the re-read succeeds but is
    // empty — that is not a recovery, so fall through to the fallback below.
    if crate::library::tag_mutation::strip_trailing_tag_containers(path).is_ok() {
        if let Ok(recovered) = track_meta::read_meta(path) {
            if !recovered.title.is_empty()
                || !recovered.artist.is_empty()
                || !recovered.album.is_empty()
            {
                return Some(recovered);
            }
        }
    }

    // Last resort: the container is broken beyond the tail. Strip everything and
    // synthesize minimal tags from the file name / folder.
    let title = path.file_stem().and_then(|stem| stem.to_str())?;
    if title.is_empty() {
        return None;
    }
    let patch = crate::library::tag_edit::TagPatch {
        title: Some(title.to_string()),
        album: Some(meta.album.clone()),
        ..crate::library::tag_edit::TagPatch::default()
    };
    crate::library::tag_mutation::strip_and_rewrite_tag(path, &patch).ok()?;
    track_meta::read_meta(path).ok()
}
