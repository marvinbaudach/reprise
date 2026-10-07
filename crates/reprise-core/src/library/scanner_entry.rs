//! What the scan decides about **one** entry the walk delivered: classify the
//! file, write the catalog row it earns, and report back what happened as a
//! value. The counting lives with the walk in `scanner.rs`; nothing here
//! touches a counter, so all ten ways an entry can end are visible in one
//! enum instead of scattered through the pre-split body's seven early
//! returns.

use std::path::Path;

use crate::library::source::{
    LibraryLinkMode, LibraryPathMetadata, LibraryPathPresence, LibrarySource,
};
use crate::library::{exclusions, import_errors};

use super::cue_sheets::{Cover, CueDirectories, SheetRef};
use super::segments::{self, Placement, SheetIssue};
use super::{move_detect, repair, track_meta, ScanError};

pub(super) struct EntryScan<'a, 'conn, 'source> {
    pub(super) source: &'source dyn LibrarySource,
    pub(super) tx: &'a rusqlite::Transaction<'conn>,
    pub(super) mount_cache: &'a mut super::mount::MountPointCache<'source>,
    pub(super) cues: &'a mut CueDirectories,
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
    /// An audio file written to the catalog, as one track or, for a CUE sheet,
    /// as the `tracks` it holds.
    Imported {
        is_update: bool,
        healed: u32,
        tracks: u32,
    },
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
pub(super) struct FileFacts {
    pub(super) mtime: i64,
    pub(super) file_size: i64,
    identity: Option<(i64, i64)>,
    pub(super) device: Option<i64>,
    pub(super) inode: Option<i64>,
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

/// What the catalog already records for this exact path: one row for an
/// ordinary file, several for the tracks of a CUE sheet. The facts are read
/// across all of them, and anything the rows disagree on reads as "changed".
#[derive(Clone, Default, PartialEq, Eq)]
pub(super) struct KnownRow {
    exists: bool,
    mtime: Option<i64>,
    missing: bool,
    removed: bool,
    untagged: bool,
    tag_scan_version: i64,
    sheet: KnownSheet,
}

/// The sheet beside the file that its rows were written under.
#[derive(Clone, Default, PartialEq, Eq)]
enum KnownSheet {
    /// No row names a sheet: a plain file, or tracks from a sheet embedded in it.
    #[default]
    None,
    /// Every row was written under this sheet at this mtime and size.
    Applied {
        path: String,
        mtime: i64,
        size: Option<i64>,
    },
    /// The rows disagree, or hold a whole-file track beside tracks.
    Mixed,
}

impl KnownRow {
    /// Whether a sheet beside the file wrote any of its rows.
    fn was_cut_by_a_sheet(&self) -> bool {
        !matches!(self.sheet, KnownSheet::None)
    }

