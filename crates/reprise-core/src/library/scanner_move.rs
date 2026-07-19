//! Move detection (Stage 2 Task 8): `find_move_candidate` and its supporting
//! types, relocated here (Task 1.8) purely to keep `scanner.rs` itself under
//! the project's 800-line rule — same rationale as `scanner_vanish.rs`'s and
//! `scanner_mount.rs`'s own module doc comments. `scanner.rs` declares this
//! via `#[path = "scanner_move.rs"] mod move_detect;`, so this is still the
//! crate-private `crate::library::scanner::move_detect` module. A pure
//! relocation, not a rewrite — the logic and its doc comments are unchanged
//! from when they lived inline in `scanner.rs`.

use std::collections::HashSet;
use std::path::Path;

use super::track_meta::TrackMeta;
use super::ScanError;

/// The one duration tolerance shared by automatic move matching and the
/// user-confirmed Locate mismatch probe.
pub(crate) const MOVE_MATCH_TOLERANCE_MS: i64 = 2_000;

/// A DB row that is a *candidate* to be the pre-move identity of a file at
/// an unknown path: `id`/`path` to perform the move `UPDATE` against.
#[derive(Debug, PartialEq)]
pub(crate) struct MoveCandidate {
    pub(crate) id: i64,
    pub(crate) path: String,
}

/// Everything `find_move_candidate` needs to know about the file it's
/// looking for a pre-move identity of. Bundled into one struct (rather than
/// seven positional arguments) purely to stay under clippy's
/// `too_many_arguments` lint.
pub(crate) struct MoveLookup<'a> {
    pub(crate) device: i64,
    pub(crate) inode: i64,
    pub(crate) title: &'a str,
    pub(crate) artist: &'a str,
    pub(crate) album: &'a str,
    pub(crate) duration_ms: i64,
    pub(crate) file_size: i64,
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
fn valid_candidates(
    rows: Vec<(i64, String, Option<i64>)>,
    allowed_ids: Option<&HashSet<i64>>,
) -> Vec<MoveCandidate> {
    rows.into_iter()
        .filter(|(id, _, _)| allowed_ids.is_none_or(|allowed| allowed.contains(id)))
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
    find_move_candidate_inner(tx, lookup, None)
}

pub(crate) fn find_move_candidate_in(
    tx: &rusqlite::Transaction,
    lookup: &MoveLookup,
    allowed_ids: &HashSet<i64>,
) -> Result<Option<MoveCandidate>, ScanError> {
    find_move_candidate_inner(tx, lookup, Some(allowed_ids))
}

fn find_move_candidate_inner(
    tx: &rusqlite::Transaction,
    lookup: &MoveLookup,
    allowed_ids: Option<&HashSet<i64>>,
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
    let mut candidates = valid_candidates(rows, allowed_ids);
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
        let fingerprint_query = format!(
            "SELECT id, path, missing_since FROM tracks WHERE title = ?1 AND artist = ?2 \
             AND album = ?3 AND ABS(duration_ms - ?4) <= {MOVE_MATCH_TOLERANCE_MS} \
             AND file_size = ?5"
        );
        let mut stmt = tx.prepare(&fingerprint_query)?;
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
    let mut candidates = valid_candidates(rows, allowed_ids);
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

/// The filesystem-identity fields `apply_file_identity` writes, bundled
/// purely to stay under clippy's `too_many_arguments` lint (same reasoning
/// as [`MoveLookup`]). This is deliberately exactly what a caller already
/// has in hand from `file_stat`/`file_mtime`/`mount::MountPointCache::
/// resolve` — no field here that isn't already computed by every call site
/// before it ever reaches this function.
pub(crate) struct FileIdentity {
    pub(crate) file_mtime: i64,
    pub(crate) file_size: i64,
    pub(crate) device: Option<i64>,
    pub(crate) inode: Option<i64>,
    pub(crate) mount_point: Option<String>,
}

/// Task 1.9: the ONE row-refresh used by move detection (below, in `scanner.
/// rs`) and, later, the "Locate…" feature (Task 5.1) when a user hand-picks
/// a replacement file for a row the scanner itself couldn't relink —
/// extracted from move detection's own `UPDATE` (previously inlined there)
/// so a second copy of this SQL never has the chance to drift from the
/// first. Sets `path` and every tag-derived column (via `tag_param_values`,
/// the same helper the INSERT/upsert arm uses, so the column order can only
/// ever be changed in one place) plus the filesystem-identity columns
/// carried in `fs`, on the existing row identified by `track_id`.
///
/// `rating`/`play_count`/`added_at`/`last_played_at` are deliberately absent
/// from the `SET` clause — untouched is the whole point of relinking a
/// track to its (possibly relocated) file, not re-importing it as if it
/// were new; see this codebase's `move_via_rename_preserves_metadata` and
/// `move_via_copy_delete_preserves_metadata` tests for the user-visible
/// promise ("your ratings are still there") this guarantees.
///
/// Also clears `missing_since`/`missing_reason`/`removed_at`: the caller
/// just proved — by successfully reading `path`'s tags/properties — that
/// the file really is there, and evidence on disk outranks whatever the row
/// believed beforehand, whether that was merely "missing" or fully
/// tombstoned via "Remove from library" (the evidence rule, Beschluss
/// 7/12). For move detection specifically this resurrect is usually
/// theoretical today (nothing sets `removed_at` yet — see this crate's
/// `removed_at` column doc comment in `db.rs`), but a hand-picked Locate
/// match against a tombstoned row is exactly the real case this was built
/// ahead of.
pub(crate) fn apply_file_identity(
    tx: &rusqlite::Transaction,
    track_id: i64,
    path: &Path,
    title: &str,
    meta: &TrackMeta,
    untagged: bool,
    fs: &FileIdentity,
) -> Result<(), ScanError> {
    let (
        title_p,
        artist_p,
        album_p,
        album_artist_p,
        artist_mbid_p,
        year_p,
        track_no_p,
        disc_no_p,
        genre_p,
        duration_ms_p,
        bitrate_kbps_p,
        untagged_p,
    ) = super::tag_param_values(title, meta, untagged);
    tx.execute(
        "UPDATE tracks SET path=?1, title=?2, artist=?3, album=?4,
           album_artist=?5, artist_mbid=COALESCE(?6, artist_mbid),
           artist_mbid_negative=CASE WHEN ?6 IS NOT NULL THEN 0 ELSE artist_mbid_negative END,
           year=?7, track_no=?8, disc_no=?9, genre=?10, duration_ms=?11,
           bitrate_kbps=?12, file_mtime=?13, file_size=?14, device=?15,
           inode=?16, mount_point=?17, untagged=?18, missing_since=NULL,
           missing_reason=NULL, removed_at=NULL
         WHERE id=?19",
        rusqlite::params![
            path.to_string_lossy(),
            title_p,
            artist_p,
            album_p,
            album_artist_p,
            artist_mbid_p,
            year_p,
            track_no_p,
            disc_no_p,
            genre_p,
            duration_ms_p,
            bitrate_kbps_p,
            fs.file_mtime,
            fs.file_size,
            fs.device,
            fs.inode,
            fs.mount_point,
            untagged_p,
            track_id,
        ],
    )?;
    Ok(())
}
