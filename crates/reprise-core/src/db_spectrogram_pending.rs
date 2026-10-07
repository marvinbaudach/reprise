//! The tracks whose rendering data is still to be produced: whole files one by
//! one, and the tracks of a CUE file grouped by the file they are cut from.

use rusqlite::OptionalExtension;

use crate::db::{Db, DbError};
use crate::library::loudness_store::LOUDNESS_FORMAT_VERSION;
use crate::render_data_segments::SegmentBounds;
use crate::spectrogram::{TrackSourceFingerprint, SPECTROGRAM_FORMAT_VERSION};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingRenderDataTrack {
    pub track_id: i64,
    pub path: String,
    pub source: TrackSourceFingerprint,
}

/// One track of a CUE file that still needs its rendering data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingSegmentTrack {
    pub track_id: i64,
    pub start_ms: i64,
    pub end_ms: i64,
    /// The file's last track, which is measured to the decoded end of the file
    /// (see [`SegmentBounds::last_in_file`]).
    pub last_in_file: bool,
}

impl PendingSegmentTrack {
    /// The stretch of the file the decode measures for this track.
    #[must_use]
    pub fn bounds(&self) -> SegmentBounds {
        SegmentBounds {
            start_ms: self.start_ms,
            end_ms: self.end_ms,
            last_in_file: self.last_in_file,
        }
    }
}

/// A CUE file with at least one track that still needs rendering data. The file
/// is decoded once for all of them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingSegmentFile {
    pub path: String,
    pub source: TrackSourceFingerprint,
    pub tracks: Vec<PendingSegmentTrack>,
}

/// Returns live whole-file tracks whose rendering data is absent or stale, in
/// stable id order, leaving out those whose analysis is remembered as failed.
/// The tracks of a CUE file are not among them: each is a stretch of a file,
/// and [`pending_segment_render_data_files`] lists them.
pub fn pending_render_data_tracks(db: &Db) -> Result<Vec<PendingRenderDataTrack>, DbError> {
    let mut statement = db.conn().prepare(&format!(
        "SELECT t.id, t.path, t.file_mtime, t.file_size, t.device, t.inode \
         FROM tracks t \
         LEFT JOIN track_spectrograms s ON s.track_id = t.id \
           AND s.format_version = ?1 AND s.source_mtime = t.file_mtime \
           AND s.source_size = t.file_size AND s.source_device IS t.device \
           AND s.source_inode IS t.inode \
         LEFT JOIN track_loudness l ON l.track_id = t.id \
           AND l.format_version = ?2 AND l.source_mtime = t.file_mtime \
           AND l.source_size = t.file_size AND l.source_device IS t.device \
           AND l.source_inode IS t.inode \
         {} \
         WHERE f.track_id IS NULL AND {} AND t.segment_index = 0 \
           AND (t.waveform_peaks IS NULL OR s.track_id IS NULL OR l.track_id IS NULL) \
         ORDER BY t.id",
        super::failures::FAILURE_JOIN,
        crate::queries::PRESENT
    ))?;
    let tracks = statement
        .query_map(
            rusqlite::params![SPECTROGRAM_FORMAT_VERSION, LOUDNESS_FORMAT_VERSION],
            |row| {
                Ok(PendingRenderDataTrack {
                    track_id: row.get(0)?,
                    path: row.get(1)?,
                    source: TrackSourceFingerprint {
                        mtime_seconds: row.get(2)?,
                        size_bytes: row.get(3)?,
                        device: row.get(4)?,
                        inode: row.get(5)?,
                    },
                })
            },
        )?
        .collect::<Result<_, _>>()?;
    Ok(tracks)
}

/// Returns the CUE files with live tracks whose rendering data is absent or
/// stale, each with just those tracks in play order, in stable path order. A
/// track whose analysis is remembered as failed is left out.
pub fn pending_segment_render_data_files(db: &Db) -> Result<Vec<PendingSegmentFile>, DbError> {
    pending_segment_files(db, None)
}

/// The live tracks of the CUE file at `path` whose rendering data is absent or
/// stale, in play order.
pub fn pending_segment_tracks_of(db: &Db, path: &str) -> Result<Vec<PendingSegmentTrack>, DbError> {
    Ok(pending_segment_files(db, Some(path))?
        .into_iter()
        .flat_map(|file| file.tracks)
        .collect())
}

