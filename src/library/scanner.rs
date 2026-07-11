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
                .map(|s| s.to_string())
        }),
        year: tag.and_then(|t| t.year()).map(|y| y as i32),
        track_no: tag.and_then(|t| t.track()).map(|n| n as i32),
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
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// `(file_size, device, inode)` for the move-detection fingerprint. Linux-
/// only (`std::os::unix::fs::MetadataExt`), matching the rest of this
/// codebase's Linux-only scope. Falls back to `(0, 0, 0)` if `stat` fails
/// (e.g. a race where the file vanished between `walkdir` listing it and
/// this call) — a file that can't be stat'd also can't be usefully matched
/// as a move candidate, so zeros are harmless placeholders here.
fn file_stat(path: &Path) -> (i64, i64, i64) {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(path)
        .map(|m| (m.size() as i64, m.dev() as i64, m.ino() as i64))
        .unwrap_or((0, 0, 0))
}

/// A DB row that is a *candidate* to be the pre-move identity of a file at
/// an unknown path: `id`/`path` to perform the move `UPDATE` against.
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
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub fn scan_folder(conn: &mut Connection, root: &Path) -> Result<ScanReport, ScanError> {
    let mut report = ScanReport::default();
    let tx = conn.transaction()?;
    for entry in walkdir::WalkDir::new(root).follow_links(false).into_iter() {
        let entry = match entry {
            Ok(entry) => entry,
            Err(err) => {
                // Directory traversal errors (permission denied, broken symlinks, ...)
                // must be recorded, not dropped, per the fault-tolerance principle.
                let err_path = err
                    .path()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|| root.to_string_lossy().to_string());
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
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase());
        let Some(ext) = ext else { continue };
        if !AUDIO_EXTENSIONS.contains(&ext.as_str()) {
            continue;
        }
        let path_str = path.to_string_lossy().to_string();
        let mtime = file_mtime(path);
        let known_mtime: Option<i64> = tx
            .query_row(
                "SELECT file_mtime FROM tracks WHERE path = ?1",
                [&path_str],
                |r| r.get(0),
            )
            .ok();
        if known_mtime == Some(mtime) {
            report.skipped_unchanged += 1;
            continue;
        }
        match read_meta(path) {
            Ok(meta) => {
                let is_update = known_mtime.is_some();
                let (file_size, device, inode) = file_stat(path);
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
                let move_candidate = if is_update {
                    None
                } else {
                    find_move_candidate(
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
                    )?
                };

                if let Some(candidate) = move_candidate {
                    // A move: refresh path/tags/filesystem-identity on the
                    // existing row by id. rating/play_count/added_at/
                    // last_played_at are deliberately absent from this SET
                    // clause — that's the whole point of move detection.
                    tx.execute(
                        "UPDATE tracks SET path=?1, title=?2, artist=?3, album=?4,
                           album_artist=?5, year=?6, track_no=?7, genre=?8, duration_ms=?9,
                           bitrate_kbps=?10, file_mtime=?11, file_size=?12, device=?13,
                           inode=?14, missing=0
                         WHERE id=?15",
                        rusqlite::params![
                            path_str,
                            title,
                            meta.artist,
                            meta.album,
                            meta.album_artist,
                            meta.year,
                            meta.track_no,
                            meta.genre,
                            meta.duration_ms,
                            meta.bitrate_kbps,
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
                            title,
                            meta.artist,
                            meta.album,
                            meta.album_artist,
                            meta.year,
                            meta.track_no,
                            meta.genre,
                            meta.duration_ms,
                            meta.bitrate_kbps,
                            now_unix(),
                            mtime,
                            file_size,
                            device,
                            inode,
                        ],
                    )?;
                    if is_update {
                        report.updated += 1
                    } else {
                        report.added += 1
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
    }
    tx.commit()?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lofty::prelude::*;
    use lofty::tag::{Tag, TagType};

    fn fixture_copy(dir: &std::path::Path, name: &str) -> std::path::PathBuf {
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sine.flac");
        let dst = dir.join(name);
        std::fs::copy(&src, &dst).unwrap();
        dst
    }

    /// Writes identical title/artist/album tags to a fixture file so two
    /// separate copies produce the same fingerprint (title+artist+album+
    /// duration+file_size) that move detection's step 2 matches on.
    fn tag_file(path: &std::path::Path, title: &str, artist: &str, album: &str) {
        let mut tag = Tag::new(TagType::VorbisComments);
        tag.set_title(title.into());
        tag.set_artist(artist.into());
        tag.set_album(album.into());
        tag.save_to_path(path, lofty::config::WriteOptions::default())
            .unwrap();
    }

    fn row_by_path(conn: &Connection, path: &std::path::Path) -> (i64, i64, i64, i64) {
        // (id, rating, play_count, added_at)
        conn.query_row(
            "SELECT id, rating, play_count, added_at FROM tracks WHERE path = ?1",
            [path.to_string_lossy().to_string()],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap()
    }

    fn row_count(conn: &Connection) -> i64 {
        conn.query_row("SELECT count(*) FROM tracks", [], |r| r.get(0))
            .unwrap()
    }

    /// `std::fs::rename` on the same filesystem keeps the file's inode (and
    /// device) identical while its path becomes unknown to the DB — this
    /// exercises move detection's step 1 (device/inode candidate). Note:
    /// rename does NOT change mtime, but that's irrelevant here — the new
    /// path has no DB row at all, so it enters the unknown-path flow
    /// regardless of mtime, exactly like a genuinely new file would.
    #[test]
    fn move_via_rename_preserves_metadata() {
        let tmp = tempfile::tempdir().unwrap();
        let old_path = fixture_copy(tmp.path(), "track.flac");
        tag_file(&old_path, "Moved Song", "Some Artist", "Some Album");

        let mut conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();
        let r1 = scan_folder(&mut conn, tmp.path()).unwrap();
        assert_eq!(r1.added, 1);

        conn.execute(
            "UPDATE tracks SET rating = 5, play_count = 7 WHERE path = ?1",
            [old_path.to_string_lossy().to_string()],
        )
        .unwrap();
        let (old_id, _, _, added_at_before) = row_by_path(&conn, &old_path);

        let new_dir = tmp.path().join("new_subdir");
        std::fs::create_dir(&new_dir).unwrap();
        let new_path = new_dir.join("track.flac");
        std::fs::rename(&old_path, &new_path).unwrap();

        let r2 = scan_folder(&mut conn, tmp.path()).unwrap();
        assert_eq!(r2.moved, 1);
        assert_eq!(r2.added, 0);
        assert_eq!(row_count(&conn), 1);

        let (new_id, rating, play_count, added_at_after) = row_by_path(&conn, &new_path);
        assert_eq!(new_id, old_id);
        assert_eq!(rating, 5);
        assert_eq!(play_count, 7);
        assert_eq!(added_at_after, added_at_before);
    }

    /// Copy + delete-original simulates a cross-filesystem move: the copy
    /// gets a brand-new inode, so step 1 (device/inode) cannot match and move
    /// detection must fall through to step 2 (tag+duration+size fingerprint).
    /// Both copies share identical tags and byte-for-byte content (same
    /// fixture file), so size and duration line up exactly; only the
    /// original's deletion makes its DB row a valid ("path gone") candidate.
    #[test]
    fn move_via_copy_delete_preserves_metadata() {
        let tmp = tempfile::tempdir().unwrap();
        let old_path = fixture_copy(tmp.path(), "track.flac");
        tag_file(&old_path, "Copied Song", "Some Artist", "Some Album");

        let mut conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();
        let r1 = scan_folder(&mut conn, tmp.path()).unwrap();
        assert_eq!(r1.added, 1);

        conn.execute(
            "UPDATE tracks SET rating = 5, play_count = 7 WHERE path = ?1",
            [old_path.to_string_lossy().to_string()],
        )
        .unwrap();
        let (old_id, _, _, added_at_before) = row_by_path(&conn, &old_path);

        let new_dir = tmp.path().join("new_subdir");
        std::fs::create_dir(&new_dir).unwrap();
        let new_path = new_dir.join("track.flac");
        std::fs::copy(&old_path, &new_path).unwrap();
        std::fs::remove_file(&old_path).unwrap();

        let r2 = scan_folder(&mut conn, tmp.path()).unwrap();
        assert_eq!(r2.moved, 1);
        assert_eq!(r2.added, 0);
        assert_eq!(row_count(&conn), 1);

        let (new_id, rating, play_count, added_at_after) = row_by_path(&conn, &new_path);
        assert_eq!(new_id, old_id);
        assert_eq!(rating, 5);
        assert_eq!(play_count, 7);
        assert_eq!(added_at_after, added_at_before);
    }

    /// Two identical copies (same fingerprint) both get deleted, then a
    /// single new copy appears at a new location. Both stale rows'
    /// paths are gone, so both are valid fingerprint candidates — move
    /// detection must refuse to guess between them (logs a warning) and
    /// fall back to a normal insert instead of silently attaching the new
    /// file's history to an arbitrary one of the two old rows.
    #[test]
    fn ambiguous_duplicates_are_not_guessed() {
        let tmp = tempfile::tempdir().unwrap();
        let path_a = fixture_copy(tmp.path(), "a.flac");
        tag_file(&path_a, "Duplicate Song", "Some Artist", "Some Album");
        let path_b = fixture_copy(tmp.path(), "b.flac");
        tag_file(&path_b, "Duplicate Song", "Some Artist", "Some Album");

        let mut conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();
        let r1 = scan_folder(&mut conn, tmp.path()).unwrap();
        assert_eq!(r1.added, 2);
        assert_eq!(row_count(&conn), 2);

        std::fs::remove_file(&path_a).unwrap();
        std::fs::remove_file(&path_b).unwrap();
        let new_dir = tmp.path().join("new_subdir");
        std::fs::create_dir(&new_dir).unwrap();
        let new_path = new_dir.join("c.flac");
        std::fs::copy(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sine.flac"),
            &new_path,
        )
        .unwrap();
        tag_file(&new_path, "Duplicate Song", "Some Artist", "Some Album");

        let r2 = scan_folder(&mut conn, tmp.path()).unwrap();
        assert_eq!(r2.moved, 0);
        assert_eq!(r2.added, 1);
        // Both stale rows remain untouched (missing-marking is a Stage 3/
        // watcher concern, out of scope here) plus the one newly added row.
        assert_eq!(row_count(&conn), 3);
    }

    /// Pins the candidate filter's order of operations: the "path gone or
    /// missing" validity check must run BEFORE candidates are counted for
    /// ambiguity, not after. Two identical copies exist; only ONE is
    /// deleted. A new copy shows up at a new location. Naively counting all
    /// fingerprint-matching rows first would see 2 candidates and refuse to
    /// guess — but the survivor's path still exists on disk, so it is never
    /// a *valid* candidate, leaving exactly one true match (the deleted
    /// row), and the move must proceed.
    #[test]
    fn one_deleted_one_alive_duplicate_is_still_an_unambiguous_move() {
        let tmp = tempfile::tempdir().unwrap();
        let path_a = fixture_copy(tmp.path(), "a.flac");
        tag_file(&path_a, "Half Deleted Song", "Some Artist", "Some Album");
        let path_b = fixture_copy(tmp.path(), "b.flac");
        tag_file(&path_b, "Half Deleted Song", "Some Artist", "Some Album");

        let mut conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();
        let r1 = scan_folder(&mut conn, tmp.path()).unwrap();
        assert_eq!(r1.added, 2);

        conn.execute(
            "UPDATE tracks SET rating = 3, play_count = 2 WHERE path = ?1",
            [path_b.to_string_lossy().to_string()],
        )
        .unwrap();
        let (survivor_id, survivor_rating, survivor_play_count, survivor_added_at) =
            row_by_path(&conn, &path_b);

        // Only path_a is deleted; path_b (the survivor) is left in place.
        std::fs::remove_file(&path_a).unwrap();
        let new_dir = tmp.path().join("new_subdir");
        std::fs::create_dir(&new_dir).unwrap();
        let new_path = new_dir.join("c.flac");
        std::fs::copy(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sine.flac"),
            &new_path,
        )
        .unwrap();
        tag_file(&new_path, "Half Deleted Song", "Some Artist", "Some Album");

        let r2 = scan_folder(&mut conn, tmp.path()).unwrap();
        assert_eq!(r2.moved, 1);
        assert_eq!(r2.added, 0);
        assert_eq!(row_count(&conn), 2);

        // The survivor (path_b) must be completely untouched.
        let (still_survivor_id, still_rating, still_play_count, still_added_at) =
            row_by_path(&conn, &path_b);
        assert_eq!(still_survivor_id, survivor_id);
        assert_eq!(still_rating, survivor_rating);
        assert_eq!(still_play_count, survivor_play_count);
        assert_eq!(still_added_at, survivor_added_at);
    }

    /// A rescan over an untouched library must not match anything as moved —
    /// pins that move detection only ever engages for genuinely unknown
    /// paths, never for files whose path/mtime already match a known row.
    #[test]
    fn unchanged_files_are_not_matched_as_moves() {
        let tmp = tempfile::tempdir().unwrap();
        fixture_copy(tmp.path(), "a.flac");
        fixture_copy(tmp.path(), "b.flac");

        let mut conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();
        let r1 = scan_folder(&mut conn, tmp.path()).unwrap();
        assert_eq!(r1.added, 2);

        let r2 = scan_folder(&mut conn, tmp.path()).unwrap();
        assert_eq!(r2.moved, 0);
        assert_eq!(r2.skipped_unchanged, 2);
        assert_eq!(r2.added, 0);
    }

    /// Writes tags to a fixture COPY (never the original) and reads them back with
    /// read_meta — the roundtrip from the spec.
    #[test]
    fn read_meta_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let file = fixture_copy(tmp.path(), "tagged.flac");
        let mut tag = Tag::new(TagType::VorbisComments);
        tag.set_title("Beast of Darkness".into());
        tag.set_artist("Brand of Sacrifice".into());
        tag.set_album("God Hand".into());
        tag.set_year(2019);
        tag.set_track(9);
        tag.set_genre("Deathcore".into());
        tag.save_to_path(&file, lofty::config::WriteOptions::default())
            .unwrap();

        let meta = read_meta(&file).unwrap();
        assert_eq!(meta.title, "Beast of Darkness");
        assert_eq!(meta.artist, "Brand of Sacrifice");
        assert_eq!(meta.album, "God Hand");
        assert_eq!(meta.year, Some(2019));
        assert_eq!(meta.track_no, Some(9));
        assert!(meta.duration_ms > 0);
    }

    #[test]
    fn scan_adds_updates_and_reports_errors() {
        let tmp = tempfile::tempdir().unwrap();
        fixture_copy(tmp.path(), "a.flac");
        fixture_copy(tmp.path(), "b.flac");
        // broken "audio" file → import_errors
        std::fs::write(tmp.path().join("kaputt.mp3"), b"not audio").unwrap();
        // non-audio is ignored
        std::fs::write(tmp.path().join("cover.jpg"), b"jpg").unwrap();

        let mut conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();

        let r1 = scan_folder(&mut conn, tmp.path()).unwrap();
        assert_eq!((r1.added, r1.errors), (2, 1));

        // second scan: nothing changed → everything skipped
        let r2 = scan_folder(&mut conn, tmp.path()).unwrap();
        assert_eq!(r2.skipped_unchanged, 2);
        assert_eq!(r2.added, 0);

        let errs: i64 = conn
            .query_row("SELECT count(*) FROM import_errors", [], |r| r.get(0))
            .unwrap();
        assert_eq!(errs, 1);
    }

    /// A file that fails to import once and is later repaired (same path, valid
    /// content) must not leave a stale row behind in `import_errors`.
    #[test]
    fn fixing_a_broken_file_clears_its_import_error() {
        let tmp = tempfile::tempdir().unwrap();
        // lofty::read_from_path determines the file type from the extension, so the
        // broken and repaired content must both use a ".flac" path for the repair
        // to actually parse.
        let path = tmp.path().join("flaky.flac");
        std::fs::write(&path, b"not audio").unwrap();

        let mut conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();

        let r1 = scan_folder(&mut conn, tmp.path()).unwrap();
        assert_eq!(r1.errors, 1);

        let path_str = path.to_string_lossy().to_string();
        let errs_after_break: i64 = conn
            .query_row(
                "SELECT count(*) FROM import_errors WHERE path = ?1",
                [&path_str],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(errs_after_break, 1);

        // "Repair" the file: overwrite the SAME path with valid audio content.
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sine.flac");
        std::fs::copy(&src, &path).unwrap();

        let r2 = scan_folder(&mut conn, tmp.path()).unwrap();
        assert_eq!(r2.added, 1);

        let errs_after_fix: i64 = conn
            .query_row(
                "SELECT count(*) FROM import_errors WHERE path = ?1",
                [&path_str],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(errs_after_fix, 0);
    }

    /// Rescanning a track whose file was modified must update tag-derived
    /// columns while preserving user data (rating, play_count, added_at).
    #[test]
    fn rescan_preserves_rating_play_count_and_added_at_on_update() {
        let tmp = tempfile::tempdir().unwrap();
        let file = fixture_copy(tmp.path(), "track.flac");
        let mut tag = Tag::new(TagType::VorbisComments);
        tag.set_title("Original Title".into());
        tag.save_to_path(&file, lofty::config::WriteOptions::default())
            .unwrap();

        let mut conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();

        let r1 = scan_folder(&mut conn, tmp.path()).unwrap();
        assert_eq!(r1.added, 1);

        let path_str = file.to_string_lossy().to_string();
        // Simulate user data plus a "changed file" without depending on filesystem
        // mtime granularity: force file_mtime to 0 so the next scan re-reads it.
        conn.execute(
            "UPDATE tracks SET rating = 4, play_count = 7, file_mtime = 0 WHERE path = ?1",
            [&path_str],
        )
        .unwrap();
        let added_at_before: i64 = conn
            .query_row(
                "SELECT added_at FROM tracks WHERE path = ?1",
                [&path_str],
                |r| r.get(0),
            )
            .unwrap();

        // Re-tag the same file with a different title.
        let mut tag2 = Tag::new(TagType::VorbisComments);
        tag2.set_title("New Title".into());
        tag2.save_to_path(&file, lofty::config::WriteOptions::default())
            .unwrap();

        let r2 = scan_folder(&mut conn, tmp.path()).unwrap();
        assert_eq!(r2.updated, 1);

        let (title, rating, play_count, added_at): (String, i64, i64, i64) = conn
            .query_row(
                "SELECT title, rating, play_count, added_at FROM tracks WHERE path = ?1",
                [&path_str],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .unwrap();
        assert_eq!(title, "New Title");
        assert_eq!(rating, 4);
        assert_eq!(play_count, 7);
        assert_eq!(added_at, added_at_before);
    }

    /// walkdir traversal errors (e.g. a permission-denied subdirectory) must be
    /// recorded in `import_errors` and counted, never silently dropped. This test
    /// is skipped when running with elevated privileges (e.g. root in a
    /// container), where `chmod 000` does not actually block directory reads.
    #[cfg(unix)]
    #[test]
    fn traversal_error_in_unreadable_dir_is_recorded_not_dropped() {
        use std::os::unix::fs::PermissionsExt;

        let tmp = tempfile::tempdir().unwrap();
        fixture_copy(tmp.path(), "readable.flac");
        let locked = tmp.path().join("locked");
        std::fs::create_dir(&locked).unwrap();
        fixture_copy(&locked, "unreachable.flac");

        let mut perms = std::fs::metadata(&locked).unwrap().permissions();
        perms.set_mode(0o000);
        std::fs::set_permissions(&locked, perms).unwrap();

        if std::fs::read_dir(&locked).is_ok() {
            // Permissions did not actually block reads (e.g. running as root).
            let mut restore = std::fs::metadata(&locked).unwrap().permissions();
            restore.set_mode(0o755);
            std::fs::set_permissions(&locked, restore).unwrap();
            eprintln!(
                "skipping traversal_error_in_unreadable_dir_is_recorded_not_dropped: \
                 directory permissions are not enforced in this environment"
            );
            return;
        }

        let mut conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();

        let scan_result = scan_folder(&mut conn, tmp.path());

        // Always restore permissions before asserting, so tempdir cleanup succeeds
        // even if the assertions below fail.
        let mut restore = std::fs::metadata(&locked).unwrap().permissions();
        restore.set_mode(0o755);
        std::fs::set_permissions(&locked, restore).unwrap();

        let report = scan_result.unwrap();
        assert!(report.errors >= 1, "expected traversal error to be counted");
        assert_eq!(report.added, 1, "the readable file must still be scanned");

        let errs: i64 = conn
            .query_row("SELECT count(*) FROM import_errors", [], |r| r.get(0))
            .unwrap();
        assert!(
            errs >= 1,
            "traversal error must be recorded in import_errors"
        );
    }
}
