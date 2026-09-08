//! What the scan decides about **one** entry the walk delivered: classify the
//! file, write the catalog row it earns, and report back what happened as a
//! value. The counting lives with the walk in `scanner.rs`; nothing here
//! touches a counter, so all ten outcomes are visible in one enum, preserving
//! the arithmetic formerly scattered across the pre-split body's seven early returns.

use std::path::Path;

use crate::library::source::{
    LibraryLinkMode, LibraryPathMetadata, LibraryPathPresence, LibrarySource,
};
use crate::library::{exclusions, import_errors};

use super::{move_detect, repair, track_meta, ScanError};

pub(super) struct EntryScan<'a, 'conn, 'source> {
    pub(super) source: &'source dyn LibrarySource,
    pub(super) tx: &'a rusqlite::Transaction<'conn>,
    pub(super) mount_cache: &'a mut super::mount::MountPointCache<'source>,
}

/// What one walk entry turned out to be, and what the scan did about it.
/// Every counter this decision earns is applied by the caller's fold
/// (`WalkState::record`), so an entry's classification and the report's
/// arithmetic can no longer drift apart.
pub(super) enum EntryOutcome {
    /// A traversal error the walk reported instead of an entry.
    WalkError,
    /// A directory the walk entered.
    Directory,
    /// A file with no audio extension.
    NotAudio,
    /// An audio file an exclusion rule claims.
    Excluded,
    /// An audio file whose row is unchanged since the last scan.
    Unchanged,
    /// An audio file whose row was flagged missing or removed, and whose
    /// unchanged mtime at its recorded path proves it is back.
    Restored { healed: u32 },
    /// An audio file whose earlier import error the user dismissed.
    Dismissed,
    /// An audio file recognised as a relocation of a row the catalog knows.
    Moved { healed: u32 },
    /// An audio file written to the catalog.
    Imported { is_update: bool, healed: u32 },
    /// An audio file neither tag pass could read.
    ImportFailed,
}

impl EntryOutcome {
    /// Whether this outcome came from an audio file the scan actually
    /// examined. That is one set, not two: it is both the root guard's
    /// `audio_files_seen` evidence and the set of entries that move the
    /// progress reporter. A directory, a non-audio file and a traversal
    /// error are evidence about neither.
    pub(super) fn examined_audio_file(&self) -> bool {
        !matches!(self, Self::WalkError | Self::Directory | Self::NotAudio)
    }
}

/// The filesystem facts one entry contributes, read once before any tag work.
/// `has_file_stat` is false only when the metadata query itself failed, which
/// is what disqualifies the entry from move detection.
struct FileFacts {
    mtime: i64,
    file_size: i64,
    identity: Option<(i64, i64)>,
    device: Option<i64>,
    inode: Option<i64>,
    has_file_stat: bool,
}

fn file_facts(
    source: &dyn LibrarySource,
    path: &Path,
    metadata: Option<LibraryPathMetadata>,
) -> FileFacts {
    // Compute identity before touching tags. An exclusion follows the
    // same file across a rename and must win over move detection.
    let metadata = metadata.or_else(|| match source.probe(path, LibraryLinkMode::Follow) {
        LibraryPathPresence::Present(metadata) => Some(metadata),
        LibraryPathPresence::Absent | LibraryPathPresence::Unknown => None,
    });
    let (mtime, stat) = super::scanner_file_metadata(metadata);
    let has_file_stat = stat.is_some();
    let (file_size, identity): (i64, Option<(i64, i64)>) = match stat {
        Some((size, identity)) => (
            size as i64,
            identity.map(|(device, inode)| (device as i64, inode as i64)),
        ),
        None => (0, None),
    };
    let (device, inode) =
        identity.map_or((None, None), |(device, inode)| (Some(device), Some(inode)));
    FileFacts {
        mtime,
        file_size,
        identity,
        device,
        inode,
        has_file_stat,
    }
}

/// What the catalog already records for this exact path.
#[derive(Clone, Copy, Default)]
struct KnownRow {
    mtime: Option<i64>,
    missing: bool,
    removed: bool,
    untagged: bool,
}