/// Whether the track `t` is the true last track of its file: no other track
/// of the file has a higher index, and no sheet track the user removed
/// (`library_exclusions`, which keep the index of a track that has no row any
/// more) comes after it. The last track is measured and played to the end of
/// the file, so a track whose successor is only excluded must not count: the
/// removed track's audio is not its own. An exclusion belongs to the file by
/// its identity, or by its path while it has none, as a scan matches it.
const LAST_IN_FILE: &str = "t.segment_index >= MAX( \
        (SELECT MAX(u.segment_index) FROM tracks u WHERE u.path = t.path), \
        COALESCE((SELECT MAX(e.segment_index) FROM library_exclusions e \
                  WHERE (e.device IS NOT NULL AND e.inode IS NOT NULL \
                         AND e.device IS t.device AND e.inode IS t.inode) \
                     OR ((e.device IS NULL OR e.inode IS NULL) AND e.path = t.path)), 0))";

/// Whether `track_id` is the last track of its file, see [`LAST_IN_FILE`]; a
/// track that does not exist is not.
pub fn track_is_last_in_file(db: &Db, track_id: i64) -> Result<bool, DbError> {
    let last: Option<bool> = db
        .conn()
        .query_row(
            &format!("SELECT {LAST_IN_FILE} FROM tracks t WHERE t.id = ?1"),
            [track_id],
            |row| row.get(0),
        )
        .optional()?;
    Ok(last.unwrap_or(false))
}

