use rusqlite::Connection;

use crate::db::Db;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use super::import_errors;
use super::source::{
    self, LibraryEntry, LibraryLinkMode, LibraryPathMetadata, LibraryPathPresence, LibrarySource,
    LibraryWalkControl, LibraryWalkError, LibraryWalkErrorKind, LibraryWalkItem, LibraryWalkOrder,
    UnixLibrarySource,
};
use crate::models::ImportErrorKind;

use entry::EntryOutcome;

#[path = "scanner_types.rs"]
mod scanner_types;
pub use scanner_types::{
    finalize_completed_scan, ScanError, ScanOutcome, ScanProgress, ScanReport, ScanResult,
};

const AUDIO_EXTENSIONS: [&str; 7] = ["mp3", "flac", "ogg", "opus", "m4a", "aac", "wav"];

type FileStat = (u64, Option<(u64, u64)>);
type FileMetadata = (i64, Option<FileStat>);

/// `(mtime, stat)` from one metadata query. `mtime` is zero when the query or
/// timestamp conversion fails, preserving the scanner's "unknown, always
/// retry" database value. `stat` is `None` only when the metadata query itself
/// fails; a timestamp failure does not discard otherwise valid file facts.
///
/// `stat` is `(file_size, identity)` for move detection. Unix supplies its stable
/// `(device, inode)` identity through `std::os::unix::fs::MetadataExt`;
/// platforms without that identity still supply the real file size and use
/// the fingerprint strategy alone. Returns `None` if `stat` itself fails, in
/// which case the scanner skips move detection rather than matching with an
/// unknown size. The database representation remains `file_size = 0` plus
/// `NULL` device/inode for that failure case.
///
/// **A platform arm must never fabricate an identity.** The non-Unix arm used
/// to return `(0, 0)` under a comment claiming it was never reached at
/// runtime — true only while the app was Linux-only. A Tauri desktop makes it
/// false, and then `WHERE device = 0 AND inode = 0` matches every row scanned
/// there; with exactly one valid candidate that attaches one track's history
/// to another, silently. `None` is the only honest answer for a platform
/// without a stable identity.
fn scanner_file_metadata(metadata: Option<LibraryPathMetadata>) -> FileMetadata {
    let mtime = metadata
        .as_ref()
        .and_then(|metadata| metadata.modified)
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |duration| duration.as_secs() as i64);
    let stat = metadata.and_then(|metadata| metadata.size.map(|size| (size, metadata.identity)));
    (mtime, stat)
}

pub(crate) fn is_audio_file(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .is_some_and(|extension| AUDIO_EXTENSIONS.contains(&extension.as_str()))
}

/// Return type for tag_param_values: (title, artist, album, album_artist,
/// artist_mbid, year, track_no, disc_no, genre, duration_ms, bitrate_kbps,
/// untagged).
type TagParams<'a> = (
    &'a str,
    &'a str,
    &'a str,
    &'a str,
    Option<&'a str>,
    Option<i32>,
    Option<i32>,
    Option<i32>,
    &'a str,
    i64,
    Option<i32>,
    i64,
);

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// Extracts tag-derived column values in the canonical order used by both
/// move-UPDATE and INSERT/upsert statements: title, artist, album, album_artist,
/// artist_mbid, year, track_no, disc_no, genre, duration_ms, bitrate_kbps,
/// untagged. Having a single
/// source for this ordering ensures that adding/removing columns is a
/// one-place change. `untagged` (Task 1.8) is threaded through here rather
/// than left for each call site to append separately, same reasoning as
/// every other column in this tuple.
fn tag_param_values<'a>(
    title: &'a str,
    meta: &'a track_meta::TrackMeta,
    untagged: bool,
) -> TagParams<'a> {
    (
        title,
        &meta.artist,
        &meta.album,
        &meta.album_artist,
        meta.artist_mbid.as_deref(),
        meta.year,
        meta.track_no,
        meta.disc_no,
        &meta.genre,
        meta.duration_ms,
        meta.bitrate_kbps,
        i64::from(untagged),
    )
}

