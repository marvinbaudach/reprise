//! The single MusicBrainz-backed New Releases pipeline and its database query
//! layer. Network work is blocking and must be called from a worker thread.
//!
//! This module is the facade: fetch-scope/candidate selection lives in
//! `artist_news_candidates`, the refresh pipeline in `artist_news_pipeline`,
//! MusicBrainz JSON parsing and URL building in `artist_news_parsing`, and
//! the read-back query layer in `artist_news_query`. Everything is
//! re-exported here so existing callers keep resolving `artist_news::*`
//! paths unchanged.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NewsKind {
    Upcoming,
    New,
    Catalog,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlbumNews {
    pub release_group_mbid: String,
    pub title: String,
    pub first_release_date: String,
    pub primary_type: String,
    pub kind: NewsKind,
    pub announce_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtistNews {
    pub artist: String,
    pub artist_mbid: String,
    pub fetched_at: i64,
    pub items: Vec<AlbumNews>,
    pub stale: bool,
}

/// Fetch-scope configuration and candidate selection for a refresh run live
/// in `artist_news_candidates`; re-exported here so existing callers keep
/// using `artist_news::{FetchScope, configured_fetch_scope, ...}`.
pub use crate::artist_news_candidates::{
    configured_fetch_scope, include_singles, set_fetch_all_artists, set_include_singles, FetchScope,
};
// `artists_for_fetch` is called directly by `artist_news_pipeline` (not
// through this facade); the re-export below only exists so the test suite
// can keep resolving `artist_news::artists_for_fetch`.
#[cfg(test)]
pub(crate) use crate::artist_news_candidates::artists_for_fetch;

#[cfg(test)]
pub(crate) use crate::artist_news_pipeline::refresh_with;
/// The refresh pipeline that talks to MusicBrainz and writes the database
/// lives in `artist_news_pipeline`; re-exported here so existing callers
/// keep using `artist_news::{refresh, RefreshReport, NewsError}`.
pub use crate::artist_news_pipeline::{refresh, NewsError, RefreshReport};

pub(crate) use crate::artist_news_parsing::parse_partial_date;
/// MusicBrainz JSON parsing and the URL builders live in
/// `artist_news_parsing`; re-exported here so existing callers keep using
/// `artist_news::{parse_release_groups, ArtistMatch, ...}`.
pub use crate::artist_news_parsing::{
    artist_search_url, parse_artist_mbid, parse_release_group_page, parse_release_groups,
    parse_release_track_count, release_group_detail_url, release_groups_page_url,
    release_groups_url, ArtistMatch, ReleaseGroupPage,
};

pub(crate) use crate::artist_news_query::local_album_track_counts;
#[cfg(test)]
pub(crate) use crate::artist_news_query::presence_for;
/// The query layer that reads releases back out and annotates library
/// presence lives in `artist_news_query`; re-exported here so existing
/// callers keep using `artist_news::{query_releases, StoredRelease, ...}`.
pub use crate::artist_news_query::{
    hidden_release_count, mark_releases_seen, most_played_album_track_path, query_artist_news,
    query_artist_news_by_name, query_releases, set_release_hidden, unseen_release_count,
    LibraryPresence, StoredRelease,
};

/// Decisions and queries for the persistent Releases full view.
pub use crate::artist_news_view::{
    count_releases_view, filter_rows as filter_release_rows, persisted_releases_filter,
    query_releases_view, release_status, sort_rows as sort_release_rows, ReleaseSortDirection,
    ReleaseStatus, ReleaseTypeFilter, ReleasesFilter, RELEASES_FILTER_HIDDEN_KEY,
    RELEASES_FILTER_TYPE_KEY,
};

/// Staleness policy (when a refresh is due, the per-install jitter, and the
/// latest fetch timestamp) lives in `artist_news_refresh`; re-exported here
/// so existing callers keep using `artist_news::{refresh_due, jitter_seconds,
/// latest_fetched_at}`.
pub use crate::artist_news_refresh::{jitter_seconds, latest_fetched_at, refresh_due};

pub(crate) fn normalize(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}