fn pending_segment_files(db: &Db, path: Option<&str>) -> Result<Vec<PendingSegmentFile>, DbError> {
    let mut statement = db.conn().prepare(&format!(
        "SELECT t.id, t.path, t.file_mtime, t.file_size, t.device, t.inode, \
                t.segment_start_ms, t.segment_end_ms, \
                {} \
         FROM tracks t \
         LEFT JOIN track_spectrograms s ON s.track_id = t.id \
           AND s.format_version = ?1 AND s.source_mtime = t.file_mtime \
           AND s.source_size = t.file_size AND s.source_device IS t.device \
           AND s.source_inode IS t.inode \
         LEFT JOIN track_loudness l ON l.track_id = t.id \
           AND l.format_version = ?2 AND l.source_mtime = t.file_mtime \
           AND l.source_size = t.file_size AND l.source_device IS t.device \
           AND l.source_inode IS t.inode \
         {} \
         WHERE f.track_id IS NULL AND {} AND t.segment_index > 0 \
           AND t.segment_start_ms IS NOT NULL AND t.segment_end_ms IS NOT NULL \
           AND (t.waveform_peaks IS NULL OR s.track_id IS NULL OR l.track_id IS NULL) \
           AND (?3 IS NULL OR t.path = ?3) \
         ORDER BY t.path, t.segment_index",
        LAST_IN_FILE,
        super::failures::FAILURE_JOIN,
        crate::queries::PRESENT
    ))?;
    let rows = statement.query_map(
        rusqlite::params![SPECTROGRAM_FORMAT_VERSION, LOUDNESS_FORMAT_VERSION, path],
        |row| {
            Ok((
                row.get::<_, String>(1)?,
                TrackSourceFingerprint {
                    mtime_seconds: row.get(2)?,
                    size_bytes: row.get(3)?,
                    device: row.get(4)?,
                    inode: row.get(5)?,
                },
                PendingSegmentTrack {
                    track_id: row.get(0)?,
                    start_ms: row.get(6)?,
                    end_ms: row.get(7)?,
                    last_in_file: row.get(8)?,
                },
            ))
        },
    )?;
    let mut files: Vec<PendingSegmentFile> = Vec::new();
    for row in rows {
        let (path, source, track) = row?;
        match files.last_mut() {
            Some(file) if file.path == path => file.tracks.push(track),
            _ => files.push(PendingSegmentFile {
                path,
                source,
                tracks: vec![track],
            }),
        }
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db_spectrogram::{set_track_spectrogram, set_waveform_peaks};
    use crate::spectrogram::TrackSpectrogram;

    fn source() -> TrackSourceFingerprint {
        TrackSourceFingerprint {
            mtime_seconds: 11,
            size_bytes: 22,
            device: Some(33),
            inode: Some(44),
        }
    }

    #[test]
    fn pending_render_data_includes_tracks_with_old_peaks_but_no_spectrogram() {
        let db = Db::open_in_memory().unwrap();
        for (id, missing_since) in [(1, None), (2, None), (3, Some(1))] {
            db.conn()
                .execute(
                    "INSERT INTO tracks \
                     (id, path, title, added_at, file_mtime, file_size, device, inode, missing_since) \
                     VALUES (?1, ?2, '', 0, 11, 22, 33, ?3, ?4)",
                    rusqlite::params![id, format!("/{id}.flac"), 40 + id, missing_since],
                )
                .unwrap();
        }
        set_waveform_peaks(&db, 1, &[9]).unwrap();

        assert_eq!(
            pending_render_data_tracks(&db).unwrap(),
            vec![
                PendingRenderDataTrack {
                    track_id: 1,
                    path: "/1.flac".into(),
                    source: TrackSourceFingerprint {
                        inode: Some(41),
                        ..source()
                    },
                },
                PendingRenderDataTrack {
                    track_id: 2,
                    path: "/2.flac".into(),
                    source: TrackSourceFingerprint {
                        inode: Some(42),
                        ..source()
                    },
                },
            ]
        );
    }

    #[test]
    fn pending_render_data_includes_a_track_missing_only_loudness() {
        let db = Db::open_in_memory().unwrap();
        db.conn()
            .execute(
                "INSERT INTO tracks \
                 (id, path, title, added_at, file_mtime, file_size, device, inode, waveform_peaks) \
                 VALUES (1, '/old.flac', '', 0, 11, 22, 33, 44, X'01')",
                [],
            )
            .unwrap();
        set_track_spectrogram(&db, 1, source(), &TrackSpectrogram::empty()).unwrap();

        assert_eq!(pending_render_data_tracks(&db).unwrap().len(), 1);
    }

    #[test]
    fn the_last_track_of_each_cue_file_is_marked_to_run_to_the_end() {
        let db = Db::open_in_memory().unwrap();
        db.conn()
            .execute_batch(
                "INSERT INTO tracks (id, path, title, added_at, file_mtime, file_size,
                                     device, inode, segment_index, segment_start_ms,
                                     segment_end_ms)
                 VALUES (1, '/a.flac', '', 0, 11, 22, 33, 44, 1, 0, 3000),
                        (2, '/a.flac', '', 0, 11, 22, 33, 44, 2, 3000, 8000),
                        (3, '/b.flac', '', 0, 11, 22, 33, 45, 1, 0, 5000);",
            )
            .unwrap();

        let files = pending_segment_render_data_files(&db).unwrap();

        let marks: Vec<(i64, bool)> = files
            .iter()
            .flat_map(|file| &file.tracks)
            .map(|track| (track.track_id, track.last_in_file))
            .collect();
        assert_eq!(marks, [(1, false), (2, true), (3, true)]);
        assert_eq!(
            pending_segment_tracks_of(&db, "/a.flac").unwrap()[1].bounds(),
            SegmentBounds {
                start_ms: 3_000,
                end_ms: 8_000,
                last_in_file: true,
            }
        );
    }

    /// Two tracks of `/a.flac` (identity 33/44) are left of a sheet whose
    /// third track the user removed.
    fn database_with_an_excluded_final_segment(exclusion: &str) -> Db {
        let db = Db::open_in_memory().unwrap();
        db.conn()
            .execute_batch(&format!(
                "INSERT INTO tracks (id, path, title, added_at, file_mtime, file_size,
                                     device, inode, segment_index, segment_start_ms,
                                     segment_end_ms)
                 VALUES (1, '/a.flac', '', 0, 11, 22, 33, 44, 1, 0, 3000),
                        (2, '/a.flac', '', 0, 11, 22, 33, 44, 2, 3000, 8000);
                 INSERT INTO library_exclusions
                   (path, device, inode, file_size, file_mtime, excluded_at, segment_index)
                 VALUES {exclusion};"
            ))
            .unwrap();
        db
    }

    #[test]
    fn mtp_66_a_track_whose_successor_is_only_excluded_keeps_its_own_end() {
        for exclusion in [
            "('/a.flac', 33, 44, 22, 11, 0, 3)",
            // A file without a stat identity is matched by its path.
            "('/a.flac', NULL, NULL, 22, 11, 0, 3)",
            // Excluded under another name; the identity follows the file.
            "('/renamed.flac', 33, 44, 22, 11, 0, 3)",
        ] {
            let db = database_with_an_excluded_final_segment(exclusion);

            let marks: Vec<bool> = pending_segment_tracks_of(&db, "/a.flac")
                .unwrap()
                .iter()
                .map(|track| track.last_in_file)
                .collect();

            assert_eq!(marks, [false, false], "{exclusion}");
        }
    }

    #[test]
    fn an_exclusion_of_another_file_or_of_the_whole_file_changes_no_last_track() {
        let db = database_with_an_excluded_final_segment("('/b.flac', 55, 66, 22, 11, 0, 3)");
        db.conn()
            .execute(
                "INSERT INTO library_exclusions
                   (path, device, inode, file_size, file_mtime, excluded_at, segment_index)
                 VALUES ('/a.flac', 33, 44, 22, 11, 0, 0)",
                [],
            )
            .unwrap();

        let marks: Vec<bool> = pending_segment_tracks_of(&db, "/a.flac")
            .unwrap()
            .iter()
            .map(|track| track.last_in_file)
            .collect();

        assert_eq!(marks, [false, true]);
    }
}
