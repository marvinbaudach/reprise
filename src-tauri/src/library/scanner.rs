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

#[derive(Debug, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")] // frontend expects skippedUnchanged
pub struct ScanReport {
    pub added: u32,
    pub updated: u32,
    pub skipped_unchanged: u32,
    pub errors: u32,
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
    let get =
        |f: &dyn Fn(&lofty::tag::Tag) -> Option<String>| tag.and_then(f).unwrap_or_default();
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

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub fn scan_folder(conn: &mut Connection, root: &Path) -> Result<ScanReport, ScanError> {
    let mut report = ScanReport::default();
    let tx = conn.transaction()?;
    for entry in walkdir::WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
    {
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
                tx.execute(
                    "INSERT INTO tracks (path, title, artist, album, album_artist, year,
                       track_no, genre, duration_ms, bitrate_kbps, added_at, file_mtime, missing)
                     VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,0)
                     ON CONFLICT(path) DO UPDATE SET
                       title=?2, artist=?3, album=?4, album_artist=?5, year=?6,
                       track_no=?7, genre=?8, duration_ms=?9, bitrate_kbps=?10,
                       file_mtime=?12, missing=0",
                    rusqlite::params![
                        path_str,
                        if meta.title.is_empty() {
                            path.file_stem()
                                .and_then(|s| s.to_str())
                                .unwrap_or("")
                                .to_string()
                        } else {
                            meta.title
                        },
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
                    ],
                )?;
                if is_update {
                    report.updated += 1
                } else {
                    report.added += 1
                }
            }
            Err(e) => {
                // import_errors has no UNIQUE constraint on path, so replace any
                // prior error row for this file to keep rescans from piling up
                // duplicate entries for a file that is still broken.
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
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/sine.flac");
        let dst = dir.join(name);
        std::fs::copy(&src, &dst).unwrap();
        dst
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
        tag.save_to_path(&file, lofty::config::WriteOptions::default()).unwrap();

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
}
