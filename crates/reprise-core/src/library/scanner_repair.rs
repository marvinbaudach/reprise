//! In-place repair of files whose tag container the strict reader can't parse,
//! run during import. Split out of `scanner.rs` to keep it under the 800-line
//! rule; it is the scanner's only tag-writing seam (all other scan reads are
//! read-only).

use std::path::Path;

use crate::models::ImportErrorKind;

use super::track_meta;

/// Repairs a file whose tag container lofty couldn't parse — NON-DESTRUCTIVELY.
///
/// The overwhelmingly common cause of an "unreadable" MP3 is a damaged trailing
/// APEv2/ID3v1 footer that aborts lofty's read ("invalid item size") while the
/// ID3v2 up front — carrying the real title/artist/album/… — is perfectly
/// intact. So the repair strips ONLY that tail into a TEMP copy and re-reads
/// the copy. Only if the copy comes back with real tags does it atomically
/// replace the original; otherwise the temp is discarded and the **original is
/// left exactly as it was**, imported as untagged.
///
/// This never overwrites tags it could not read: a file whose real metadata
/// lofty simply can't parse (e.g. the front container itself is at fault) keeps
/// every byte it had. An earlier version fell back to stripping *everything*
/// and writing the file name / folder as tags — which silently destroyed the
/// real, still-present ID3v2 of any file that didn't recover. That fallback is
/// gone: guessing tags is never worth clobbering real ones.
///
/// Returns `None` when the repair doesn't apply (only `UnreadableTags` is
/// repairable) or the recovery didn't yield real tags, so the caller keeps the
/// plain untagged import rather than aborting the scan.
pub(super) fn repair_damaged_tags(
    path: &Path,
    _meta: &track_meta::TrackMeta,
    kind: ImportErrorKind,
) -> Option<track_meta::TrackMeta> {
    if kind != ImportErrorKind::UnreadableTags {
        return None;
    }

    // Recover into a sibling temp file so the original is untouched unless the
    // recovery succeeds. Same directory ⇒ the final `rename` is atomic. The
    // temp gets a NON-audio extension (`.reprise-repair-tmp`) so the walk in
    // progress never mistakes it for a track to import — it is read back by
    // CONTENT (`read_meta_content_based`), not by extension.
    let temp = path.with_extension("reprise-repair-tmp");
    if crate::library::tag_mutation::write_tail_stripped(path, &temp).is_err() {
        let _ = std::fs::remove_file(&temp);
        return None;
    }

    // Accept only a recovery that actually produced real tags. If the tags
    // lived solely in the stripped tail (or the front container is also
    // unreadable), the temp reads empty/erroring — discard it and leave the
    // original file as-is.
    let recovered = match track_meta::read_meta_content_based(&temp) {
        Ok(meta) if !meta.title.is_empty() || !meta.artist.is_empty() || !meta.album.is_empty() => {
            meta
        }
        _ => {
            let _ = std::fs::remove_file(&temp);
            return None;
        }
    };

    // Commit: suppress the watcher for the replacement write, then swap the
    // recovered copy in atomically.
    crate::library::watcher::ignore_path(path, crate::library::tag_mutation::IGNORE_DURATION);
    if std::fs::rename(&temp, path).is_err() {
        let _ = std::fs::remove_file(&temp);
        return None;
    }
    Some(recovered)
}
