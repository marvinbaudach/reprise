//! Fetch-scope configuration and candidate selection for a New Releases
//! refresh run. Split out of `artist_news.rs` purely to stay under the
//! project's 800-line rule; re-exported from there so existing callers keep
//! using `artist_news::{FetchScope, configured_fetch_scope, ...}`.

use rusqlite::Connection;

use crate::artist_news::normalize;

const FETCH_ALL_ARTISTS_KEY: &str = "module.new_releases.all_artists";
const INCLUDE_SINGLES_KEY: &str = "module.new_releases.include_singles";
const TOP_ARTIST_COUNT: usize = 20;
const REST_ARTISTS_PER_RUN: usize = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FetchScope {
    TopArtists,
    AllArtists,
}

pub fn configured_fetch_scope(conn: &Connection) -> Result<FetchScope, rusqlite::Error> {
    if crate::library::settings::get_bool(conn, FETCH_ALL_ARTISTS_KEY, false)? {
        Ok(FetchScope::AllArtists)
    } else {
        Ok(FetchScope::TopArtists)
    }
}

pub fn set_fetch_all_artists(conn: &Connection, all_artists: bool) -> Result<(), rusqlite::Error> {
    crate::library::settings::set_bool(conn, FETCH_ALL_ARTISTS_KEY, all_artists)
}

/// Whether already-released singles count as news. Off by default: singles
/// are the most common release type, so switching this on noticeably
/// increases how much the badge reports.
pub fn include_singles(conn: &Connection) -> Result<bool, rusqlite::Error> {
    crate::library::settings::get_bool(conn, INCLUDE_SINGLES_KEY, false)
}

pub fn set_include_singles(conn: &Connection, include: bool) -> Result<(), rusqlite::Error> {
    crate::library::settings::set_bool(conn, INCLUDE_SINGLES_KEY, include)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ArtistCandidate {
    pub(crate) name: String,
    pub(crate) mbid: Option<String>,
}

/// Candidates for this run: the `TOP_ARTIST_COUNT` most-played artists
/// always, plus — in `AllArtists` scope — the `REST_ARTISTS_PER_RUN` artists
/// that have gone longest without an attempt, never-checked ones first.
///
/// Ordering the tail by staleness rather than by a date-derived rotation
/// window is what lets an artist you own a single track of ever come up at
/// all: play count decides who is *preferred*, not who is *reachable*. A run
/// that never happens costs nothing now — the skipped artists are simply the
/// oldest next time.
pub(crate) fn artists_for_fetch(
    conn: &Connection,
    scope: FetchScope,
) -> Result<Vec<ArtistCandidate>, rusqlite::Error> {
    let mut statement = conn.prepare(
        "SELECT MIN(trim(artist)), MAX(artist_mbid), SUM(play_count) AS plays
         FROM tracks
         WHERE removed_at IS NULL AND missing_since IS NULL AND trim(artist) <> ''
         GROUP BY lower(trim(artist))
         HAVING MAX(artist_mbid) IS NOT NULL OR MAX(artist_mbid_negative) = 0
         ORDER BY plays DESC, lower(MIN(trim(artist))) ASC",
    )?;
    let mut candidates = statement
        .query_map([], |row| {
            Ok(ArtistCandidate {
                name: row.get(0)?,
                mbid: row.get(1)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    if candidates.len() <= TOP_ARTIST_COUNT {
        return Ok(candidates);
    }
    match scope {
        FetchScope::TopArtists => {
            candidates.truncate(TOP_ARTIST_COUNT);
            Ok(candidates)
        }
        FetchScope::AllArtists => {
            let mut rest = candidates.split_off(TOP_ARTIST_COUNT);
            // One query for every artist's last attempt instead of one point
            // query per rest candidate — most of which are immediately
            // discarded by the `take(REST_ARTISTS_PER_RUN)` below.
            let last_attempts = crate::artist_news_ledger::all_last_attempts(conn)?;
            let mut keyed = Vec::with_capacity(rest.len());
            for candidate in rest.drain(..) {
                let last_attempt = last_attempts.get(&normalize(&candidate.name)).copied();
                keyed.push((last_attempt, candidate));
            }
            // `None` sorts before `Some` — never-checked artists come first.
            keyed.sort_by_key(|(last_attempt, _)| *last_attempt);
            candidates.extend(
                keyed
                    .into_iter()
                    .take(REST_ARTISTS_PER_RUN)
                    .map(|(_, candidate)| candidate),
            );
            Ok(candidates)
        }
    }
}
