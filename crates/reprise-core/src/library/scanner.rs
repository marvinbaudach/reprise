use rusqlite::Connection;
use std::path::Path;

use super::mounts;
use crate::models::MissingReason;
use crate::queries::PRESENT;

#[derive(Debug, thiserror::Error)]
pub enum ScanError {
    #[error("database error: {0}")]
    Db(#[from] crate::db::DbError),
    #[error("Sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("unreadable tags: {0}")]
    Tags(String),
    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Default)]
pub struct ScanReport {
    pub added: u32,
    pub updated: u32,
    pub skipped_unchanged: u32,
    pub errors: u32,
    /// Stage 2 Task 8: files recognized as relocated (same `(device, inode)`
    /// or, failing that, an unambiguous tag+size fingerprint match against a
    /// row whose old path is gone) rather than treated as new. A moved file
    /// counts here, not in `added`.
    pub moved: u32,
    /// Task 1.5: count of previously-present tracks under this scan's root
    /// newly marked missing by this same scan's folded-in reconcile pass —
    /// see the module's `## Fold: scan IS reconcile` doc section. An
    /// already-missing row is not recounted. Always `0` when the scan
    /// returns [`ScanOutcome::RootUnavailable`] instead of wrapping this
    /// report in [`ScanOutcome::Completed`], since that outcome means the
    /// mark phase never ran at all.
    pub vanished: u32,
}

/// What a `scan_folder`/`scan_folder_with_progress` call concluded — Task
/// 1.5 replaced the bare `ScanReport` return with this two-variant outcome
/// so a scan can distinguish "I walked `root` and reconciled it" from "I
/// have no evidence about `root` at all" without silently reporting the
/// latter as a suspiciously-empty former. See the module's `## Root guard`
/// doc section on [`scan_folder_inner`] for exactly when [`RootUnavailable`]
/// fires and why marking nothing beats marking every track "unmounted".
///
/// [`RootUnavailable`]: ScanOutcome::RootUnavailable
#[derive(Debug)]
pub enum ScanOutcome {
    /// The walk ran (even if it found nothing) and, unless the root guard
    /// tripped, the vanish-mark phase ran too, in the same transaction as
    /// the walk's own upserts.
    Completed(ScanReport),
    /// Nothing was written — not even an "unmounted" mark — because the
    /// root guard tripped: see [`scan_folder_inner`]'s doc comment.
    RootUnavailable { root: std::path::PathBuf },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScanProgress {
    Discovering,
    Scanning {
        processed: u64,
        total: u64,
        current_path: std::path::PathBuf,
    },
    Fetching {
        done: u64,
        total: u64,
    },
}

/// Summary passed to the UI after a scan finishes, for the completion toast.
#[derive(Debug, Clone, Copy)]
pub struct ScanResult {
    pub new_tracks: u32,
    pub failed: u32,
}

impl ScanReport {
    pub fn to_scan_result(&self) -> ScanResult {
        ScanResult {
            new_tracks: self.added,
            failed: self.errors,
        }
    }
}

#[derive(Debug, Default)]
pub struct TrackMeta {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub album_artist: String,
    pub year: Option<i32>,
    pub track_no: Option<i32>,
    pub genre: String,
    pub duration_ms: i64,
    pub bitrate_kbps: Option<i32>,
}

const AUDIO_EXTENSIONS: [&str; 7] = ["mp3", "flac", "ogg", "opus", "m4a", "aac", "wav"];

fn is_audio_file(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .is_some_and(|extension| AUDIO_EXTENSIONS.contains(&extension.as_str()))
}

fn count_audio_files(root: &Path) -> u64 {
    walkdir::WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file() && is_audio_file(entry.path()))
        .count() as u64
}

struct ScanProgressReporter<'a> {
    callback: &'a mut dyn FnMut(ScanProgress),
    processed: u64,
    total: u64,
}

impl ScanProgressReporter<'_> {
    fn advance(&mut self, path: &Path) {
        self.processed += 1;
        self.total = self.total.max(self.processed);
        (self.callback)(ScanProgress::Scanning {
            processed: self.processed,
            total: self.total,
            current_path: path.to_path_buf(),
        });
    }
}