    /// Whether the rows were written under exactly the sheet that governs the
    /// file now, or under none when none does.
    fn matches_sheet(&self, governing: Option<&SheetRef>) -> bool {
        match (&self.sheet, governing) {
            (KnownSheet::None, None) => true,
            (KnownSheet::Applied { path, mtime, size }, Some(sheet)) => {
                *path == sheet.path_text() && *mtime == sheet.mtime && *size == Some(sheet.size)
            }
            _ => false,
        }
    }
}

type KnownRowColumns = (
    i64,
    Option<i64>,
    Option<i64>,
    i64,
    i64,
    i64,
    Option<i64>,
    i64,
    i64,
    i64,
    Option<String>,
    Option<String>,
    Option<i64>,
    Option<i64>,
    Option<i64>,
    Option<i64>,
);

fn known_row(tx: &rusqlite::Transaction, path_str: &str) -> KnownRow {
    // Query failure is deliberately indistinguishable from an absent row:
    // both preserve the scanner's unknown-mtime retry behaviour through the
    // default `KnownRow`. Do not replace this `.ok()` with error propagation.
    let known: Option<KnownRowColumns> = tx
        .prepare_cached(
            "SELECT count(*), min(file_mtime), max(file_mtime), count(missing_since),
                    count(removed_at), max(untagged), min(tag_scan_version),
                    count(CASE WHEN segment_index = 0 THEN 1 END),
                    count(CASE WHEN segment_index > 0 THEN 1 END), count(cue_path),
                    min(cue_path), max(cue_path), min(cue_mtime), max(cue_mtime),
                    min(cue_size), max(cue_size)
             FROM tracks WHERE path = ?1",
        )
        .ok()
        .and_then(|mut statement| {
            statement
                .query_row([path_str], |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                        row.get(7)?,
                        row.get(8)?,
                        row.get(9)?,
                        row.get(10)?,
                        row.get(11)?,
                        row.get(12)?,
                        row.get(13)?,
                        row.get(14)?,
                        row.get(15)?,
                    ))
                })
                .ok()
        })
        .filter(|columns| columns.0 > 0);
    let Some((
        rows,
        min_mtime,
        max_mtime,
        missing_rows,
        removed_rows,
        untagged,
        tag_scan_version,
        whole_rows,
        segment_rows,
        sheet_rows,
        min_sheet,
        max_sheet,
        min_sheet_mtime,
        max_sheet_mtime,
        min_sheet_size,
        max_sheet_size,
    )) = known
    else {
        return KnownRow::default();
    };
    let sheet = if whole_rows > 0 && segment_rows > 0 {
        KnownSheet::Mixed
    } else if sheet_rows == 0 {
        KnownSheet::None
    } else if sheet_rows == rows
        && min_sheet == max_sheet
        && min_sheet_mtime == max_sheet_mtime
        && min_sheet_size == max_sheet_size
    {
        match (min_sheet, min_sheet_mtime) {
            (Some(path), Some(mtime)) => KnownSheet::Applied {
                path,
                mtime,
                size: min_sheet_size,
            },
            _ => KnownSheet::Mixed,
        }
    } else {
        KnownSheet::Mixed
    };
    KnownRow {
        exists: true,
        mtime: (min_mtime == max_mtime).then_some(min_mtime).flatten(),
        missing: missing_rows > 0,
        // Task 1.9: a row can be tombstoned (`removed_at` set, via a future
        // "Remove from library") independently of ever having been marked
        // missing — evidence that the file is still sitting at its exact
        // recorded path outranks that removal (evidence rule, Beschluss
        // 7/12), so this reappearance check must fire for a tombstoned row
        // too, not only a missing one.
        removed: removed_rows > 0,
        // A present row still flagged `untagged` (an earlier scan couldn't parse
        // its container) must NOT take the unchanged-mtime fast path: excluding
        // it here drops it through to re-read + `repair_damaged_tags`, so a
        // library imported before auto-repair existed stops staying untagged.
        untagged: untagged != 0,
        tag_scan_version: tag_scan_version.unwrap_or(0),
        sheet,
    }
}

