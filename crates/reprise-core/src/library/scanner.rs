use rusqlite::Connection;
use std::path::Path;

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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScanProgress {
    Discovering,
    Scanning {
        processed: u64,
        total: u64,
        current_path: std::path::PathBuf,
    },
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
        year: tag.and_then(Accessor::year).map(|y| y as i32),
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
/// path is gone from disk, or which are already flagged `missing`. This
/// filter is applied — and candidates counted — only over this valid subset,
/// never over the raw SQL match count. That ordering matters: two DB rows
/// can share a fingerprint (duplicate tracks) while only one of their files
/// has actually disappeared; counting the raw matches would flag that as a
/// false ambiguity and refuse a move that is in fact unambiguous (see the
/// `one_deleted_one_alive_duplicate_is_still_an_unambiguous_move` test).
fn valid_candidates(rows: Vec<(i64, String, i64)>) -> Vec<MoveCandidate> {
    rows.into_iter()
        .filter(|(_, path, missing)| *missing != 0 || !Path::new(path).exists())
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
    let rows: Vec<(i64, String, i64)> = {
        let mut stmt =
            tx.prepare("SELECT id, path, missing FROM tracks WHERE device = ?1 AND inode = ?2")?;
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

    let rows: Vec<(i64, String, i64)> = {
        let mut stmt = tx.prepare(
            "SELECT id, path, missing FROM tracks WHERE title = ?1 AND artist = ?2 \
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

pub fn scan_folder(conn: &mut Connection, root: &Path) -> Result<ScanReport, ScanError> {
    scan_folder_inner(conn, root, None)
}

pub fn scan_folder_with_progress(
    conn: &mut Connection,
    root: &Path,
    mut on_progress: impl FnMut(ScanProgress),
) -> Result<ScanReport, ScanError> {
    on_progress(ScanProgress::Discovering);
    let total = count_audio_files(root);
    let reporter = ScanProgressReporter {
        callback: &mut on_progress,
        processed: 0,
        total,
    };
    scan_folder_inner(conn, root, Some(reporter))
}

fn scan_folder_inner(
    conn: &mut Connection,
    root: &Path,
    mut progress: Option<ScanProgressReporter<'_>>,
) -> Result<ScanReport, ScanError> {
    let mut report = ScanReport::default();
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
                tx.execute("DELETE FROM import_errors WHERE path = ?1", [&err_path])?;
                tx.execute(
                    "INSERT INTO import_errors (path, reason, occurred_at) VALUES (?1,?2,?3)",
                    rusqlite::params![
                        err_path,
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
        let path_str = path.to_string_lossy().to_string();
        let mtime = file_mtime(path);
        let known: Option<(i64, i64)> = tx
            .query_row(
                "SELECT file_mtime, missing FROM tracks WHERE path = ?1",
                [&path_str],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .ok();
        let known_mtime = known.map(|(file_mtime, _)| file_mtime);
        let known_missing = known.is_some_and(|(_, missing)| missing != 0);
        if known_mtime == Some(mtime) {
            if known_missing {
                // The file reappeared at its exact recorded path with an
                // unchanged mtime (NAS remount, restore-from-trash): the
                // ordinary incremental fast path would otherwise skip it
                // forever, silently ignoring `missing` — this is the one
                // case the fast path must NOT take, since the row still
                // needs `missing` cleared even though nothing else changed.
                tx.execute("UPDATE tracks SET missing = 0 WHERE path = ?1", [&path_str])?;
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
                           inode=?14, missing=0
                         WHERE id=?15",
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
                           missing, file_size, device, inode)
                         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,0,?13,?14,?15)
                         ON CONFLICT(path) DO UPDATE SET
                           title=?2, artist=?3, album=?4, album_artist=?5, year=?6,
                           track_no=?7, genre=?8, duration_ms=?9, bitrate_kbps=?10,
                           file_mtime=?12, missing=0, file_size=?13, device=?14, inode=?15",
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
                // import_errors has no UNIQUE constraint on path, so replace any
                // prior error row for this file to keep rescans from piling up
                // duplicate entries for a file that is still broken.
                // occurred_at is intentionally refreshed to the most recent failing scan.
                tx.execute("DELETE FROM import_errors WHERE path = ?1", [&path_str])?;
                tx.execute(
                    "INSERT INTO import_errors (path, reason, occurred_at) VALUES (?1,?2,?3)",
                    rusqlite::params![path_str, e.to_string(), now_unix()],
                )?;
                report.errors += 1;
            }
        }
        if let Some(progress) = &mut progress {
            progress.advance(path);
        }
    }
    tx.commit()?;
    Ok(report)
}

/// Marks `missing = 1` for every currently-not-missing track whose `path` is
/// under `root` AND whose file no longer exists on disk. Returns the count
/// of rows newly marked (an already-`missing` row is left alone and not
/// recounted, even if its file is still gone).
///
/// ## Call this AFTER `scan_folder(root)`, never before
///
/// The folder watcher (`library::watcher`) runs this immediately after an
/// incremental `scan_folder(root)` on every debounce firing, in that order,
/// deliberately: a file that was renamed/moved within `root` is reconciled
/// by `scan_folder`'s move detection first (the row's `path` column is
/// updated to the new location, history intact), so by the time this
/// function runs, a row whose recorded `path` no longer exists really was
/// deleted, not just relocated. Running this *before* the scan would
/// transiently — and wrongly — flag a moved-but-not-yet-rescanned file as
/// missing.
///
/// ## Component-wise prefix check is authoritative; SQL `LIKE` only prefilters
///
/// Membership ("under `root`") is decided by `Path::starts_with`, which
/// compares path *components*, not raw bytes: a track at `/music/foobar/
/// x.flac` does NOT count as being under `/music/foo`, which a naive
/// string/`LIKE 'foo%'` prefix check would incorrectly include. This is also
/// what keeps this function from ever touching a track outside `root` — the
/// guarantee a future multi-folder library depends on — even when that other
/// track's file has also vanished from disk; only a scan/watch of *that*
/// track's own root is ever responsible for marking it.
///
/// A SQL `LIKE '<root>/%'` prefilter (Queue-C ledger item) narrows the
/// candidate rows the watcher streams through Rust on every reconcile,
/// instead of full-scanning all non-missing tracks. The `/` before `%` and
/// the escaping of LIKE metacharacters mean the pattern already excludes
/// sibling roots (`/music` vs `/music2`), but it is deliberately only a
/// *superset* filter: `starts_with` still decides membership on every
/// surviving candidate, so the result is byte-identical to the pre-filter
/// implementation regardless of `LIKE`'s ASCII case-insensitivity or any
/// other way the pattern is wider than the component check.
pub fn mark_vanished_under_root(conn: &Connection, root: &Path) -> Result<u32, ScanError> {
    // Perf (Queue-C ledger item): narrow candidates in SQL instead of
    // streaming the whole table through Rust on every watcher reconcile.
    // The pattern is "<root>/%" with LIKE metacharacters escaped, so it can
    // never match a *sibling* root sharing a string prefix ("/music" vs
    // "/music2"). The component-wise starts_with() below remains the
    // authoritative check — the LIKE only shrinks the candidate set, it
    // never decides membership, so semantics are byte-identical to the
    // pre-filter implementation (including exotic-UTF-8 and trailing-slash
    // edges).
    let root_str = root.to_string_lossy();
    let pattern = format!(
        "{}/%",
        crate::library::playlists::escape_like(root_str.trim_end_matches('/'))
    );
    let candidates: Vec<(i64, String)> = {
        let mut stmt = conn.prepare(
            "SELECT id, path FROM tracks WHERE missing = 0 AND path LIKE ?1 ESCAPE '\\'",
        )?;
        let rows = stmt
            .query_map(rusqlite::params![pattern], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<Result<_, _>>()?;
        rows
    };

    let mut marked = 0u32;
    for (id, path_str) in candidates {
        let path = Path::new(&path_str);
        if !path.starts_with(root) || path.exists() {
            continue;
        }
        conn.execute(
            "UPDATE tracks SET missing = 1 WHERE id = ?1",
            rusqlite::params![id],
        )?;
        marked += 1;
        tracing::info!(path = %path_str, "watcher: marked vanished track missing");
    }
    Ok(marked)
}

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
