use std::cmp::Ordering;
use std::collections::BTreeMap;

use rusqlite::{Connection, OptionalExtension};

use super::normalize_component;

pub(crate) const MAX_ARTISTS_PER_RUN: usize = 30;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ArtistCandidate {
    pub key: String,
    pub name: String,
    pub mbid: Option<String>,
    pub plays: i64,
    pub last_attempt_at: Option<i64>,
    pub is_similar: bool,
    pub similar_to: Option<String>,
}

pub(crate) type SeedArtist = ArtistCandidate;

pub(crate) fn library_candidates(
    conn: &Connection,
    cutoff: i64,
) -> Result<Vec<ArtistCandidate>, rusqlite::Error> {
    let mut rows = aggregate_recent_artists(conn, cutoff)?;
    for candidate in &mut rows {
        candidate.last_attempt_at = conn
            .query_row(
                "SELECT last_attempt_at FROM concert_artists WHERE artist_key = ?1",
                [&candidate.key],
                |row| row.get(0),
            )
            .optional()?
            .flatten();
    }
    rows.sort_by(candidate_fetch_order);
    Ok(rows)
}

pub(crate) fn seed_artists(
    conn: &Connection,
    cutoff: i64,
    limit: usize,
) -> Result<Vec<SeedArtist>, rusqlite::Error> {
    let mut rows = aggregate_recent_artists(conn, cutoff)?;
    rows.sort_by(|left, right| {
        right
            .plays
            .cmp(&left.plays)
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });
    rows.truncate(limit);
    Ok(rows)
}

fn aggregate_recent_artists(
    conn: &Connection,
    cutoff: i64,
) -> Result<Vec<ArtistCandidate>, rusqlite::Error> {
    let mut statement = conn.prepare(
        "SELECT trim(artist), artist_mbid
         FROM listen_events
         WHERE played_at >= ?1 AND trim(artist) <> ''",
    )?;
    let listens = statement.query_map([cutoff], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
    })?;
    let mut aggregated: BTreeMap<String, ArtistCandidate> = BTreeMap::new();
    for listen in listens {
        let (name, mbid) = listen?;
        let key = normalize_component(&name);
        let entry = aggregated.entry(key.clone()).or_insert(ArtistCandidate {
            key,
            name: name.clone(),
            mbid: None,
            plays: 0,
            last_attempt_at: None,
            is_similar: false,
            similar_to: None,
        });
        entry.plays += 1;
        if name.to_lowercase() < entry.name.to_lowercase() {
            entry.name = name;
        }
        if mbid
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
            && mbid > entry.mbid
        {
            entry.mbid = mbid;
        }
    }
    Ok(aggregated.into_values().collect())
}

fn candidate_fetch_order(left: &ArtistCandidate, right: &ArtistCandidate) -> Ordering {
    match (left.last_attempt_at, right.last_attempt_at) {
        (None, Some(_)) => Ordering::Less,
        (Some(_), None) => Ordering::Greater,
        (Some(left), Some(right)) => left.cmp(&right),
        (None, None) => Ordering::Equal,
    }
    .then_with(|| right.plays.cmp(&left.plays))
    .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
}