fn restore_present_row(
    scan: &mut EntryScan<'_, '_, '_>,
    path: &Path,
    path_str: &str,
    known: &KnownRow,
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

pub(super) enum EntryPlan {
    Skip(EntryOutcome),
    Import(Box<ImportPlan>),
}

pub(super) struct ImportPlan {
    path: std::path::PathBuf,
    path_str: String,
    facts: FileFacts,
    known: KnownRow,
    /// The sheet beside the file that describes it, if there is one.
    governing: Option<SheetRef>,
}

impl ImportPlan {
    pub(super) fn path(&self) -> &Path {
        &self.path
    }

    /// The sheet beside the file that describes it, if there is one.
    pub(super) fn governing(&self) -> Option<&SheetRef> {
        self.governing.as_ref()
    }
}

pub(super) fn classify_entry(
    scan: &mut EntryScan<'_, '_, '_>,
    path: &Path,
    metadata: Option<LibraryPathMetadata>,
) -> Result<EntryPlan, ScanError> {
    if !super::is_audio_file(path) {
        return Ok(EntryPlan::Skip(EntryOutcome::NotAudio));
    }
    let path_str = path.to_string_lossy().to_string();
    let facts = file_facts(scan.source, path, metadata);
    if exclusions::matches_file(scan.tx, path, facts.device, facts.inode)? {
        return Ok(EntryPlan::Skip(EntryOutcome::Excluded));
    }
    let known = known_row(scan.tx, &path_str);
    let governing = match scan.cues.covering(scan.source, scan.tx, path)? {
        Cover::Sheet(sheet) => Some(sheet),
        Cover::Plain => None,
        // Whether a sheet beside it cuts this file cannot be told this scan.
        // Rows a sheet cut stay exactly as they are. Any other file is read as
        // if no sheet were there, which is how its rows came about, so that
        // loses nothing either; a sheet that is new beside it waits for a scan
        // that can see it. A file with every track hidden has no rows, and its
        // exclusions say the same.
        Cover::Unknown
            if known.was_cut_by_a_sheet()
                || (!known.exists
                    && exclusions::was_hidden_by_a_sheet(
                        scan.tx,
                        path,
                        facts.device,
                        facts.inode,
                    )?) =>
        {
            return Ok(EntryPlan::Skip(EntryOutcome::Unchanged));
        }
        Cover::Unknown => None,
    };
    if !known.exists && hidden_file_unchanged(scan, path, &facts, governing.as_ref())? {
        return Ok(EntryPlan::Skip(EntryOutcome::Unchanged));
    }
    if known.mtime == Some(facts.mtime)
        && known.tag_scan_version >= super::TAG_SCAN_VERSION
        && !known.untagged
        && known.matches_sheet(governing.as_ref())
    {
        if known.missing || known.removed {
            return restore_present_row(scan, path, &path_str, &known).map(EntryPlan::Skip);
        }
        return Ok(EntryPlan::Skip(EntryOutcome::Unchanged));
    }
    // Dismiss-skip fast path: a `stat`, not a tag parse. Must run BEFORE
    // `read_meta` — see `check_dismissed`'s doc comment. An `untagged` row
    // is exempt: a dismissal only silences the notification and predates
    // auto-repair, so skipping here would strand a now-repairable file
    // forever (its mtime never changes, so it is never re-read). Neither is a
    // file whose sheet changed: a dismissal is about the file as it was read,
    // and a sheet that arrived since may well apply.
    if !known.untagged
        && known.matches_sheet(governing.as_ref())
        && import_errors::check_dismissed(
            scan.tx,
            &path_str,
            facts.mtime,
            facts.file_size,
            super::now_unix(),
        )?
    {
        return Ok(EntryPlan::Skip(EntryOutcome::Dismissed));
    }
    Ok(EntryPlan::Import(Box::new(ImportPlan {
        path: path.to_path_buf(),
        path_str,
        facts,
        known,
        governing,
    })))
}

/// A CUE file whose every track the user removed has no row to be unchanged
/// against; its exclusions stand in for the rows (see
/// [`exclusions::hidden_file_unchanged`]).
fn hidden_file_unchanged(
    scan: &EntryScan<'_, '_, '_>,
    path: &Path,
    facts: &FileFacts,
    governing: Option<&SheetRef>,
) -> Result<bool, ScanError> {
    let sheet_text = governing.map(SheetRef::path_text);
    let governing = governing
        .zip(sheet_text.as_deref())
        .map(|(sheet, text)| (text, sheet.mtime, sheet.size));
    Ok(exclusions::hidden_file_unchanged(
        scan.tx,
        path,
        facts.device,
        facts.inode,
        facts.mtime,
        governing,
    )?)
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
    layout: &segments::Layout,
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
            tracks_album: segments::tracks_album(layout, meta),
        },
    )
}

pub(super) struct ImportedTrack<'a> {
    pub(super) title: &'a str,
    pub(super) meta: &'a track_meta::TrackMeta,
    pub(super) untagged: bool,
    pub(super) mount_point: Option<String>,
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
                           file_mtime, file_size, device, inode, mount_point, untagged,
                           rg_track_gain, rg_track_peak, rg_album_gain, rg_album_peak,
                           tag_scan_version, segment_index, segment_start_ms, segment_end_ms,
                           cue_path, cue_mtime, cue_size)
                         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23,?24,
                                 ?25,?26,?27,?28,?29,?30)
                         ON CONFLICT(path, segment_index) DO UPDATE SET
                           title=?2, artist=?3, album=?4, album_artist=?5,
                           artist_mbid=COALESCE(?6, artist_mbid),
                           artist_mbid_negative=CASE WHEN ?6 IS NOT NULL THEN 0 ELSE artist_mbid_negative END,
                           year=?7, track_no=?8, disc_no=?9, genre=?10,
                           duration_ms=?11, bitrate_kbps=?12, file_mtime=?14,
                           missing_since=NULL, missing_reason=NULL, removed_at=NULL,
                           file_size=?15, device=?16, inode=?17, mount_point=?18,
                           untagged=?19, rg_track_gain=?20, rg_track_peak=?21,
                           rg_album_gain=?22, rg_album_peak=?23, tag_scan_version=?24,
                           segment_start_ms=?26, segment_end_ms=?27,
                           cue_path=?28, cue_mtime=?29, cue_size=?30";

