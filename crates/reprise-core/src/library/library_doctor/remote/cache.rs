//! Persistent cache adapter for strongly identified remote lookups.

use rusqlite::Connection;

use super::album_match::AlbumQuery;
use super::{RemoteDirectLookup, RemoteProvider, RemoteProviderResult, RemoteTrackMetadata};
use crate::library::group_key::normalize_group_key;
use crate::library::library_doctor::ScanControl;

pub(crate) struct CachedRemoteProvider<'connection, P> {
    upstream: P,
    conn: &'connection Connection,
    now: i64,
}

const COMPLETE_TTL_SECONDS: i64 = 30 * 24 * 60 * 60;
const AMBIGUOUS_TTL_SECONDS: i64 = 7 * 24 * 60 * 60;

impl<'connection, P> CachedRemoteProvider<'connection, P> {
    pub(crate) fn new(upstream: P, conn: &'connection Connection, now: i64) -> Self {
        Self {
            upstream,
            conn,
            now,
        }
    }

    fn cached(&self, key: &str) -> Option<Vec<super::RemoteIdentity>> {
        let result_json = self
            .conn
            .query_row(
                "SELECT result_json FROM library_doctor_remote_cache \
                 WHERE cache_key=?1 AND expires_at>?2",
                rusqlite::params![key, self.now],
                |row| row.get::<_, String>(0),
            )
            .ok()?;
        serde_json::from_str(&result_json).ok()
    }

    fn store(&self, key: &str, result: &[super::RemoteIdentity]) {
        let Ok(result_json) = serde_json::to_string(result) else {
            return;
        };
        let ttl = if result.len() == 1 {
            COMPLETE_TTL_SECONDS
        } else {
            AMBIGUOUS_TTL_SECONDS
        };
        let _ = self.conn.execute(
            "INSERT INTO library_doctor_remote_cache \
             (cache_key, fetched_at, expires_at, result_json) VALUES (?1, ?2, ?3, ?4) \
             ON CONFLICT(cache_key) DO UPDATE SET \
             fetched_at=excluded.fetched_at, expires_at=excluded.expires_at, \
             result_json=excluded.result_json",
            rusqlite::params![key, self.now, self.now.saturating_add(ttl), result_json],
        );
    }
}

impl<P: RemoteProvider> RemoteProvider for CachedRemoteProvider<'_, P> {
    fn direct(
        &mut self,
        lookup: &RemoteDirectLookup,
        control: &mut dyn FnMut() -> ScanControl,
    ) -> RemoteProviderResult {
        let key = direct_cache_key(lookup);
        if let Some(result) = self.cached(&key) {
            return Ok(result);
        }
        let result = self.upstream.direct(lookup, control);
        if let Ok(identities) = &result {
            self.store(&key, identities);
        }
        result
    }

    fn search_musicbrainz(
        &mut self,
        metadata: &RemoteTrackMetadata,
        control: &mut dyn FnMut() -> ScanControl,
    ) -> RemoteProviderResult {
        let key = recording_search_cache_key(metadata);
        if let Some(result) = self.cached(&key) {
            return Ok(result);
        }
        let result = self.upstream.search_musicbrainz(metadata, control);
        if let Ok(identities) = &result {
            self.store(&key, identities);
        }
        result
    }

    fn search_release(
        &mut self,
        query: &AlbumQuery,
        control: &mut dyn FnMut() -> ScanControl,
    ) -> RemoteProviderResult {
        let key = release_search_cache_key(query);
        if let Some(result) = self.cached(&key) {
            return Ok(result);
        }
        let result = self.upstream.search_release(query, control);
        if let Ok(identities) = &result {
            self.store(&key, identities);
        }
        result
    }

    fn acoustid(
        &mut self,
        metadata: &RemoteTrackMetadata,
        fingerprint_namespace: &str,
        fingerprint: &str,
        duration_seconds: u64,
        control: &mut dyn FnMut() -> ScanControl,
    ) -> RemoteProviderResult {
        let key = fingerprint_cache_key(fingerprint_namespace, fingerprint, duration_seconds);
        if let Some(result) = self.cached(&key) {
            return Ok(result);
        }
        let result = self.upstream.acoustid(
            metadata,
            fingerprint_namespace,
            fingerprint,
            duration_seconds,
            control,
        );
        if let Ok(identities) = &result {
            self.store(&key, identities);
        }
        result
    }
}

fn direct_cache_key(lookup: &RemoteDirectLookup) -> String {
    let (kind, id) = match lookup {
        RemoteDirectLookup::Recording(id) => ("recording", id),
        RemoteDirectLookup::Release(id) => ("release", id),
        RemoteDirectLookup::ReleaseGroup(id) => ("release_group", id),
        RemoteDirectLookup::Artist(id) => ("artist", id),
        RemoteDirectLookup::ReleaseArtist(id) => ("release_artist", id),
    };
    format!("musicbrainz:{kind}:{id}")
}

fn recording_search_cache_key(metadata: &RemoteTrackMetadata) -> String {
    format!(
        "musicbrainz:v2:recording_search:{}\u{1}{}\u{1}{}",
        normalize_group_key(metadata.lookup_title().unwrap_or_default()),
        normalize_group_key(metadata.lookup_artist().unwrap_or_default()),
        normalize_group_key(metadata.lookup_album().unwrap_or_default()),
    )
}

fn release_search_cache_key(query: &AlbumQuery) -> String {
    format!(
        "musicbrainz:v2:release_search:{}\u{1}{}\u{1}{}",
        normalize_group_key(&query.album_artist),
        normalize_group_key(&query.album),
        query.track_count,
    )
}

fn fingerprint_cache_key(namespace: &str, fingerprint: &str, duration_seconds: u64) -> String {
    format!(
        "acoustid:{}:{namespace}:{duration_seconds}:{}:{fingerprint}",
        namespace.len(),
        fingerprint.len()
    )
}