pub fn scan_folder(db: &Db, root: &Path) -> Result<ScanOutcome, ScanError> {
    let conn = db.conn();
    scan_folder_inner(&UnixLibrarySource, conn, root, None)
}

#[cfg(test)]
fn scan_folder_with_source(
    source: &dyn LibrarySource,
    db: &Db,
    root: &Path,
) -> Result<ScanOutcome, ScanError> {
    let conn = db.conn();
    scan_folder_inner(source, conn, root, None)
}

pub(crate) fn scan_folder_in(conn: &Connection, root: &Path) -> Result<ScanOutcome, ScanError> {
    scan_folder_inner(&UnixLibrarySource, conn, root, None)
}

pub fn scan_folder_with_progress(
    db: &Db,
    root: &Path,
    mut on_progress: impl FnMut(ScanProgress),
) -> Result<ScanOutcome, ScanError> {
    scan_folder_with_progress_from(&UnixLibrarySource, db, root, &mut on_progress)
}

/// Scans through an explicitly selected library source while forwarding the
/// same progress contract as [`scan_folder_with_progress`]. Platform adapters
/// use this entry point; the desktop wrapper above remains pinned to
/// [`UnixLibrarySource`].
pub fn scan_folder_with_source_and_progress(
    source: &dyn LibrarySource,
    db: &Db,
    root: &Path,
    mut on_progress: impl FnMut(ScanProgress),
) -> Result<ScanOutcome, ScanError> {
    scan_folder_with_progress_from(source, db, root, &mut on_progress)
}

fn scan_folder_with_progress_from(
    source: &dyn LibrarySource,
    db: &Db,
    root: &Path,
    on_progress: &mut dyn FnMut(ScanProgress),
) -> Result<ScanOutcome, ScanError> {
    let conn = db.conn();
    on_progress(ScanProgress::Discovering);
    let total = scan_progress::estimated_audio_files(conn, root)?;
    let reporter = scan_progress::ScanProgressReporter::new(on_progress, total);
    scan_folder_inner(source, conn, root, Some(reporter))
}

/// What the walk delivered, and what the report owes for it.
struct WalkTrace {
    audio_files_seen: u64,
    observed_paths: HashSet<PathBuf>,
    dirs: HashSet<PathBuf>,
    failed: HashSet<PathBuf>,
}

struct WalkState {
    report: ScanReport,
    trace: WalkTrace,
}

impl WalkState {
    /// The single place a scan's counters move. Each variant's arithmetic is
    /// the arithmetic the pre-split body's seven early returns used to do inline.
    fn record(&mut self, outcome: &EntryOutcome) {
        // Root-Guard input: "did the walk find any audio file at all under
        // `root`?" — counted regardless of whether this particular file
        // goes on to be added/updated/skipped/errored below. See
        // `scan_folder_inner`'s `## Root guard` doc section.
        if outcome.examined_audio_file() {
            self.trace.audio_files_seen += 1;
        }
        match *outcome {
            EntryOutcome::WalkError => self.report.errors += 1,
            EntryOutcome::Directory | EntryOutcome::NotAudio | EntryOutcome::Dismissed => {}
            EntryOutcome::Excluded => self.report.excluded += 1,
            EntryOutcome::Unchanged => self.report.skipped_unchanged += 1,
            EntryOutcome::Restored { healed } => {
                self.report.updated += 1;
                self.report.healed += healed;
            }
            EntryOutcome::Moved { healed } => {
                self.report.moved += 1;
                self.report.healed += healed;
            }
            EntryOutcome::Imported { is_update, healed } => {
                if is_update {
                    self.report.updated += 1;
                } else {
                    self.report.added += 1;
                }
                self.report.healed += healed;
            }
            EntryOutcome::ImportFailed => self.report.errors += 1,
        }
    }
}

