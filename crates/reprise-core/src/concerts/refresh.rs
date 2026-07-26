use rusqlite::Connection;

pub(crate) const FETCH_TTL_SECONDS: i64 = 24 * 60 * 60;
const REFRESH_JITTER_MAX_SECONDS: i64 = 2 * 60 * 60;

#[must_use]
pub fn artist_due(last_attempt_at: Option<i64>, now: i64, force: bool) -> bool {
    let Some(last_attempt_at) = last_attempt_at else {
        return true;
    };
    if force {
        return true;
    }
    let elapsed = now.saturating_sub(last_attempt_at);
    elapsed >= FETCH_TTL_SECONDS
}

#[must_use]
pub fn refresh_due(latest_attempt: Option<i64>, now: i64, jitter: i64) -> bool {
    let Some(latest_attempt) = latest_attempt else {
        return true;
    };
    let elapsed = now.saturating_sub(latest_attempt);
    if elapsed < 0 {
        return false;
    }
    elapsed >= FETCH_TTL_SECONDS + jitter.clamp(0, REFRESH_JITTER_MAX_SECONDS)
}

#[must_use]
pub fn jitter_seconds(seed: &str) -> i64 {
    let hash = crate::artist_news_refresh::fnv1a_64(seed.as_bytes());
    (hash % (REFRESH_JITTER_MAX_SECONDS as u64 + 1)) as i64
}

pub(crate) fn latest_attempt(conn: &Connection) -> Result<Option<i64>, rusqlite::Error> {
    conn.query_row(
        "SELECT MAX(last_attempt_at) FROM concert_artists",
        [],
        |row| row.get(0),
    )
}