pub fn read_meta(path: &Path) -> Result<TrackMeta, ScanError> {
    use lofty::prelude::*;
    let tagged = lofty::read_from_path(path).map_err(|e| ScanError::Tags(e.to_string()))?;
    let props = tagged.properties();
    let tag = tagged.primary_tag().or_else(|| tagged.first_tag());
    let get = |f: &dyn Fn(&lofty::tag::Tag) -> Option<String>| tag.and_then(f).unwrap_or_default();
    Ok(TrackMeta {
        title: get(&|t| t.title().map(|s| s.to_string())),
        artist: get(&|t| t.artist().map(|s| s.to_string())),
        album: get(&|t| t.album().map(|s| s.to_string())),
        album_artist: get(&|t| {
            t.get_string(&lofty::tag::ItemKey::AlbumArtist)
                .map(std::string::ToString::to_string)
        }),
        year: tag
            .and_then(Accessor::year)
            .or_else(|| tagged.tags().iter().find_map(Accessor::year))
            .map(|y| y as i32),
        track_no: tag.and_then(Accessor::track).map(|n| n as i32),
        genre: get(&|t| t.genre().map(|s| s.to_string())),
        duration_ms: props.duration().as_millis() as i64,
        bitrate_kbps: props.audio_bitrate().map(|b| b as i32),
    })
}

fn file_mtime(path: &Path) -> i64 {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs() as i64)
}

/// `(file_size, device, inode)` for the move-detection fingerprint. Linux-
/// only (`std::os::unix::fs::MetadataExt`), matching the rest of this
/// codebase's Linux-only scope. Returns `None` if `stat` fails (e.g. a race
/// where the file vanished between `walkdir` listing it and this call) —
/// Stage 3 Task 1: a file that can't be stat'd has no reliable filesystem
/// identity to fingerprint, so `scan_folder` skips the move-detection step
/// entirely for it (rather than the pre-Task-1 behavior of silently
/// fingerprinting on placeholder zeros, which could have coincidentally
/// matched an unrelated `(device, inode)` of `(0, 0)`) and stores `NULL`
/// device/inode for the row, same as any pre-Stage-2 row that predates these
/// columns. `file_size` still defaults to `0` in that case — unlike device/
/// inode it is `NOT NULL DEFAULT 0` in the schema, matching every other
/// tag-derived column's non-null convention, so `0` (rather than `NULL`) is
/// the only representable "unknown" value for it anyway.
fn file_stat(path: &Path) -> Option<(u64, u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(path)
        .ok()
        .map(|m| (m.size(), m.dev(), m.ino()))
}

/// A DB row that is a *candidate* to be the pre-move identity of a file at
/// an unknown path: `id`/`path` to perform the move `UPDATE` against.
#[derive(Debug, PartialEq)]
struct MoveCandidate {
    id: i64,
    path: String,
}

/// Everything `find_move_candidate` needs to know about the file it's
/// looking for a pre-move identity of. Bundled into one struct (rather than
/// seven positional arguments) purely to stay under clippy's
/// `too_many_arguments` lint.
struct MoveLookup<'a> {
    device: i64,
    inode: i64,
    title: &'a str,
    artist: &'a str,
    album: &'a str,
    duration_ms: i64,
    file_size: i64,
}

