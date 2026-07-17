//! Move detection (Stage 2 Task 8): `find_move_candidate` and its supporting
//! types, relocated here (Task 1.8) purely to keep `scanner.rs` itself under
//! the project's 800-line rule — same rationale as `scanner_vanish.rs`'s and
//! `scanner_mount.rs`'s own module doc comments. `scanner.rs` declares this
//! via `#[path = "scanner_move.rs"] mod move_detect;`, so this is still the
//! crate-private `crate::library::scanner::move_detect` module. A pure
//! relocation, not a rewrite — the logic and its doc comments are unchanged
//! from when they lived inline in `scanner.rs`.

use std::path::Path;

use super::ScanError;

/// A DB row that is a *candidate* to be the pre-move identity of a file at
/// an unknown path: `id`/`path` to perform the move `UPDATE` against.
#[derive(Debug, PartialEq)]
pub(super) struct MoveCandidate {
    pub(super) id: i64,
    pub(super) path: String,
}

/// Everything `find_move_candidate` needs to know about the file it's
/// looking for a pre-move identity of. Bundled into one struct (rather than
/// seven positional arguments) purely to stay under clippy's
/// `too_many_arguments` lint.
pub(super) struct MoveLookup<'a> {
    pub(super) device: i64,
    pub(super) inode: i64,
    pub(super) title: &'a str,
    pub(super) artist: &'a str,
    pub(super) album: &'a str,
    pub(super) duration_ms: i64,
    pub(super) file_size: i64,
}

/// Filters raw SQL matches down to *valid* move candidates: rows whose old
/// path is gone from disk, or which are already flagged missing (`missing_
/// since` set). This filter is applied — and candidates counted — only over
/// this valid subset, never over the raw SQL match count. That ordering
/// matters: two DB rows can share a fingerprint (duplicate tracks) while
/// only one of their files has actually disappeared; counting the raw
/// matches would flag that as a false ambiguity and refuse a move that is in
/// fact unambiguous (see the
/// `one_deleted_one_alive_duplicate_is_still_an_unambiguous_move` test).
fn valid_candidates(rows: Vec<(i64, String, Option<i64>)>) -> Vec<MoveCandidate> {
    rows.into_iter()
        .filter(|(_, path, missing_since)| missing_since.is_some() || !Path::new(path).exists())
        .map(|(id, path, _)| MoveCandidate { id, path })
        .collect()
}

/// Resolves a moved-file candidate for a file at an as-yet-unknown path,
/// trying (1) exact `(device, inode)` — a same-filesystem `rename` — then
/// (2) a tag+duration+size fingerprint — a cross-filesystem copy+delete,
/// where the inode changes but the content and tags don't. Returns `Ok(None)`
/// both when nothing matches and when multiple rows match ambiguously (the
/// latter logs a `tracing::warn!` so the caller can fall back to a normal
/// insert without ever guessing which row to attach history to).
pub(super) fn find_move_candidate(
    tx: &rusqlite::Transaction,
    lookup: &MoveLookup,
) -> Result<Option<MoveCandidate>, ScanError> {
    let rows: Vec<(i64, String, Option<i64>)> = {
        let mut stmt = tx.prepare(
            "SELECT id, path, missing_since FROM tracks WHERE device = ?1 AND inode = ?2",
        )?;
        let mapped = stmt
            .query_map(rusqlite::params![lookup.device, lookup.inode], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })?
            .collect::<Result<_, _>>()?;
        mapped
    };
    let mut candidates = valid_candidates(rows);
    match candidates.len() {
        1 => return Ok(Some(candidates.remove(0))),
        n if n > 1 => {
            tracing::warn!(
                device = lookup.device,
                inode = lookup.inode,
                candidate_count = n,
                "ambiguous device/inode move candidates; not guessing"
            );
            return Ok(None);
        }
        _ => {}
    }

    let rows: Vec<(i64, String, Option<i64>)> = {
        let mut stmt = tx.prepare(
            "SELECT id, path, missing_since FROM tracks WHERE title = ?1 AND artist = ?2 \
             AND album = ?3 AND ABS(duration_ms - ?4) <= 2000 AND file_size = ?5",
        )?;
        let mapped = stmt
            .query_map(
                rusqlite::params![
                    lookup.title,
                    lookup.artist,
                    lookup.album,
                    lookup.duration_ms,
                    lookup.file_size
                ],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )?
            .collect::<Result<_, _>>()?;
        mapped
    };
    let mut candidates = valid_candidates(rows);
    match candidates.len() {
        1 => Ok(Some(candidates.remove(0))),
        n if n > 1 => {
            tracing::warn!(
                title = lookup.title,
                artist = lookup.artist,
                album = lookup.album,
                candidate_count = n,
                "ambiguous fingerprint move candidates; not guessing"
            );
            Ok(None)
        }
        _ => Ok(None),
    }
}