fn record_walk_error(
    source: &dyn LibrarySource,
    tx: &rusqlite::Transaction,
    failed: &mut HashSet<PathBuf>,
    root: &Path,
    error: &LibraryWalkError,
) -> Result<EntryOutcome, ScanError> {
    // A walk error may name a directory or an unstatable child;
    // poison both it and its parent before recording the error.
    let failed_path = error.path.as_deref().unwrap_or(root);
    vanish::poison_walk_failure(source, failed, failed_path);
    let err_path = failed_path.to_string_lossy().to_string();
    let kind = match error.kind {
        LibraryWalkErrorKind::PermissionDenied => ImportErrorKind::PermissionDenied,
        LibraryWalkErrorKind::Io => ImportErrorKind::Io,
        LibraryWalkErrorKind::Unknown => ImportErrorKind::Unknown,
    };
    import_errors::record_error(
        tx,
        &err_path,
        kind,
        &format!("directory traversal error: {}", error.detail),
        now_unix(),
    )?;
    Ok(EntryOutcome::WalkError)
}

/// Root-Guard case (a): a root the source cannot see is not evidence about any
/// file beneath it, so the scan reports back without a walk and without
/// touching the database at all. `None` means the walk may proceed.
fn guard_root_before_walk(source: &dyn LibrarySource, root: &Path) -> Option<ScanOutcome> {
    // No absoluteness assertion here any more. It used to live at this line and
    // it was the wrong layer: nothing the scanner does needs an absolute root —
    // it hands the root to the source and reads back what the source says. The
    // requirement belongs to `UnixLibrarySource`'s ancestor walk, and it now
    // sits there, next to the guarantee it protects.
    //
    // This is not a formality. A SAF root is a content URI, and
    // `Path::is_absolute` is false for one (it has no leading `/`), so this
    // assertion fired on the first scan a real Android source ever attempted.
    if source.probe(root, LibraryLinkMode::Follow) == LibraryPathPresence::Absent {
        // Root-Guard case (a): no walk, no database write at all — see
        // `scan_folder_inner`'s `## Root guard` doc section.
        tracing::warn!(
            root = %root.display(),
            "scan: root does not exist; reporting RootUnavailable without touching the database"
        );
        return Some(ScanOutcome::RootUnavailable {
            root: root.to_path_buf(),
        });
    }
    None
}

fn handle_walk_item(
    item: LibraryWalkItem,
    root: &Path,
    state: &mut WalkState,
    mobile_sync: &mut mobile_sync::MobileSyncDiscovery,
    scan: &mut entry::EntryScan<'_, '_, '_>,
    progress: &mut Option<scan_progress::ScanProgressReporter<'_>>,
) -> Result<(), ScanError> {
    let entry = match item {
        LibraryWalkItem::Error(error) => {
            let outcome =
                record_walk_error(scan.source, scan.tx, &mut state.trace.failed, root, &error)?;
            state.record(&outcome);
            return Ok(());
        }
        LibraryWalkItem::Entry(entry) => entry,
    };
    mobile_sync.observe(scan.source, root, &entry);
    let LibraryEntry {
        path,
        is_file,
        metadata,
    } = entry;
    state.trace.observed_paths.insert(path.clone());
    if !is_file {
        state.trace.dirs.insert(path);
        state.record(&EntryOutcome::Directory);
        return Ok(());
    }
    let outcome = entry::scan_entry(scan, &path, metadata)?;
    if outcome.examined_audio_file() {
        if let Some(progress) = progress {
            progress.advance(&path);
        }
    }
    state.record(&outcome);
    Ok(())
}