fn known_row(tx: &rusqlite::Transaction, path_str: &str) -> KnownRow {
    // Query failure is deliberately indistinguishable from an absent row:
    // both preserve the scanner's unknown-mtime retry behaviour through the
    // default `KnownRow`. Do not replace this `.ok()` with error propagation.
    let known: Option<(i64, Option<i64>, Option<i64>, i64)> = tx
        .query_row(
            "SELECT file_mtime, missing_since, removed_at, untagged FROM tracks WHERE path = ?1",
            [path_str],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .ok();
    KnownRow {
        mtime: known.map(|(file_mtime, ..)| file_mtime),
        missing: known.is_some_and(|(_, missing_since, ..)| missing_since.is_some()),
        // Task 1.9: a row can be tombstoned (`removed_at` set, via a future
        // "Remove from library") independently of ever having been marked
        // missing — evidence that the file is still sitting at its exact
        // recorded path outranks that removal (evidence rule, Beschluss
        // 7/12), so this reappearance check must fire for a tombstoned row
        // too, not only a missing one.
        removed: known.is_some_and(|(_, _, removed_at, _)| removed_at.is_some()),
        // A present row still flagged `untagged` (an earlier scan couldn't parse
        // its container) must NOT take the unchanged-mtime fast path: excluding
        // it here drops it through to re-read + `repair_damaged_tags`, so a
        // library imported before auto-repair existed stops staying untagged.
        untagged: known.is_some_and(|(_, _, _, untagged)| untagged != 0),
    }
}

fn restore_present_row(
    scan: &mut EntryScan<'_, '_, '_>,
    path: &Path,
    path_str: &str,
    known: KnownRow,
) -> Result<EntryOutcome, ScanError> {
    // The file reappeared at its exact recorded path with an
    // unchanged mtime (NAS remount, restore-from-trash, or a
    // tombstoned row whose object turned out to still be
    // there): the ordinary incremental fast path would
    // otherwise skip it forever, silently ignoring `missing_
    // since`/`removed_at` — this is the one case the fast path
    // must NOT take, since the row still needs both cleared
    // even though nothing else changed. This is also the ONLY
    // chance a row whose `mount_point` is NULL (a pre-schema-v10
    // row, or any row that was never re-scanned since) has to
    // acquire one without its file actually changing — see
    // `scanner_mount.rs`'s module doc comment.
    let mount_point = scan.mount_cache.resolve(path);
    scan.tx.execute(
        "UPDATE tracks SET missing_since = NULL, missing_reason = NULL, \
                     removed_at = NULL, mount_point = ?2 WHERE path = ?1",
        rusqlite::params![path_str, mount_point],
    )?;
    let healed = u32::from(import_errors::clear_error(scan.tx, path_str)?);
    tracing::info!(
        path = %path_str,
        was_missing = known.missing,
        was_removed = known.removed,
        "restored track from evidence (unchanged mtime)"
    );
    Ok(EntryOutcome::Restored { healed })
}

pub(super) fn scan_entry(
    scan: &mut EntryScan<'_, '_, '_>,
    path: &Path,
    metadata: Option<LibraryPathMetadata>,
) -> Result<EntryOutcome, ScanError> {
    if !super::is_audio_file(path) {
        return Ok(EntryOutcome::NotAudio);
    }
    let path_str = path.to_string_lossy().to_string();
    let facts = file_facts(scan.source, path, metadata);
    if exclusions::matches_file(scan.tx, path, facts.device, facts.inode)? {
        return Ok(EntryOutcome::Excluded);
    }
    let known = known_row(scan.tx, &path_str);
    if known.mtime == Some(facts.mtime) && !known.untagged {
        if known.missing || known.removed {
            return restore_present_row(scan, path, &path_str, known);
        }
        return Ok(EntryOutcome::Unchanged);
    }
    // Dismiss-skip fast path: a `stat`, not a tag parse. Must run BEFORE
    // `read_meta` — see `check_dismissed`'s doc comment. An `untagged` row
    // is exempt: a dismissal only silences the notification and predates
    // auto-repair, so skipping here would strand a now-repairable file
    // forever (its mtime never changes, so it is never re-read).
    if !known.untagged
        && import_errors::check_dismissed(
            scan.tx,
            &path_str,
            facts.mtime,
            facts.file_size,
            super::now_unix(),
        )?
    {
        return Ok(EntryOutcome::Dismissed);
    }
    import_entry(scan, path, &path_str, &facts, known.mtime.is_some())
}

fn read_import_meta(
    path: &Path,
    outcome: track_meta::MetaOutcome,
) -> (
    track_meta::TrackMeta,
    Option<(crate::models::ImportErrorKind, String)>,
) {
    // Task 1.8: `hint` is `Some((kind, detail))` only when pass 1
    // failed but pass 2 rescued the container — see
    // `scan_folder_inner`'s `## Hint coexistence` doc section.
    match outcome {
        track_meta::MetaOutcome::Tagged(meta) => (meta, None),
        // A file the strict reader couldn't parse is repaired in
        // place (damaged containers stripped, fresh ID3v2 written
        // from the file name / folder), then re-read as a normal
        // tagged import. On any repair failure it stays untagged.
        track_meta::MetaOutcome::Untagged { meta, kind, detail } => {
            match repair::repair_damaged_tags(path, &meta, kind) {
                Some(repaired) => (repaired, None),
                None => (meta, Some((kind, detail))),
            }
        }
    }
}

fn record_hint_or_healing(
    tx: &rusqlite::Transaction,
    path_str: &str,
    hint: Option<(crate::models::ImportErrorKind, String)>,
) -> Result<u32, ScanError> {
    // A pass-1 success clears any previous failure for this path
    // (a file that errored once and is now readable again must
    // not stay in the error log). A pass-2 (untagged) success
    // must NOT clear it — instead it refreshes the row with
    // pass 1's diagnosis, keeping it alive as a HINT. See
    // `scan_folder_inner`'s `## Hint coexistence` doc section.
    if let Some((kind, detail)) = hint {
        import_errors::record_error(tx, path_str, kind, &detail, super::now_unix())?;
        Ok(0)
    } else {
        // Task 1.9: a real pass-1 success (never the pass-2
        // hint-refresh branch above) that actually deleted a
        // prior error row — see `ScanReport::healed`'s doc
        // comment for why the hint case must never land here.
        Ok(u32::from(import_errors::clear_error(tx, path_str)?))
    }
}

fn find_move(
    scan: &EntryScan<'_, '_, '_>,
    facts: &FileFacts,
    title: &str,
    meta: &track_meta::TrackMeta,
    is_update: bool,
) -> Result<Option<move_detect::MoveCandidate>, ScanError> {
    // Move detection (Stage 2 Task 8) only ever applies to a path
    // the DB has never seen before — a file whose path is already
    // known just falls through to the ordinary upsert below, even
    // if its content changed.
    // Skip move detection entirely when `stat` failed above,
    // because step 2 would compare against an unknown size.
    // Missing identity skips only step 1; the real size still
    // makes the fingerprint strategy safe.
    if is_update || !facts.has_file_stat {
        return Ok(None);
    }
    move_detect::find_move_candidate_with_source(
        scan.source,
        scan.tx,
        &move_detect::MoveLookup {
            identity: facts.identity,
            title,
            artist: &meta.artist,
            album: &meta.album,
            duration_ms: meta.duration_ms,
            file_size: facts.file_size,
        },
    )
}

struct ImportedTrack<'a> {
    title: &'a str,
    meta: &'a track_meta::TrackMeta,
    untagged: bool,
    mount_point: Option<String>,
}