/// Return type for tag_param_values: (title, artist, album, album_artist,
/// year, track_no, genre, duration_ms, bitrate_kbps).
type TagParams<'a> = (
    &'a str,
    &'a str,
    &'a str,
    &'a str,
    Option<i32>,
    Option<i32>,
    &'a str,
    i64,
    Option<i32>,
);

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
fn find_move_candidate(
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

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// Extracts tag-derived column values in the canonical order used by both
/// move-UPDATE and INSERT/upsert statements: title, artist, album, album_artist,
/// year, track_no, genre, duration_ms, bitrate_kbps. Having a single source
/// for this ordering ensures that adding/removing columns is a one-place change.
fn tag_param_values<'a>(title: &'a str, meta: &'a TrackMeta) -> TagParams<'a> {
    (
        title,
        &meta.artist,
        &meta.album,
        &meta.album_artist,
        meta.year,
        meta.track_no,
        &meta.genre,
        meta.duration_ms,
        meta.bitrate_kbps,
    )
}

pub fn scan_folder(conn: &mut Connection, root: &Path) -> Result<ScanOutcome, ScanError> {
    scan_folder_inner(conn, root, None)
}

pub fn scan_folder_with_progress(
    conn: &mut Connection,
    root: &Path,
    mut on_progress: impl FnMut(ScanProgress),
) -> Result<ScanOutcome, ScanError> {
    on_progress(ScanProgress::Discovering);
    let total = count_audio_files(root);
    let reporter = ScanProgressReporter {
        callback: &mut on_progress,
        processed: 0,
        total,
    };
    scan_folder_inner(conn, root, Some(reporter))
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
/// the walk even starts, `!root.exists()` short-circuits straight to
/// [`ScanOutcome::RootUnavailable`] with no walk and no database write at
/// all (`import_errors` included) — see Root-Guard case (a) in this
/// function's test suite.
///
/// A subtler case remains even when `root` itself resolves to *some*
/// directory: a removable/network mount that hasn't come up yet often still
/// has an empty directory sitting at its mount point (owned by whatever
/// filesystem is underneath, typically the root filesystem) — walking it
/// finds zero audio files, indistinguishable at a glance from a genuinely
/// emptied folder. Marking every track under it "unmounted" would still make
/// the whole library look empty in the UI the moment that scan lands (see
/// this module's `library::mounts::classify_missing`'s own doc comment for
/// why `Unmounted` vs `Deleted` matters — this guard is about whether ANY
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
fn scan_folder_inner(
    conn: &mut Connection,
    root: &Path,
    mut progress: Option<ScanProgressReporter<'_>>,
) -> Result<ScanOutcome, ScanError> {
    debug_assert!(
        root.is_absolute(),
        "scan roots must be absolute paths — library roots (GTK folder chooser, \
         persisted settings) are always absolute in this codebase, and mounts::\
         nearest_existing_ancestor_dev's walk-to-`/` guarantee assumes it"
    );
    if !root.exists() {
        // Root-Guard case (a): no walk, no database write at all — see this
        // function's `## Root guard` doc section.
        tracing::warn!(
            root = %root.display(),
            "scan: root does not exist; reporting RootUnavailable without touching the database"
        );
        return Ok(ScanOutcome::RootUnavailable {
            root: root.to_path_buf(),
        });
    }

    let mut report = ScanReport::default();
    let mut audio_files_seen: u64 = 0;
    let mut mount_cache = mount::MountPointCache::new();
    let tx = conn.transaction()?;
    for entry in walkdir::WalkDir::new(root).follow_links(false).into_iter() {
        let entry = match entry {
            Ok(entry) => entry,
            Err(err) => {
                // Directory traversal errors (permission denied, broken symlinks, ...)
                // must be recorded, not dropped, per the fault-tolerance principle.
                let err_path = err.path().map_or_else(
                    || root.to_string_lossy().to_string(),
                    |p| p.to_string_lossy().to_string(),
                );
                // Schema v10 (Task 1.1) rebuilt `import_errors` with `path`
                // as its primary key and typed `reason_kind`/`reason_detail`
                // columns; the DELETE-then-INSERT pair below (rather than an
                // upsert) keeps a fresh `first_seen`/`last_seen` pair on
                // every failing scan, matching this path's pre-v10 behavior
                // of always refreshing `occurred_at`. `reason_kind` typing
                // beyond this single "io" bucket is self-healing-list work
                // for a later task.
                tx.execute("DELETE FROM import_errors WHERE path = ?1", [&err_path])?;
                tx.execute(
                    "INSERT INTO import_errors \
                     (path, reason_kind, reason_detail, first_seen, last_seen) \
                     VALUES (?1,?2,?3,?4,?4)",
                    rusqlite::params![
                        err_path,
                        "io",
                        format!("directory traversal error: {err}"),
                        now_unix()
                    ],
                )?;
                report.errors += 1;
                continue;
            }
        };
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        if !is_audio_file(path) {
            continue;
        }
        // Root-Guard input: "did the walk find any audio file at all under
        // `root`?" — counted regardless of whether this particular file
        // goes on to be added/updated/skipped/errored below. See this
        // function's `## Root guard` doc section.
        audio_files_seen += 1;
        let path_str = path.to_string_lossy().to_string();
        let mtime = file_mtime(path);
        let known: Option<(i64, Option<i64>)> = tx
            .query_row(
                "SELECT file_mtime, missing_since FROM tracks WHERE path = ?1",
                [&path_str],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .ok();
        let known_mtime = known.map(|(file_mtime, _)| file_mtime);
        let known_missing = known.is_some_and(|(_, missing_since)| missing_since.is_some());
        if known_mtime == Some(mtime) {
            if known_missing {
                // The file reappeared at its exact recorded path with an
                // unchanged mtime (NAS remount, restore-from-trash): the
                // ordinary incremental fast path would otherwise skip it
                // forever, silently ignoring `missing_since` — this is the
                // one case the fast path must NOT take, since the row still
                // needs `missing_since` cleared even though nothing else
                // changed. This is also the ONLY chance a row whose
                // `mount_point` is NULL (a pre-schema-v10 row, or any row
                // that was never re-scanned since) has to acquire one
                // without its file actually changing — see
                // `scanner_mount.rs`'s module doc comment.
                let mount_point = mount_cache.resolve(path);
                tx.execute(
                    "UPDATE tracks SET missing_since = NULL, missing_reason = NULL, \
                     mount_point = ?2 WHERE path = ?1",
                    rusqlite::params![path_str, mount_point],
                )?;
                tx.execute("DELETE FROM import_errors WHERE path = ?1", [&path_str])?;
                report.updated += 1;
                tracing::info!(path = %path_str, "restored missing track");
            } else {
                report.skipped_unchanged += 1;
            }
            if let Some(progress) = &mut progress {
                progress.advance(path);
            }
            continue;
        }
        match read_meta(path) {
            Ok(meta) => {
                let is_update = known_mtime.is_some();
                // Stage 3 Task 1: `file_stat` returns `None` on a `stat`
                // failure (e.g. the file vanished in the race window between
                // `walkdir` listing it and here) — `file_size` still
                // defaults to `0` (the schema's `NOT NULL DEFAULT 0`), but
                // `device`/`inode` become `NULL` rather than a fabricated
                // `(0, 0)` that could coincidentally fingerprint-match an
                // unrelated row.
                let stat = file_stat(path);
                let (file_size, device, inode): (i64, Option<i64>, Option<i64>) = match stat {
                    Some((size, dev, ino)) => (size as i64, Some(dev as i64), Some(ino as i64)),
                    None => (0, None, None),
                };
                let title = if meta.title.is_empty() {
                    path.file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("")
                        .to_string()
                } else {
                    meta.title.clone()
                };
                // Task 1.6: recorded now, while still reachable, and
                // memoized per parent dir — see `scanner_mount.rs`.
                let mount_point = mount_cache.resolve(path);
                // Clear any previous failure for this path: a file that errored
                // once and is now readable again must not stay in the error log.
                tx.execute("DELETE FROM import_errors WHERE path = ?1", [&path_str])?;

                // Move detection (Stage 2 Task 8) only ever applies to a path
                // the DB has never seen before — a file whose path is already
                // known just falls through to the ordinary upsert below, even
                // if its content changed.
                // Skip move detection entirely when `stat` failed above (no
                // reliable device/inode to key step 1 off, and step 2's
                // fingerprint would be matching against a placeholder
                // `file_size` of 0) — see `file_stat`'s doc comment.
                let move_candidate = if is_update {
                    None
                } else {
                    match (device, inode) {
                        (Some(device), Some(inode)) => find_move_candidate(
                            &tx,
                            &MoveLookup {
                                device,
                                inode,
                                title: &title,
                                artist: &meta.artist,
                                album: &meta.album,
                                duration_ms: meta.duration_ms,
                                file_size,
                            },
                        )?,
                        _ => None,
                    }
                };

                if let Some(candidate) = move_candidate {
                    // A move: refresh path/tags/filesystem-identity on the
                    // existing row by id. rating/play_count/added_at/
                    // last_played_at are deliberately absent from this SET
                    // clause — that's the whole point of move detection.
                    let (
                        title_p,
                        artist_p,
                        album_p,
                        album_artist_p,
                        year_p,
                        track_no_p,
                        genre_p,
                        duration_ms_p,
                        bitrate_kbps_p,
                    ) = tag_param_values(&title, &meta);
                    tx.execute(
                        "UPDATE tracks SET path=?1, title=?2, artist=?3, album=?4,
                           album_artist=?5, year=?6, track_no=?7, genre=?8, duration_ms=?9,
                           bitrate_kbps=?10, file_mtime=?11, file_size=?12, device=?13,
                           inode=?14, mount_point=?15, missing_since=NULL, missing_reason=NULL
                         WHERE id=?16",
                        rusqlite::params![
                            path_str,
                            title_p,
                            artist_p,
                            album_p,
                            album_artist_p,
                            year_p,
                            track_no_p,
                            genre_p,
                            duration_ms_p,
                            bitrate_kbps_p,
                            mtime,
                            file_size,
                            device,
                            inode,
                            mount_point,
                            candidate.id,
                        ],
                    )?;
                    // Clear a stale import_errors row under the old path too
                    // (e.g. the old location briefly failed to read before
                    // being moved away) — the new path was already cleared
                    // above.
                    tx.execute(
                        "DELETE FROM import_errors WHERE path = ?1",
                        [&candidate.path],
                    )?;
                    report.moved += 1;
                } else {
                    let (
                        title_p,
                        artist_p,
                        album_p,
                        album_artist_p,
                        year_p,
                        track_no_p,
                        genre_p,
                        duration_ms_p,
                        bitrate_kbps_p,
                    ) = tag_param_values(&title, &meta);
                    tx.execute(
                        "INSERT INTO tracks (path, title, artist, album, album_artist, year,
                           track_no, genre, duration_ms, bitrate_kbps, added_at, file_mtime,
                           file_size, device, inode, mount_point)
                         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16)
                         ON CONFLICT(path) DO UPDATE SET
                           title=?2, artist=?3, album=?4, album_artist=?5, year=?6,
                           track_no=?7, genre=?8, duration_ms=?9, bitrate_kbps=?10,
                           file_mtime=?12, missing_since=NULL, missing_reason=NULL,
                           file_size=?13, device=?14, inode=?15, mount_point=?16",
                        rusqlite::params![
                            path_str,
                            title_p,
                            artist_p,
                            album_p,
                            album_artist_p,
                            year_p,
                            track_no_p,
                            genre_p,
                            duration_ms_p,
                            bitrate_kbps_p,
                            now_unix(),
                            mtime,
                            file_size,
                            device,
                            inode,
                            mount_point,
                        ],
                    )?;
                    if is_update {
                        report.updated += 1;
                    } else {
                        report.added += 1;
                    }
                }
            }
            Err(e) => {
                // `path` became `import_errors`'s primary key in schema v10
                // (Task 1.1), so a plain re-INSERT would now violate that
                // constraint on a second failing scan of the same file;
                // explicitly deleting first (rather than an upsert) keeps
                // this symmetric with the traversal-error site above and
                // refreshes `first_seen`/`last_seen` to the most recent
                // failing scan, matching this path's pre-v10 `occurred_at`
                // behavior. `reason_kind` typing beyond this single "tag"
                // bucket is self-healing-list work for a later task.
                tx.execute("DELETE FROM import_errors WHERE path = ?1", [&path_str])?;
                tx.execute(
                    "INSERT INTO import_errors \
                     (path, reason_kind, reason_detail, first_seen, last_seen) \
                     VALUES (?1,?2,?3,?4,?4)",
                    rusqlite::params![path_str, "tag", e.to_string(), now_unix()],
                )?;
                report.errors += 1;
            }
        }
        if let Some(progress) = &mut progress {
            progress.advance(path);
        }
    }

    // `candidates` (`PRESENT`-only) feeds the mark phase below regardless of
    // outcome. The guard's own evidence, `guard_evidence` (the wider
    // `removed_at IS NULL` list — see `scanner_vanish::guard_evidence_under_
    // root`'s doc comment for why it must NOT be `candidates`), is only
    // queried when the walk found nothing, the same short-circuit
    // `root_unavailable` used before this was split into two lists — so a
    // scan that actually found audio files never pays for the extra query.
    let candidates = vanish::present_candidates_under_root(&tx, root)?;
    let guard_evidence = if audio_files_seen == 0 {
        Some(vanish::guard_evidence_under_root(&tx, root)?)
    } else {
        None
    };
    let root_unavailable = guard_evidence.as_ref().is_some_and(|evidence| {
        !evidence.is_empty() && !vanish::any_candidate_confirms_root_device(evidence, root)
    });

    let outcome = if root_unavailable {
        // Root-Guard case (b): see this function's `## Root guard` doc
        // section. The upserts the walk itself produced (normally none,
        // since `audio_files_seen == 0`, but a traversal error is still
        // possible) still commit below — only the mark phase is skipped.
        tracing::warn!(
            root = %root.display(),
            candidate_count = guard_evidence.map_or(0, |e| e.len()),
            "scan: walk found no audio files and no known track under root confirms the \
             root's current device; reporting RootUnavailable instead of marking tracks missing"
        );
        ScanOutcome::RootUnavailable {
            root: root.to_path_buf(),
        }
    } else {
        report.vanished = vanish::mark_vanished(&tx, candidates)?;
        ScanOutcome::Completed(report)
    };
    tx.commit()?;
    Ok(outcome)
}

// Task 1.5: the vanish-mark phase `scan_folder_inner` folds in above lives in
// its own file purely to keep this one under the project's 800-line rule —
// see `scanner_vanish.rs`'s own module doc comment. Not `#[cfg(test)]`: this
// is production code, always compiled.
#[path = "scanner_vanish.rs"]
mod vanish;

// Task 1.6: the mount_point memoization used above lives in its own file for
// the same 800-line reason — see `scanner_mount.rs`'s own doc comment.
#[path = "scanner_mount.rs"]
mod mount;

#[cfg(test)]
#[path = "scanner_progress_tests.rs"]
mod progress_tests;

// Stage 3 Task 1: the test suite moved to its own file purely to keep this
// file under the project's 800-line rule — see `scanner_tests.rs`'s module
// doc comment.
#[cfg(test)]
#[path = "scanner_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "scanner_vanished_tests.rs"]
mod vanished_tests;