fn walk_root<'source>(
    source: &'source dyn LibrarySource,
    tx: &rusqlite::Transaction,
    root: &Path,
    state: &mut WalkState,
    mobile_sync: &mut mobile_sync::MobileSyncDiscovery,
    mount_cache: &mut mount::MountPointCache<'source>,
    progress: &mut Option<scan_progress::ScanProgressReporter<'_>>,
) -> Result<(), ScanError> {
    let mut scan = entry::EntryScan {
        source,
        tx,
        mount_cache,
    };
    let mut walk_failure = None;
    source::walk_with(
        source,
        root,
        LibraryWalkOrder::Native,
        |item| match handle_walk_item(item, root, state, mobile_sync, &mut scan, progress) {
            Ok(()) => LibraryWalkControl::Continue,
            Err(error) => {
                walk_failure = Some(error);
                LibraryWalkControl::Stop
            }
        },
    );
    if let Some(error) = walk_failure {
        return Err(error);
    }
    Ok(())
}

/// The metadata a mobile sync left beside the audio, applied inside the walk's
/// own transaction so the sidecars and the rows they describe commit together.
fn apply_mobile_sync(
    mobile_sync: &mobile_sync::MobileSyncDiscovery,
    source: &dyn LibrarySource,
    tx: &rusqlite::Transaction,
    report: &mut ScanReport,
) -> Result<(), ScanError> {
    report.updated = report
        .updated
        .saturating_add(mobile_sync.apply_metadata(source, tx)?);
    mobile_sync.register_analysis_sidecars(tx)?;
    mobile_sync.register_device_paths(tx)?;
    Ok(())
}

/// What the reconcile phase is allowed to reason about: the rows the catalog
/// still calls present under `root`, what the walk proved about the tree, and —
/// only when the walk found no audio file at all — the wider evidence the root
/// guard needs.
struct VanishEvidence {
    candidates: Vec<(i64, String, Option<i64>)>,
    evidence: Option<vanish::WalkEvidence>,
    guard_evidence: Option<Vec<(i64, String, Option<i64>)>>,
}

fn gather_vanish_evidence(
    tx: &rusqlite::Transaction,
    root: &Path,
    trace: WalkTrace,
) -> Result<VanishEvidence, ScanError> {
    let WalkTrace {
        audio_files_seen,
        observed_paths,
        dirs,
        failed,
    } = trace;
    // `candidates` (`PRESENT`-only) feeds the mark phase below regardless of
    // outcome. The guard's own evidence, `guard_evidence` (the wider
    // `removed_at IS NULL` list — see `scanner_vanish::guard_evidence_under_
    // root`'s doc comment for why it must NOT be `candidates`), is only
    // queried when the walk found nothing, the same short-circuit
    // `root_unavailable` used before this was split into two lists — so a
    // scan that actually found audio files never pays for the extra query.
    let candidates = vanish::present_candidates_under_root(tx, root)?;
    // A walk that saw no audio file at all is exactly the situation Android
    // cannot distinguish from lost storage. An empty walk is a question, not
    // proof: layer 3 stays silent and only a real source `Absent` still marks.
    let evidence = vanish::evidence_after_walk(audio_files_seen, observed_paths, &dirs, &failed);
    let guard_evidence = if audio_files_seen == 0 {
        Some(vanish::guard_evidence_under_root(tx, root)?)
    } else {
        None
    };
    Ok(VanishEvidence {
        candidates,
        evidence,
        guard_evidence,
    })
}

