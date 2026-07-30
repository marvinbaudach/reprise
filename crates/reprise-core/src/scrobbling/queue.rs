//! Provider-specific durable scrobble queues.

use rusqlite::params;

use super::{Listen, QueueError};

const LISTENBRAINZ_BATCH_LIMIT: usize = 1_000;
const LASTFM_BATCH_LIMIT: usize = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrobbleProvider {
    ListenBrainz,
    LastFm,
}

impl ScrobbleProvider {
    fn table(self) -> &'static str {
        match self {
            Self::ListenBrainz => "listenbrainz_queue",
            Self::LastFm => "lastfm_queue",
        }
    }

    fn batch_limit(self) -> usize {
        match self {
            Self::ListenBrainz => LISTENBRAINZ_BATCH_LIMIT,
            Self::LastFm => LASTFM_BATCH_LIMIT,
        }
    }

    fn submitted_key(self) -> &'static str {
        match self {
            Self::ListenBrainz => "scrobble.submitted.listenbrainz",
            Self::LastFm => "scrobble.submitted.lastfm",
        }
    }
}

pub fn enqueue_for(
    db: &crate::db::Db,
    provider: ScrobbleProvider,
    listen: &Listen,
) -> Result<i64, QueueError> {
    let conn = db.conn();
    listen.track.validate()?;
    let release_name = listen
        .track
        .release_name
        .as_deref()
        .map(str::trim)
        .filter(|release| !release.is_empty());
    let sql = format!(
        "INSERT INTO {} \
         (listened_at, artist_name, track_name, release_name, duration_ms) \
         VALUES (?1, ?2, ?3, ?4, ?5)",
        provider.table()
    );
    conn.execute(
        &sql,
        params![
            listen.listened_at,
            listen.track.artist_name.trim(),
            listen.track.track_name.trim(),
            release_name,
            listen.track.duration_ms.max(0),
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn pending_for(
    db: &crate::db::Db,
    provider: ScrobbleProvider,
    limit: usize,
) -> Result<Vec<Listen>, QueueError> {
    let conn = db.conn();
    let limit = limit.min(provider.batch_limit());
    if limit == 0 {
        return Ok(Vec::new());
    }
    let sql = format!(
        "SELECT id, listened_at, artist_name, track_name, release_name, duration_ms \
         FROM {} ORDER BY id ASC LIMIT ?1",
        provider.table()
    );
    let mut statement = conn.prepare(&sql)?;
    let rows = statement.query_map(params![limit as i64], |row| {
        Ok(Listen {
            id: Some(row.get(0)?),
            listened_at: row.get(1)?,
            track: super::TrackMetadata {
                artist_name: row.get(2)?,
                track_name: row.get(3)?,
                release_name: row.get(4)?,
                duration_ms: row.get(5)?,
            },
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(QueueError::from)
}

pub fn acknowledge_for(
    db: &crate::db::Db,
    provider: ScrobbleProvider,
    ids: &[i64],
) -> Result<(), QueueError> {
    let conn = db.conn();
    if ids.is_empty() {
        return Ok(());
    }
    let sql = format!("DELETE FROM {} WHERE id = ?1", provider.table());
    let transaction = conn.unchecked_transaction()?;
    for id in ids {
        transaction.execute(&sql, params![id])?;
    }
    // Increment the cumulative submitted counter inside the same transaction
    // so a crash between DELETE and increment cannot produce a miscount.
    let key = provider.submitted_key();
    let current: i64 = transaction
        .query_row(
            "SELECT COALESCE((SELECT CAST(value AS INTEGER) FROM settings WHERE key = ?1), 0)",
            params![key],
            |row| row.get(0),
        )
        .unwrap_or(0);
    let new_total = current.saturating_add(ids.len() as i64);
    transaction.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2) \
         ON CONFLICT(key) DO UPDATE SET value = ?2",
        params![key, new_total.to_string()],
    )?;
    transaction.commit()?;
    Ok(())
}

/// Returns the cumulative number of listens submitted to the provider.
/// Starts at 0 for fresh installs and survives disconnects.
pub fn submitted_count_for(
    db: &crate::db::Db,
    provider: ScrobbleProvider,
) -> Result<usize, QueueError> {
    let conn = db.conn();
    let key = provider.submitted_key();
    let count: i64 = conn
        .query_row(
            "SELECT COALESCE((SELECT CAST(value AS INTEGER) FROM settings WHERE key = ?1), 0)",
            params![key],
            |row| row.get(0),
        )
        .map_err(QueueError::from)?;
    usize::try_from(count).map_err(|_| QueueError::InvalidCount)
}

pub fn clear_pending_for(
    db: &crate::db::Db,
    provider: ScrobbleProvider,
) -> Result<usize, QueueError> {
    let conn = db.conn();
    let sql = format!("DELETE FROM {}", provider.table());
    conn.execute(&sql, []).map_err(QueueError::from)
}

pub fn pending_count_for(
    db: &crate::db::Db,
    provider: ScrobbleProvider,
) -> Result<usize, QueueError> {
    let conn = db.conn();
    let sql = format!("SELECT COUNT(*) FROM {}", provider.table());
    let count: i64 = conn.query_row(&sql, [], |row| row.get(0))?;
    usize::try_from(count).map_err(|_| QueueError::InvalidCount)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scrobbling::{Listen, TrackMetadata};

    fn conn() -> crate::db::Db {
        crate::db::Db::open_in_memory().unwrap()
    }

    fn listen(timestamp: i64) -> Listen {
        Listen {
            id: None,
            listened_at: timestamp,
            track: TrackMetadata {
                artist_name: "Portishead".to_string(),
                track_name: format!("Roads {timestamp}"),
                release_name: Some("Dummy".to_string()),
                duration_ms: 307_000,
            },
        }
    }

    #[test]
    fn provider_queues_are_isolated_for_pending_and_count() {
        let conn = conn();
        enqueue_for(&conn, ScrobbleProvider::ListenBrainz, &listen(1)).unwrap();
        enqueue_for(&conn, ScrobbleProvider::LastFm, &listen(2)).unwrap();
        assert_eq!(
            pending_count_for(&conn, ScrobbleProvider::ListenBrainz).unwrap(),
            1
        );
        assert_eq!(
            pending_count_for(&conn, ScrobbleProvider::LastFm).unwrap(),
            1
        );
        assert_eq!(
            pending_for(&conn, ScrobbleProvider::ListenBrainz, 10).unwrap()[0].listened_at,
            1
        );
        assert_eq!(
            pending_for(&conn, ScrobbleProvider::LastFm, 10).unwrap()[0].listened_at,
            2
        );
    }

    #[test]
    fn lastfm_pending_is_fifo_and_clamped_to_fifty() {
        let conn = conn();
        for timestamp in 0..55 {
            enqueue_for(&conn, ScrobbleProvider::LastFm, &listen(timestamp)).unwrap();
        }
        let pending = pending_for(&conn, ScrobbleProvider::LastFm, usize::MAX).unwrap();
        assert_eq!(pending.len(), 50);
        assert_eq!(pending.first().unwrap().listened_at, 0);
        assert_eq!(pending.last().unwrap().listened_at, 49);
    }

    #[test]
    fn provider_acknowledge_deletes_only_matching_rows() {
        let conn = conn();
        let listenbrainz = enqueue_for(&conn, ScrobbleProvider::ListenBrainz, &listen(1)).unwrap();
        let lastfm = enqueue_for(&conn, ScrobbleProvider::LastFm, &listen(2)).unwrap();
        acknowledge_for(&conn, ScrobbleProvider::LastFm, &[lastfm]).unwrap();
        assert_eq!(
            pending_count_for(&conn, ScrobbleProvider::LastFm).unwrap(),
            0
        );
        assert_eq!(
            pending_for(&conn, ScrobbleProvider::ListenBrainz, 10).unwrap()[0].id,
            Some(listenbrainz)
        );
    }

    #[test]
    fn clear_removes_only_the_selected_provider() {
        let conn = conn();
        enqueue_for(&conn, ScrobbleProvider::ListenBrainz, &listen(1)).unwrap();
        enqueue_for(&conn, ScrobbleProvider::LastFm, &listen(2)).unwrap();
        assert_eq!(
            clear_pending_for(&conn, ScrobbleProvider::LastFm).unwrap(),
            1
        );
        assert_eq!(
            pending_count_for(&conn, ScrobbleProvider::ListenBrainz).unwrap(),
            1
        );
    }

    #[test]
    fn lastfm_queue_survives_database_reopen() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("reprise.db");
        {
            let conn = crate::db::Db::open_migrated(Some(&path)).unwrap();
            enqueue_for(&conn, ScrobbleProvider::LastFm, &listen(7)).unwrap();
        }
        let conn = crate::db::Db::open_migrated(Some(&path)).unwrap();
        assert_eq!(
            pending_for(&conn, ScrobbleProvider::LastFm, 10).unwrap()[0].listened_at,
            7
        );
    }

    #[test]
    fn lastfm_queue_rejects_blank_required_metadata() {
        let conn = conn();
        let mut invalid = listen(1);
        invalid.track.track_name = " ".to_string();
        assert!(enqueue_for(&conn, ScrobbleProvider::LastFm, &invalid).is_err());
        assert_eq!(
            pending_count_for(&conn, ScrobbleProvider::LastFm).unwrap(),
            0
        );
    }

    #[test]
    fn zero_pending_limit_returns_no_rows_without_deleting_them() {
        let conn = conn();
        enqueue_for(&conn, ScrobbleProvider::LastFm, &listen(1)).unwrap();
        assert!(pending_for(&conn, ScrobbleProvider::LastFm, 0)
            .unwrap()
            .is_empty());
        assert_eq!(
            pending_count_for(&conn, ScrobbleProvider::LastFm).unwrap(),
            1
        );
    }

    #[test]
    fn acknowledge_increments_submitted_counter_atomically() {
        let conn = conn();
        assert_eq!(
            submitted_count_for(&conn, ScrobbleProvider::LastFm).unwrap(),
            0
        );
        let id1 = enqueue_for(&conn, ScrobbleProvider::LastFm, &listen(1)).unwrap();
        let id2 = enqueue_for(&conn, ScrobbleProvider::LastFm, &listen(2)).unwrap();
        acknowledge_for(&conn, ScrobbleProvider::LastFm, &[id1, id2]).unwrap();
        assert_eq!(
            submitted_count_for(&conn, ScrobbleProvider::LastFm).unwrap(),
            2
        );
        // Counter is cumulative: a second acknowledge adds to the total.
        let id3 = enqueue_for(&conn, ScrobbleProvider::LastFm, &listen(3)).unwrap();
        acknowledge_for(&conn, ScrobbleProvider::LastFm, &[id3]).unwrap();
        assert_eq!(
            submitted_count_for(&conn, ScrobbleProvider::LastFm).unwrap(),
            3
        );
        // Provider counters are isolated.
        assert_eq!(
            submitted_count_for(&conn, ScrobbleProvider::ListenBrainz).unwrap(),
            0
        );
    }

    #[test]
    fn submitted_counter_survives_disconnect_clear() {
        let conn = conn();
        let id = enqueue_for(&conn, ScrobbleProvider::ListenBrainz, &listen(1)).unwrap();
        acknowledge_for(&conn, ScrobbleProvider::ListenBrainz, &[id]).unwrap();
        assert_eq!(
            submitted_count_for(&conn, ScrobbleProvider::ListenBrainz).unwrap(),
            1
        );
        clear_pending_for(&conn, ScrobbleProvider::ListenBrainz).unwrap();
        assert_eq!(
            submitted_count_for(&conn, ScrobbleProvider::ListenBrainz).unwrap(),
            1
        );
    }
}
