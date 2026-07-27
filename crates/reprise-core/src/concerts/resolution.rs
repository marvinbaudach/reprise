use rusqlite::{params, Connection, OptionalExtension};

use super::ProviderKind;

pub(crate) const RESOLUTION_RETRY_SECONDS: i64 = 7 * 24 * 60 * 60;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StoredOutcome {
    Ok,
    Unmatched,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StoredResolution {
    pub provider: Option<ProviderKind>,
    pub provider_id: Option<String>,
    pub mbid_verified: bool,
    pub last_attempt_at: Option<i64>,
    pub outcome: Option<StoredOutcome>,
}

pub(crate) fn load(
    conn: &Connection,
    artist_key: &str,
) -> Result<Option<StoredResolution>, rusqlite::Error> {
    conn.query_row(
        "SELECT provider, provider_id, mbid_verified, last_attempt_at, last_outcome
         FROM concert_artists WHERE artist_key = ?1",
        [artist_key],
        |row| {
            let provider: Option<String> = row.get(0)?;
            let outcome: Option<String> = row.get(4)?;
            Ok(StoredResolution {
                provider: provider.as_deref().and_then(provider_kind),
                provider_id: row.get(1)?,
                mbid_verified: row.get::<_, i64>(2)? != 0,
                last_attempt_at: row.get(3)?,
                outcome: outcome.as_deref().and_then(stored_outcome),
            })
        },
    )
    .optional()
}

#[must_use]
pub(crate) fn negative_retry_blocked(stored: Option<&StoredResolution>, now: i64) -> bool {
    stored.is_some_and(|stored| {
        stored.outcome == Some(StoredOutcome::Unmatched)
            && stored.last_attempt_at.is_some_and(|attempt| {
                let elapsed = now.saturating_sub(attempt);
                (0..RESOLUTION_RETRY_SECONDS).contains(&elapsed)
            })
    })
}

pub(crate) struct LedgerArtist<'a> {
    pub key: &'a str,
    pub name: &'a str,
    pub mbid: Option<&'a str>,
    pub is_similar: bool,
    pub similar_to: Option<&'a str>,
}

pub(crate) struct ResolvedIdentity<'a> {
    pub provider: ProviderKind,
    pub provider_id: &'a str,
    pub mbid_verified: bool,
}

pub(crate) fn store_success(
    conn: &Connection,
    artist: &LedgerArtist<'_>,
    identity: &ResolvedIdentity<'_>,
    attempted_at: i64,
    events_found: usize,
) -> Result<(), rusqlite::Error> {
    conn.execute(
        "INSERT INTO concert_artists (
           artist_key, artist_name, artist_mbid, provider, provider_id,
           mbid_verified, is_similar, similar_to, last_attempt_at,
           last_outcome, events_found
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'ok', ?10)
         ON CONFLICT(artist_key) DO UPDATE SET
           artist_name = excluded.artist_name,
           artist_mbid = excluded.artist_mbid,
           provider = excluded.provider,
           provider_id = excluded.provider_id,
           mbid_verified = excluded.mbid_verified,
           is_similar = MIN(concert_artists.is_similar, excluded.is_similar),
           similar_to = CASE
             WHEN MIN(concert_artists.is_similar, excluded.is_similar) = 0
               THEN NULL
             ELSE excluded.similar_to
           END,
           last_attempt_at = excluded.last_attempt_at,
           last_outcome = 'ok',
           events_found = excluded.events_found",
        params![
            artist.key,
            artist.name,
            artist.mbid,
            identity.provider.to_string(),
            identity.provider_id,
            i64::from(identity.mbid_verified),
            i64::from(artist.is_similar),
            artist.similar_to,
            attempted_at,
            events_found as i64
        ],
    )?;
    Ok(())
}

pub(crate) fn store_unmatched(
    conn: &Connection,
    artist: &LedgerArtist<'_>,
    attempted_at: i64,
) -> Result<(), rusqlite::Error> {
    conn.execute(
        "INSERT INTO concert_artists (
           artist_key, artist_name, artist_mbid, is_similar, similar_to,
           last_attempt_at, last_outcome, events_found
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'unmatched', 0)
         ON CONFLICT(artist_key) DO UPDATE SET
           artist_name = excluded.artist_name,
           artist_mbid = excluded.artist_mbid,
           provider = NULL,
           provider_id = NULL,
           mbid_verified = 0,
           is_similar = MIN(concert_artists.is_similar, excluded.is_similar),
           similar_to = CASE
             WHEN MIN(concert_artists.is_similar, excluded.is_similar) = 0
               THEN NULL
             ELSE excluded.similar_to
           END,
           last_attempt_at = excluded.last_attempt_at,
           last_outcome = 'unmatched',
           events_found = 0",
        params![
            artist.key,
            artist.name,
            artist.mbid,
            i64::from(artist.is_similar),
            artist.similar_to,
            attempted_at
        ],
    )?;
    Ok(())
}

pub(crate) fn store_failed(
    conn: &Connection,
    artist: &LedgerArtist<'_>,
    attempted_at: i64,
) -> Result<(), rusqlite::Error> {
    conn.execute(
        "INSERT INTO concert_artists (
           artist_key, artist_name, artist_mbid, is_similar, similar_to,
           last_attempt_at, last_outcome, events_found
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'failed', 0)
         ON CONFLICT(artist_key) DO UPDATE SET
           artist_name = excluded.artist_name,
           artist_mbid = excluded.artist_mbid,
           is_similar = MIN(concert_artists.is_similar, excluded.is_similar),
           similar_to = CASE
             WHEN MIN(concert_artists.is_similar, excluded.is_similar) = 0
               THEN NULL
             ELSE excluded.similar_to
           END,
           last_attempt_at = excluded.last_attempt_at,
           last_outcome = 'failed'",
        params![
            artist.key,
            artist.name,
            artist.mbid,
            i64::from(artist.is_similar),
            artist.similar_to,
            attempted_at
        ],
    )?;
    Ok(())
}

fn provider_kind(value: &str) -> Option<ProviderKind> {
    match value {
        "bandsintown" => Some(ProviderKind::Bandsintown),
        "ticketmaster" => Some(ProviderKind::Ticketmaster),
        _ => None,
    }
}

fn stored_outcome(value: &str) -> Option<StoredOutcome> {
    match value {
        "ok" => Some(StoredOutcome::Ok),
        "unmatched" => Some(StoredOutcome::Unmatched),
        "failed" => Some(StoredOutcome::Failed),
        _ => None,
    }
}