/// Root-Guard case (b) or the mark phase: decides whether this scan may say
/// anything about the files it did not see, and returns the outcome the
/// transaction will commit.
fn decide_outcome(
    source: &dyn LibrarySource,
    tx: &rusqlite::Transaction,
    root: &Path,
    evidence: VanishEvidence,
    mut report: ScanReport,
) -> Result<ScanOutcome, ScanError> {
    let root_unavailable = evidence.guard_evidence.as_ref().is_some_and(|guard| {
        !guard.is_empty() && !vanish::any_candidate_confirms_root_with(source, guard, root)
    });
    if root_unavailable {
        // Root-Guard case (b): see `scan_folder_inner`'s `## Root guard` doc
        // section. The upserts the walk itself produced (normally none,
        // since `audio_files_seen == 0`, but a traversal error is still
        // possible) still commit below — only the mark phase is skipped.
        tracing::warn!(
            root = %root.display(),
            candidate_count = evidence.guard_evidence.map_or(0, |e| e.len()),
            "scan: walk found no audio files and no known track under root confirms the \
             root's current device; reporting RootUnavailable instead of marking tracks missing"
        );
        return Ok(ScanOutcome::RootUnavailable {
            root: root.to_path_buf(),
        });
    }
    let reclassified =
        vanish::reclassify_missing_with(source, tx, root, evidence.evidence.as_ref(), now_unix())?;
    report.vanished = vanish::mark_vanished_with(
        source,
        tx,
        root,
        evidence.candidates,
        evidence.evidence.as_ref(),
    )?;
    // T0.3: one collective change-log row per scan that actually touched
    // the catalog (never per track, never for a no-op reconcile), inside
    // the same transaction as the walk so the event and the rows it
    // announces commit together. Foreign scanners (`reprise-cli scan`)
    // wake the running app through this; the app's own scans carry its
    // writer token and are filtered out by its own consumer.
    if scan_touched_library(&report) || reclassified > 0 {
        crate::events::record(tx, "library", "", "scan")?;
        crate::library::startup_tasks::advance_library_signature_in(tx)?;
    }
    Ok(ScanOutcome::Completed(report))
}