fn apply_move(
    scan: &EntryScan<'_, '_, '_>,
    path: &Path,
    facts: &FileFacts,
    imported: &ImportedTrack<'_>,
    candidate: &move_detect::MoveCandidate,
    healed: u32,
) -> Result<EntryOutcome, ScanError> {
    // A move: refresh path/tags/filesystem-identity on the
    // existing row by id via the shared `apply_file_identity`
    // — see its own doc comment for exactly what it touches
    // (and, deliberately, doesn't).
    move_detect::apply_file_identity(
        scan.tx,
        candidate.id,
        path,
        imported.title,
        imported.meta,
        imported.untagged,
        &move_detect::FileIdentity {
            file_mtime: facts.mtime,
            file_size: facts.file_size,
            device: facts.device,
            inode: facts.inode,
            mount_point: imported.mount_point.clone(),
        },
    )?;
    // Clear a stale import_errors row under the old path too
    // (e.g. the old location briefly failed to read before
    // being moved away) — the new path was already cleared
    // above. Unconditional even for an untagged import: this
    // is the OLD path's row, a different path string from
    // the hint (if any) recorded above for the CURRENT path.
    let healed = healed + u32::from(import_errors::clear_error(scan.tx, &candidate.path)?);
    Ok(EntryOutcome::Moved { healed })
}

