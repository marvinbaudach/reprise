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

fn effective_gain_db_result(
    conn: &Connection,
    track_id: i64,
    mode: ReplayGainMode,
) -> Result<f64, rusqlite::Error> {
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
    Ok(resolve_gain(GainInputs {
        mode,
        tags,
        measured: measured_loudness(conn, track_id)?,
        album_measured: album_measured_loudness(conn, track_id)?,
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
    fn play_measured_gain_normalises_an_untagged_track() {
        let db = track((None, None));
        store_measured(&db);

        assert_eq!(effective_gain_db(&db, 1, ReplayGainMode::Track), 3.0);
    }

    #[test]
    fn play_tagged_gain_wins_over_the_measurement() {
        let db = track((Some(-4.0), Some(0.5)));
        store_measured(&db);

        assert_eq!(effective_gain_db(&db, 1, ReplayGainMode::Track), -4.0);
    }

    #[test]
    fn play_off_disables_tags_and_measurements() {
        let db = track((Some(-4.0), Some(0.5)));
        store_measured(&db);

        assert_eq!(effective_gain_db(&db, 1, ReplayGainMode::Off), 0.0);
    }
}
