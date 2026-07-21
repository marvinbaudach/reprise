//! Coverage-honest audio-character projection over local listen events.

use chrono::TimeZone;
use rusqlite::{params, Connection};

use super::stats_period::StatsPeriod;
use super::stats_screen::first_event_unix;
use crate::audio_analysis::CURRENT_EXTRACTOR_VERSION;
use crate::mix_planner::ProfileTarget;
use crate::sound_profile::CURRENT_PROFILE_VERSION;

pub const MIN_ANALYZED_PLAYS: i64 = 20;
pub const MIN_COVERAGE_PERCENT: i64 = 70;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProfileDirection {
    Intensity,
    Brightness,
    Dynamicity,
    Rhythmicity,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ListenedAudioCharacter {
    pub target: ProfileTarget,
    pub direction: ProfileDirection,
    pub analyzed_plays: i64,
    pub total_plays: i64,
}

pub fn compute<Tz: TimeZone>(
    conn: &Connection,
    period: StatsPeriod,
    now_unix: i64,
    tz: &Tz,
) -> Result<Option<ListenedAudioCharacter>, rusqlite::Error> {
    let range = period.resolve(now_unix, tz, first_event_unix(conn)?);
    let sql = format!(
        "SELECT COUNT(*), COUNT(a.track_id), AVG(a.intensity), AVG(a.brightness),
                AVG(a.dynamicity), AVG(a.rhythmicity)
         FROM listen_events le
         JOIN tracks t ON t.id = le.track_id
         LEFT JOIN track_audio_analysis a
           ON a.track_id = t.id AND a.status = 'ready'
          AND a.source_mtime = t.file_mtime AND a.source_size = t.file_size
          AND a.extractor_version = {CURRENT_EXTRACTOR_VERSION}
          AND a.profile_version = {CURRENT_PROFILE_VERSION}
         WHERE t.removed_at IS NULL AND le.played_at >= ?1 AND le.played_at < ?2"
    );
    let row = conn.query_row(&sql, params![range.start_unix, range.end_unix], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, Option<f64>>(2)?,
            row.get::<_, Option<f64>>(3)?,
            row.get::<_, Option<f64>>(4)?,
            row.get::<_, Option<f64>>(5)?,
        ))
    })?;
    let (total_plays, analyzed_plays) = (row.0, row.1);
    if analyzed_plays < MIN_ANALYZED_PLAYS
        || analyzed_plays * 100 < total_plays * MIN_COVERAGE_PERCENT
    {
        return Ok(None);
    }
    let values = [row.2, row.3, row.4, row.5]
        .map(|value| value.ok_or(rusqlite::Error::InvalidQuery))
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?;
    let target = ProfileTarget::new(values[0], values[1], values[2], values[3])
        .map_err(|_| rusqlite::Error::InvalidQuery)?;
    let direction = [
        ProfileDirection::Intensity,
        ProfileDirection::Brightness,
        ProfileDirection::Dynamicity,
        ProfileDirection::Rhythmicity,
    ]
    .into_iter()
    .zip(values)
    .max_by(|left, right| left.1.total_cmp(&right.1))
    .map_or(ProfileDirection::Intensity, |(direction, _)| direction);
    Ok(Some(ListenedAudioCharacter {
        target,
        direction,
        analyzed_plays,
        total_plays,
    }))
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use rusqlite::{params, Connection};

    use super::*;

    fn fixture() -> Connection {
        crate::db::open_migrated(None).unwrap()
    }

    fn track(conn: &Connection, id: i64, values: Option<[f64; 4]>) {
        conn.execute(
            "INSERT INTO tracks
             (id, path, title, artist, album, duration_ms, added_at, file_mtime, file_size)
             VALUES (?1, ?2, ?3, 'Artist', 'Album', 180000, 1, 10, 20)",
            params![id, format!("/fixture/{id}.flac"), format!("Track {id}")],
        )
        .unwrap();
        let Some([intensity, brightness, dynamicity, rhythmicity]) = values else {
            return;
        };
        conn.execute(
            "INSERT INTO track_audio_analysis
             (track_id, source_mtime, source_size, extractor_version, profile_version,
              analyzed_at, status, loudness_rms, dynamic_range, spectral_centroid_hz,
              spectral_rolloff_hz, spectral_flux, onset_rate, intensity,
              intensity_confidence, brightness, brightness_confidence, dynamicity,
              dynamicity_confidence, rhythmicity, rhythmicity_confidence)
             VALUES (?1, 10, 20, 1, 1, 30, 'ready', 0.1, 0.2, 1000, 2000,
                     0.3, 0.4, ?2, 0.9, ?3, 0.9, ?4, 0.9, ?5, 0.9)",
            params![id, intensity, brightness, dynamicity, rhythmicity],
        )
        .unwrap();
    }

    fn listens(conn: &Connection, track_id: i64, count: i64, start: i64) {
        for offset in 0..count {
            conn.execute(
                "INSERT INTO listen_events (track_id, played_at, ms_played)
                 VALUES (?1, ?2, 180000)",
                params![track_id, start + offset],
            )
            .unwrap();
        }
    }

    #[test]
    fn listened_audio_character_requires_twenty_analyzed_plays_and_seventy_percent_coverage() {
        let conn = fixture();
        track(&conn, 1, Some([0.8, 0.2, 0.3, 0.4]));
        track(&conn, 2, None);
        track(&conn, 3, None);
        conn.execute("UPDATE tracks SET removed_at = 1 WHERE id = 3", [])
            .unwrap();
        listens(&conn, 3, 100, 500);
        listens(&conn, 1, 19, 100);
        listens(&conn, 2, 1, 200);
        assert_eq!(
            compute(&conn, StatsPeriod::AllTime, 1_000, &Utc).unwrap(),
            None
        );

        listens(&conn, 1, 2, 300);
        listens(&conn, 2, 8, 400);
        let insight = compute(&conn, StatsPeriod::AllTime, 1_000, &Utc)
            .unwrap()
            .expect("21 of 30 plays reaches both thresholds");
        assert_eq!((insight.analyzed_plays, insight.total_plays), (21, 30));
    }

    #[test]
    fn repeated_listens_weight_the_profile_and_period_bounds_are_respected() {
        let conn = fixture();
        track(&conn, 1, Some([0.8, 0.1, 0.2, 0.3]));
        track(&conn, 2, Some([0.2, 0.1, 0.2, 0.3]));
        listens(&conn, 1, 15, 1_700_000_000);
        listens(&conn, 2, 5, 1_700_000_100);
        let insight = compute(&conn, StatsPeriod::AllTime, 1_800_000_000, &Utc)
            .unwrap()
            .unwrap();
        assert_eq!(insight.target.values()[0], 0.65);
        assert_eq!(insight.direction, ProfileDirection::Intensity);
        assert_eq!(insight.analyzed_plays, 20);

        assert_eq!(
            compute(&conn, StatsPeriod::Year(2026), 1_800_000_000, &Utc).unwrap(),
            None
        );
    }
}
