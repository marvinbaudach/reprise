//! Read/write access to the per-artist New Releases fetch ledger.
//!
//! Every refresh attempt lands here — success, unmatched artist, and network
//! failure alike. That is the whole point: freshness must not depend on
//! whether the artist happened to have news, or artists with nothing to
//! report get re-fetched forever (see `db_artist_news_fetch`).

use rusqlite::Connection;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FetchOutcome {
    Ok,
    Unmatched,
    Failed,
}

impl FetchOutcome {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            FetchOutcome::Ok => "ok",
            FetchOutcome::Unmatched => "unmatched",
            FetchOutcome::Failed => "failed",
        }
    }
}

/// Records one attempt. A later attempt that could not resolve an MBID keeps
/// the previously known one via `COALESCE` — losing a resolved MBID because
/// of one failed run would cost an extra search request on every future run.
pub(crate) fn record_attempt(
    conn: &Connection,
    artist_key: &str,
    artist_mbid: Option<&str>,
    now: i64,
    outcome: FetchOutcome,
    releases_found: usize,
) -> Result<(), rusqlite::Error> {
    conn.execute(
        "INSERT INTO artist_news_fetch
           (artist_key, artist_mbid, last_attempt_at, last_outcome, releases_found)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(artist_key) DO UPDATE SET
           artist_mbid     = COALESCE(excluded.artist_mbid, artist_news_fetch.artist_mbid),
           last_attempt_at = excluded.last_attempt_at,
           last_outcome    = excluded.last_outcome,
           releases_found  = excluded.releases_found",
        rusqlite::params![
            artist_key,
            artist_mbid,
            now,
            outcome.as_str(),
            i64::try_from(releases_found).unwrap_or(i64::MAX),
        ],
    )?;
    Ok(())
}

pub(crate) fn last_attempt_at(
    conn: &Connection,
    artist_key: &str,
) -> Result<Option<i64>, rusqlite::Error> {
    conn.query_row(
        "SELECT last_attempt_at FROM artist_news_fetch WHERE artist_key = ?1",
        [artist_key],
        |row| row.get::<_, Option<i64>>(0),
    )
    .or_else(|error| match error {
        rusqlite::Error::QueryReturnedNoRows => Ok(None),
        other => Err(other),
    })
}

/// The newest attempt across every artist — the clock `refresh_due` is
/// judged against.
pub(crate) fn latest_attempt(conn: &Connection) -> Result<Option<i64>, rusqlite::Error> {
    conn.query_row(
        "SELECT MAX(last_attempt_at) FROM artist_news_fetch",
        [],
        |row| row.get(0),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn() -> Connection {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();
        conn
    }

    #[test]
    fn record_attempt_inserts_then_updates_same_key() {
        let conn = conn();
        record_attempt(
            &conn,
            "pink floyd",
            Some("mbid-1"),
            100,
            FetchOutcome::Ok,
            3,
        )
        .unwrap();
        assert_eq!(last_attempt_at(&conn, "pink floyd").unwrap(), Some(100));

        record_attempt(&conn, "pink floyd", None, 200, FetchOutcome::Failed, 0).unwrap();
        assert_eq!(last_attempt_at(&conn, "pink floyd").unwrap(), Some(200));

        let rows: i64 = conn
            .query_row("SELECT COUNT(*) FROM artist_news_fetch", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(rows, 1, "same key must not create a second row");
    }

    #[test]
    fn record_attempt_keeps_known_mbid_when_later_attempt_has_none() {
        let conn = conn();
        record_attempt(
            &conn,
            "pink floyd",
            Some("mbid-1"),
            100,
            FetchOutcome::Ok,
            1,
        )
        .unwrap();
        record_attempt(&conn, "pink floyd", None, 200, FetchOutcome::Failed, 0).unwrap();
        let mbid: Option<String> = conn
            .query_row(
                "SELECT artist_mbid FROM artist_news_fetch WHERE artist_key = 'pink floyd'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(mbid.as_deref(), Some("mbid-1"));
    }

    #[test]
    fn unknown_key_has_no_attempt() {
        let conn = conn();
        assert_eq!(last_attempt_at(&conn, "nobody").unwrap(), None);
    }

    #[test]
    fn latest_attempt_reports_newest_across_all_artists() {
        let conn = conn();
        assert_eq!(latest_attempt(&conn).unwrap(), None);
        record_attempt(&conn, "a", None, 100, FetchOutcome::Unmatched, 0).unwrap();
        record_attempt(&conn, "b", None, 400, FetchOutcome::Ok, 2).unwrap();
        record_attempt(&conn, "c", None, 250, FetchOutcome::Failed, 0).unwrap();
        assert_eq!(latest_attempt(&conn).unwrap(), Some(400));
    }

    #[test]
    fn outcomes_serialize_to_stable_strings() {
        assert_eq!(FetchOutcome::Ok.as_str(), "ok");
        assert_eq!(FetchOutcome::Unmatched.as_str(), "unmatched");
        assert_eq!(FetchOutcome::Failed.as_str(), "failed");
    }
}