// `ON CONFLICT(path, segment_index)` fires whenever this path already
// has a row for that track — including one still carrying `removed_at`
// from a prior tombstone: the walk just proved the file
// is there, so `removed_at=NULL` in the `DO UPDATE SET`
// below resurrects it here too (evidence rule, Beschluss
// 7/12), same as the fast-path-restore branch and
// `apply_file_identity`'s move arm above.
pub(super) fn upsert_track(
    scan: &EntryScan<'_, '_, '_>,
    path_str: &str,
    facts: &FileFacts,
    imported: &ImportedTrack<'_>,
    placement: &Placement<'_>,
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
        rg_track_gain,
        rg_track_peak,
        rg_album_gain,
        rg_album_peak,
        tag_scan_version,
    ) = params;
    let cue_path = placement.sheet.map(SheetRef::path_text);
    let cue_mtime = placement.sheet.map(|sheet| sheet.mtime);
    let cue_size = placement.sheet.map(|sheet| sheet.size);
    scan.tx
        .prepare_cached(UPSERT_TRACK_SQL)?
        .execute(rusqlite::params![
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
            rg_track_gain,
            rg_track_peak,
            rg_album_gain,
            rg_album_peak,
            tag_scan_version,
            placement.segment_index,
            placement.start_ms,
            placement.end_ms,
            cue_path,
            cue_mtime,
            cue_size,
        ])?;
    Ok(())
}

fn import_readable_entry(
    scan: &mut EntryScan<'_, '_, '_>,
    plan: &ImportPlan,
    is_update: bool,
    outcome: track_meta::MetaOutcome,
) -> Result<EntryOutcome, ScanError> {
    let ImportPlan {
        path,
        path_str,
        facts,
        governing,
        ..
    } = plan;
    let (meta, hint) = read_import_meta(path, outcome);
    let untagged = hint.is_some();
    let title = if meta.title.is_empty() {
        scan.source.display_name(path).unwrap_or_default()
    } else {
        meta.title.clone()
    };
    // The sheets are consulted before anything is written: a sheet that could
    // not be read leaves the file's rows, and its issue, as they are.
    let Some(plan) = segments::plan_layout(scan, path, governing.as_ref(), &meta)? else {
        return Ok(EntryOutcome::Unchanged);
    };
    let layout = plan.layout;
    // Task 1.6: recorded now, while still reachable, and
    // memoized per parent dir — see `scanner_mount.rs`.
    let mount_point = scan.mount_cache.resolve(path);
    // A broken embedded sheet is the file's issue: it replaces the clearing a
    // clean read would do, so its dismissal survives the read.
    let healed = match (&hint, &plan.issue) {
        (None, Some(SheetIssue::Embedded(_))) => 0,
        _ => record_hint_or_healing(scan.tx, path_str, hint)?,
    };
    if let Some(issue) = &plan.issue {
        segments::report_issue(scan, path_str, facts, issue)?;
    }
    let candidate = find_move(scan, facts, &title, &meta, &layout, is_update)?;
    let imported = ImportedTrack {
        title: &title,
        meta: &meta,
        untagged,
        mount_point,
    };
    let Some(candidate) = candidate else {
        let tracks = segments::write_layout(scan, path, path_str, facts, &imported, &layout)?;
        return Ok(EntryOutcome::Imported {
            is_update,
            healed,
            tracks,
        });
    };
    if candidate.segmented {
        segments::move_segments(
            scan,
            &candidate.path,
            path,
            facts,
            imported.mount_point.as_deref(),
        )?;
        let healed = healed + u32::from(import_errors::clear_error(scan.tx, &candidate.path)?);
        segments::write_layout(scan, path, path_str, facts, &imported, &layout)?;
        return Ok(EntryOutcome::Moved { healed });
    }
    let moved = apply_move(scan, path, facts, &imported, &candidate, healed)?;
    if !layout.is_plain_whole() {
        // A file that moved and gained a sheet on the way: its row is already
        // at the new path, and now becomes the sheet's tracks.
        segments::write_layout(scan, path, path_str, facts, &imported, &layout)?;
    }
    Ok(moved)
}

pub(super) fn apply_entry(
    scan: &mut EntryScan<'_, '_, '_>,
    plan: &ImportPlan,
    meta_result: Result<track_meta::MetaOutcome, ScanError>,
) -> Result<EntryOutcome, ScanError> {
    if known_row(scan.tx, &plan.path_str) != plan.known {
        return Ok(EntryOutcome::Unchanged);
    }
    let is_update = plan.known.exists;
    match meta_result {
        Ok(outcome) => import_readable_entry(scan, plan, is_update, outcome),
        Err(ScanError::Import { kind, detail }) => {
            // Both passes failed: `kind`/`detail` are pass 2's
            // classification (see `read_meta_with_fallback`'s doc
            // comment). Episode upsert — see `record_error`'s doc
            // comment.
            import_errors::record_error(scan.tx, &plan.path_str, kind, &detail, super::now_unix())?;
            Ok(EntryOutcome::ImportFailed)
        }
        // `read_meta_with_fallback` only ever produces `Import`;
        // propagating any other variant is safer than an
        // `unreachable!()` panic if that changes.
        Err(other) => Err(other),
    }
}