/// Walks `root`, upserting every audio file found, then — in the SAME
/// transaction — reconciles whatever the walk did NOT find: rows the DB
/// still believes are present under `root` whose file has actually vanished.
///
/// ## Fold: scan IS reconcile, not scan-then-mark
///
/// Through Stage 3, this was two separate calls: `scan_folder` (which
/// committed its own transaction), then the folder watcher separately called
/// `mark_vanished_under_root` — and that function's own doc comment spent
/// three paragraphs establishing a rule every caller had to remember: mark-
/// vanished must run strictly AFTER scan_folder, never before, because a
/// file moved/renamed within `root` is only reconciled by move detection
/// (which updates its row's `path` in place) during the walk — running
/// mark-vanished first would transiently and wrongly flag a moved-but-not-
/// yet-rescanned file as missing. A convention that needs three paragraphs
/// of documentation and that every call site must get in the right order
/// belongs in the structure, not in a comment: Task 1.5 folds the mark phase
/// into this function, after the walk loop, inside the walk's own `tx` —
/// there is now nothing left to call in the wrong order, and a move and an
/// unrelated deletion discovered in the same pass reconcile as one atomic
/// transaction instead of leaving a window (between the old two commits)
/// where the database briefly says a moved file is gone.
///
/// ## Root guard: no evidence about `root` must never look like "all gone"
///
/// A scan whose own `root` cannot be seen has no evidence about any
/// individual file under it — it only knows "my root is unreachable". Before
/// the walk even starts, a failed source probe short-circuits straight to
/// [`ScanOutcome::RootUnavailable`] with no walk and no database write at
/// all (`import_errors` included) — see Root-Guard case (a) in the
/// `vanished_tests` module.
///
/// A subtler case remains even when `root` itself resolves to *some*
/// directory: a removable/network mount that hasn't come up yet often still
/// has an empty directory sitting at its mount point (owned by whatever
/// filesystem is underneath, typically the root filesystem) — walking it
/// finds zero audio files, indistinguishable at a glance from a genuinely
/// emptied folder. Marking every track under it "unmounted" would still make
/// the whole library look empty in the UI the moment that scan lands (see
/// the `library::source::LibrarySource::reachability` contract for why
/// `Unmounted` vs `Deleted` matters — this guard is about whether ANY
/// marking should happen at all, not which reason to use once it does). So,
/// only once the walk found zero audio files AND at least one NOT-YET-
/// TOMBSTONED track (`removed_at IS NULL` — present or already-missing
/// alike, via `scanner_vanish::guard_evidence_under_root`) is recorded under
/// `root`, this function asks one more question before marking anything:
/// does ANY of those tracks' recorded `device` match the device `root`
/// itself currently resolves to? A `NULL` (`None`) recorded device never
/// counts as a match. If yes, at least one track proves `root`'s filesystem
/// really is the one previously scanned — proceed to mark normally
/// (Root-Guard case (c): a real, provable deletion). If no such evidence
/// exists, mark nothing and return [`ScanOutcome::RootUnavailable`] instead
/// (Root-Guard case (b)) — the transaction still commits whatever the
/// (empty) walk itself produced, but the mark phase never runs.
///
/// This evidence set is deliberately wider than the mark phase's own
/// `PRESENT`-only candidate list (`scanner_vanish::present_candidates_under_
/// root`): a row an earlier reconcile already flagged missing still carries
/// a recorded `device` that means exactly as much as a present row's does.
/// If the guard's evidence were narrowed to `PRESENT`, a root whose tracks
/// are ALL already flagged missing — and whose mount point then gets a
/// different filesystem swapped underneath it — would look like it has no
/// evidence at all (empty candidate list) and would never trip the guard,
/// silently reporting `Completed`/`vanished == 0` instead of surfacing
/// `RootUnavailable` — exactly the "empty library" lie this guard exists to
/// prevent. A tombstoned row (`removed_at` set) is still excluded even from
/// this wider set: it's been explicitly removed from the library and no
/// longer carries evidence about anything.
///
/// This guard is deliberately root-only: it decides whether to run the mark
/// phase over `root` at all, never which individual rows within it get
/// marked. If `root` itself is confirmed reachable but some *subfolder*
/// under it sits on its own, now-absent mount, that subtree's tracks still
/// get marked — normally, each via its own `classify_missing` call — because
/// that is an honest partial outage, not "we have no evidence".
///
/// ## Hint coexistence: a `tracks` row and an `import_errors` row can now
/// both exist for the same path
///
/// Before Task 1.8, a `tracks` row and an `import_errors` row for the same
/// `path` were mutually exclusive — any successful import cleared the error
/// row. `track_meta::read_meta_with_fallback`'s pass-2 rescue breaks that: a
/// file with unreadable tags but an intact container now gets BOTH a
/// `tracks` row (`untagged = 1`, so the collection has no hole for it) AND
/// an `import_errors` row, which becomes a HINT ("imported without
/// metadata") rather than a failure — see `import_errors.rs`'s module doc
/// comment for the exact hint contract a later query layer/sidebar badge
/// must use.
///
/// Concretely, `scanner_entry::record_hint_or_healing`, called while the walk
/// processes an entry, still clears the error after a pass-1 success
/// (unchanged — the self-healing rule sharpens, it doesn't change, for that
/// case); after a pass-2 (untagged) success it records pass 1's own `(kind,
/// detail)` instead — refreshing the hint's `last_seen`/`seen_count` rather
/// than deleting it. Only a later scan that achieves a real pass-1 success
/// (the file got re-tagged) clears it.
fn scan_folder_inner(
    source: &dyn LibrarySource,
    conn: &Connection,
    root: &Path,
    mut progress: Option<scan_progress::ScanProgressReporter<'_>>,
) -> Result<ScanOutcome, ScanError> {
    if let Some(outcome) = guard_root_before_walk(source, root) {
        return Ok(outcome);
    }
    let mut mobile_sync = mobile_sync::MobileSyncDiscovery::default();
    let mut mount_cache = mount::MountPointCache::new(source);
    let tx = conn.unchecked_transaction()?;
    let mut state = WalkState {
        report: ScanReport::default(),
        trace: WalkTrace {
            audio_files_seen: 0,
            observed_paths: HashSet::new(),
            dirs: HashSet::new(),
            failed: HashSet::new(),
        },
    };
    walk_root(
        source,
        &tx,
        root,
        &mut state,
        &mut mobile_sync,
        &mut mount_cache,
        &mut progress,
    )?;
    apply_mobile_sync(&mobile_sync, source, &tx, &mut state.report)?;
    let evidence = gather_vanish_evidence(&tx, root, state.trace)?;
    let outcome = decide_outcome(source, &tx, root, evidence, state.report)?;
    tx.commit()?;
    Ok(outcome)
}