const UPSERT_TRACK_SQL: &str =
    "INSERT INTO tracks (path, title, artist, album, album_artist, artist_mbid,
                           year, track_no, disc_no, genre, duration_ms, bitrate_kbps, added_at,
                           file_mtime, file_size, device, inode, mount_point, untagged)
                         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19)
                         ON CONFLICT(path) DO UPDATE SET
                           title=?2, artist=?3, album=?4, album_artist=?5,
                           artist_mbid=COALESCE(?6, artist_mbid),
                           artist_mbid_negative=CASE WHEN ?6 IS NOT NULL THEN 0 ELSE artist_mbid_negative END,
                           year=?7, track_no=?8, disc_no=?9, genre=?10,
                           duration_ms=?11, bitrate_kbps=?12, file_mtime=?14,
                           missing_since=NULL, missing_reason=NULL, removed_at=NULL,
                           file_size=?15, device=?16, inode=?17, mount_point=?18,
                           untagged=?19";

// `ON CONFLICT(path)` fires whenever this path already
// has a row — including one still carrying `removed_at`
// from a prior tombstone: the walk just proved the file
// is there, so `removed_at=NULL` in the `DO UPDATE SET`
// below resurrects it here too (evidence rule, Beschluss
// 7/12), same as the fast-path-restore branch and
// `apply_file_identity`'s move arm above.
fn upsert_track(
    scan: &EntryScan<'_, '_, '_>,
    path_str: &str,
    facts: &FileFacts,
    imported: &ImportedTrack<'_>,
) -> Result<(), ScanError> {
    let params = super::tag_param_values(imported.title, imported.meta, imported.untagged);
    let (
        title,
        artist,
        album,
        album_artist,
        artist_mbid,
        year,
        track_no,
        disc_no,
        genre,
        duration_ms,
        bitrate_kbps,
        untagged,
    ) = params;
    scan.tx.execute(
        UPSERT_TRACK_SQL,
        rusqlite::params![
            path_str,
            title,
            artist,
            album,
            album_artist,
            artist_mbid,
            year,
            track_no,
            disc_no,
            genre,
            duration_ms,
            bitrate_kbps,
            super::now_unix(),
            facts.mtime,
            facts.file_size,
            facts.device,
            facts.inode,
            imported.mount_point,
            untagged,
        ],
    )?;
    Ok(())
}

fn import_readable_entry(
    scan: &mut EntryScan<'_, '_, '_>,
    path: &Path,
    path_str: &str,
    facts: &FileFacts,
    is_update: bool,
    outcome: track_meta::MetaOutcome,
) -> Result<EntryOutcome, ScanError> {
    let (meta, hint) = read_import_meta(path, outcome);
    let untagged = hint.is_some();
    let title = if meta.title.is_empty() {
        scan.source.display_name(path).unwrap_or_default()
    } else {
        meta.title.clone()
    };
    // Task 1.6: recorded now, while still reachable, and
    // memoized per parent dir — see `scanner_mount.rs`.
    let mount_point = scan.mount_cache.resolve(path);
    let healed = record_hint_or_healing(scan.tx, path_str, hint)?;
    let candidate = find_move(scan, facts, &title, &meta, is_update)?;
    let imported = ImportedTrack {
        title: &title,
        meta: &meta,
        untagged,
        mount_point,
    };
    if let Some(candidate) = candidate {
        return apply_move(scan, path, facts, &imported, &candidate, healed);
    }
    upsert_track(scan, path_str, facts, &imported)?;
    Ok(EntryOutcome::Imported { is_update, healed })
}

fn import_entry(
    scan: &mut EntryScan<'_, '_, '_>,
    path: &Path,
    path_str: &str,
    facts: &FileFacts,
    is_update: bool,
) -> Result<EntryOutcome, ScanError> {
    match track_meta::read_meta_with_fallback(scan.source, path) {
        Ok(outcome) => import_readable_entry(scan, path, path_str, facts, is_update, outcome),
        Err(ScanError::Import { kind, detail }) => {
            // Both passes failed: `kind`/`detail` are pass 2's
            // classification (see `read_meta_with_fallback`'s doc
            // comment). Episode upsert — see `record_error`'s doc
            // comment.
            import_errors::record_error(scan.tx, path_str, kind, &detail, super::now_unix())?;
            Ok(EntryOutcome::ImportFailed)
        }
        // `read_meta_with_fallback` only ever produces `Import`;
        // propagating any other variant is safer than an
        // `unreachable!()` panic if that changes.
        Err(other) => Err(other),
    }
}
