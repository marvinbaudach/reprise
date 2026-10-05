use rusqlite::{Connection, OptionalExtension};

use crate::db::Db;
use crate::library::loudness::{resolve_gain, GainInputs, ReplayGainTags};
use crate::library::loudness_store::{album_measured_loudness, measured_loudness};
use crate::library::settings::ReplayGainMode;

pub fn effective_gain_db(db: &Db, track_id: i64, mode: ReplayGainMode) -> f64 {
    effective_gain_db_result(db.conn(), track_id, mode).unwrap_or_else(|error| {
        tracing::warn!(track_id, %error, "could not resolve track gain; using unity gain");
        0.0
    })
}

type AlbumLookup = fn(&Connection, i64) -> Result<Option<(f64, f64)>, rusqlite::Error>;

fn effective_gain_db_result(
    conn: &Connection,
    track_id: i64,
    mode: ReplayGainMode,
) -> Result<f64, rusqlite::Error> {
    resolve_for_track(conn, track_id, mode, album_measured_loudness)
}

/// Runs only the queries the mode can use: `Off` none, `Track` the tags and the
/// track's own measurement, `Album` additionally the album lookup (and only
/// when the album carries no tag of its own). This runs on the UI thread for
/// every play and pre-feed.
fn resolve_for_track(
    conn: &Connection,
    track_id: i64,
    mode: ReplayGainMode,
    album_lookup: AlbumLookup,
) -> Result<f64, rusqlite::Error> {
    if mode == ReplayGainMode::Off {
        return Ok(0.0);
    }
    let tags = conn
        .query_row(
            "SELECT rg_track_gain, rg_track_peak, rg_album_gain, rg_album_peak \
             FROM tracks WHERE id = ?1",
            [track_id],
            |row| {
                Ok(ReplayGainTags {
                    track_gain_db: row.get(0)?,
                    track_peak: row.get(1)?,
                    album_gain_db: row.get(2)?,
                    album_peak: row.get(3)?,
                })
            },
        )
        .optional()?
        .unwrap_or_default();
    let album_measured = if mode == ReplayGainMode::Album && tags.album_gain_db.is_none() {
        album_lookup(conn, track_id)?
    } else {
        None
    };
    Ok(resolve_gain(GainInputs {
        mode,
        tags,
        measured: measured_loudness(conn, track_id)?,
        album_measured,
    })
    .gain_db)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use crate::library::loudness::MeasuredLoudness;
    use crate::library::loudness_store::write_track_loudness;
    use crate::spectrogram::TrackSourceFingerprint;

    fn track(tags: (Option<f64>, Option<f64>)) -> Db {
        let db = Db::open_in_memory().unwrap();
        db.conn()
            .execute(
                "INSERT INTO tracks \
                 (id, path, title, album, artist, added_at, duration_ms, file_mtime, file_size, \
                  device, inode, rg_track_gain, rg_track_peak) \
                 VALUES (1, '/track.flac', '', 'Album', 'Artist', 0, 1000, 11, 22, 33, 44, ?1, ?2)",
                rusqlite::params![tags.0, tags.1],
            )
            .unwrap();
        db
    }

    fn store_measured(db: &Db) {
        write_track_loudness(
            db.conn(),
            1,
            TrackSourceFingerprint {
                mtime_seconds: 11,
                size_bytes: 22,
                device: Some(33),
                inode: Some(44),
            },
            Some(MeasuredLoudness {
                integrated_lufs: -21.0,
                true_peak: 0.5,
            }),
        )
        .unwrap();
    }

    #[test]
    fn play_19_measured_gain_normalises_an_untagged_track() {
        let db = track((None, None));
        store_measured(&db);

        assert_eq!(effective_gain_db(&db, 1, ReplayGainMode::Track), 3.0);
    }

    #[test]
    fn play_19_tagged_gain_wins_over_the_measurement() {
        let db = track((Some(-4.0), Some(0.5)));
        store_measured(&db);

        assert_eq!(effective_gain_db(&db, 1, ReplayGainMode::Track), -4.0);
    }

    #[test]
    fn play_19_off_disables_tags_and_measurements() {
        let db = track((Some(-4.0), Some(0.5)));
        store_measured(&db);

        assert_eq!(effective_gain_db(&db, 1, ReplayGainMode::Off), 0.0);
    }

    use std::sync::atomic::{AtomicUsize, Ordering};

    static ALBUM_LOOKUPS: AtomicUsize = AtomicUsize::new(0);

    fn counted_album_lookup(_: &Connection, _: i64) -> Result<Option<(f64, f64)>, rusqlite::Error> {
        ALBUM_LOOKUPS.fetch_add(1, Ordering::SeqCst);
        Ok(None)
    }

    #[test]
    fn play_19_off_runs_no_query_at_all() {
        // A connection without a schema: any query would fail.
        let bare = Connection::open_in_memory().unwrap();

        assert_eq!(
            resolve_for_track(&bare, 1, ReplayGainMode::Off, counted_album_lookup).unwrap(),
            0.0
        );
    }

    #[test]
    fn play_19_only_album_mode_runs_the_album_lookup() {
        let db = track((None, None));
        store_measured(&db);
        let before = ALBUM_LOOKUPS.load(Ordering::SeqCst);

        resolve_for_track(db.conn(), 1, ReplayGainMode::Track, counted_album_lookup).unwrap();
        assert_eq!(ALBUM_LOOKUPS.load(Ordering::SeqCst), before);

        resolve_for_track(db.conn(), 1, ReplayGainMode::Album, counted_album_lookup).unwrap();
        assert_eq!(ALBUM_LOOKUPS.load(Ordering::SeqCst), before + 1);
    }
}