/// Whether a completed scan changed anything a consumer's view reflects — any
/// catalog upsert/move/vanish/exclusion or an import-error row added or healed.
/// A scan that only skipped unchanged files leaves every view identical and so
/// logs no event.
fn scan_touched_library(report: &ScanReport) -> bool {
    report.added
        + report.updated
        + report.moved
        + report.vanished
        + report.excluded
        + report.healed
        + report.errors
        > 0
}

// Scan progress counting/reporting owns the counting and delivery contract;
// see `scanner_progress.rs`'s own module doc comment.
#[path = "scanner_progress.rs"]
mod scan_progress;

#[path = "scanner_entry.rs"]
mod entry;

#[path = "scanner_mobile_sync.rs"]
mod mobile_sync;

// Reconcile owns every conclusion about catalog rows the walk did not find;
// see `scanner_vanish.rs`'s own module doc comment. Not `#[cfg(test)]`: this
// is production code, always compiled.
#[path = "scanner_vanish.rs"]
mod vanish;

// Task 1.6: the mount_point memoization used above lives in its own file for
// the same 800-line reason — see `scanner_mount.rs`'s own doc comment.
#[path = "scanner_mount.rs"]
mod mount;

// Task 1.8: `TrackMeta`, the pass-1/pass-2 lofty reads, and their
// orchestration live in their own file for the same 800-line reason — see
// `scanner_meta.rs`'s own module doc comment.
#[path = "scanner_meta.rs"]
pub(crate) mod track_meta;

#[path = "scanner_repair.rs"]
mod repair;

// Task 1.8: move detection also moved out to make room for the above — see
// `scanner_move.rs`'s own module doc comment.
#[path = "scanner_move.rs"]
pub(crate) mod move_detect;

#[cfg(test)]
#[path = "scanner_progress_tests.rs"]
mod progress_tests;

// Stage 3 Task 1: the test suite moved to its own file purely to keep this
// file under the project's 800-line rule — see `scanner_tests.rs`'s module
// doc comment.
#[cfg(test)]
#[path = "scanner_tests.rs"]
mod tests;

// The scan tests that use a non-Unix `LibrarySource` live apart from the rest,
// again for the 800-line rule — see `scanner_source_tests.rs`'s module doc.
#[cfg(test)]
#[path = "scanner_source_tests.rs"]
mod source_tests;

#[cfg(test)]
#[path = "scanner_mobile_sync_path_tests.rs"]
mod mobile_sync_path_tests;

#[cfg(test)]
#[path = "scanner_metadata_persistence_tests.rs"]
mod metadata_persistence_tests;

// Task 1.7: the episode/dismiss/directory-dedup test suite lives in its own
// file, same 800-line reason as every other `_tests.rs` sibling here.
#[cfg(test)]
#[path = "scanner_import_errors_tests.rs"]
mod import_errors_tests;

#[cfg(test)]
#[path = "scanner_vanished_tests.rs"]
mod vanished_tests;

// Task 1.8: the tag-free relaxed second-pass test suite lives in its own
// file, same 800-line reason as every other `_tests.rs` sibling here.
#[cfg(test)]
#[path = "scanner_untagged_tests.rs"]
mod untagged_tests;

// Task 1.9: the tombstone-resurrect + `healed`-counter test suite lives in
// its own file, same 800-line reason as every other `_tests.rs` sibling
// here.
#[cfg(test)]
#[path = "scanner_tombstone_tests.rs"]
mod tombstone_tests;

#[cfg(test)]
#[path = "scanner_exclusion_tests.rs"]
mod exclusion_tests;
