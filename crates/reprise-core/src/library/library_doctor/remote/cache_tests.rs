use std::cell::Cell;
use std::rc::Rc;

use super::*;
use crate::library::library_doctor::ScanControl;

struct DirectProvider {
    calls: Rc<Cell<usize>>,
    result: Vec<RemoteIdentity>,
}

struct ErrorProvider {
    calls: Rc<Cell<usize>>,
}

impl RemoteProvider for ErrorProvider {
    fn direct(
        &mut self,
        _: &RemoteDirectLookup,
        _: &mut dyn FnMut() -> ScanControl,
    ) -> RemoteProviderResult {
        self.calls.set(self.calls.get() + 1);
        Err(RemoteProviderError::InvalidResponse)
    }

    fn search_musicbrainz(
        &mut self,
        _: &RemoteTrackMetadata,
        _: &mut dyn FnMut() -> ScanControl,
    ) -> RemoteProviderResult {
        Err(RemoteProviderError::InvalidResponse)
    }

    fn acoustid(
        &mut self,
        _: &RemoteTrackMetadata,
        _: &str,
        _: &str,
        _: u64,
        _: &mut dyn FnMut() -> ScanControl,
    ) -> RemoteProviderResult {
        Err(RemoteProviderError::InvalidResponse)
    }
}

impl RemoteProvider for DirectProvider {
    fn direct(
        &mut self,
        _: &RemoteDirectLookup,
        _: &mut dyn FnMut() -> ScanControl,
    ) -> RemoteProviderResult {
        self.calls.set(self.calls.get() + 1);
        Ok(self.result.clone())
    }

    fn search_musicbrainz(
        &mut self,
        _: &RemoteTrackMetadata,
        _: &mut dyn FnMut() -> ScanControl,
    ) -> RemoteProviderResult {
        Ok(Vec::new())
    }

    fn acoustid(
        &mut self,
        _: &RemoteTrackMetadata,
        _: &str,
        _: &str,
        _: u64,
        _: &mut dyn FnMut() -> ScanControl,
    ) -> RemoteProviderResult {
        self.calls.set(self.calls.get() + 1);
        Ok(self.result.clone())
    }
}

fn identity() -> RemoteIdentity {
    RemoteIdentity {
        source: RemoteEvidenceSource::MusicBrainz,
        confidence: 91,
        recording_mbid: Some("123e4567-e89b-12d3-a456-426614174000".into()),
        release_mbid: None,
        release_group_mbid: None,
        artist_mbid: None,
        release_artist_mbid: None,
        title: Some("Canonical title".into()),
        artist: Some("Canonical artist".into()),
        album: None,
        album_artist: None,
        release_year: None,
        original_release_year: None,
        duration_ms: Some(180_000),
    }
}

#[test]
fn doc_1c_complete_mbid_hit_survives_provider_restart_with_provenance() {
    let conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    let calls = Rc::new(Cell::new(0));
    let lookup = RemoteDirectLookup::Recording("123e4567-e89b-12d3-a456-426614174000".into());
    let expected = vec![identity()];
    let mut control = || ScanControl::Continue;

    {
        let upstream = DirectProvider {
            calls: calls.clone(),
            result: expected.clone(),
        };
        let mut provider = CachedRemoteProvider::new(upstream, &conn, 1_000);
        assert_eq!(provider.direct(&lookup, &mut control).unwrap(), expected);
    }
    {
        let upstream = DirectProvider {
            calls: calls.clone(),
            result: Vec::new(),
        };
        let mut provider = CachedRemoteProvider::new(upstream, &conn, 1_001);
        assert_eq!(provider.direct(&lookup, &mut control).unwrap(), expected);
    }

    assert_eq!(calls.get(), 1);
}

#[test]
fn doc_1c_acoustid_cache_key_includes_namespace_fingerprint_and_duration() {
    let conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    let calls = Rc::new(Cell::new(0));
    let expected = vec![identity()];
    let metadata = RemoteTrackMetadata {
        title: Some("Local title".into()),
        artist: None,
        album: None,
        album_artist: None,
        year: None,
        recording_mbid: None,
        release_mbid: None,
        release_group_mbid: None,
        artist_mbid: None,
        release_artist_mbid: None,
        duration_ms: Some(180_000),
    };
    let mut control = || ScanControl::Continue;

    {
        let upstream = DirectProvider {
            calls: calls.clone(),
            result: expected.clone(),
        };
        let mut provider = CachedRemoteProvider::new(upstream, &conn, 1_000);
        assert_eq!(
            provider
                .acoustid(
                    &metadata,
                    "chromaprint-v1",
                    "encoded-fingerprint",
                    180,
                    &mut control,
                )
                .unwrap(),
            expected
        );
    }
    {
        let upstream = DirectProvider {
            calls: calls.clone(),
            result: Vec::new(),
        };
        let mut provider = CachedRemoteProvider::new(upstream, &conn, 1_001);
        assert_eq!(
            provider
                .acoustid(
                    &metadata,
                    "chromaprint-v1",
                    "encoded-fingerprint",
                    180,
                    &mut control,
                )
                .unwrap(),
            expected
        );
    }

    for (namespace, fingerprint, duration) in [
        ("chromaprint-v2", "encoded-fingerprint", 180),
        ("chromaprint-v1", "different-fingerprint", 180),
        ("chromaprint-v1", "encoded-fingerprint", 181),
    ] {
        let upstream = DirectProvider {
            calls: calls.clone(),
            result: Vec::new(),
        };
        let mut provider = CachedRemoteProvider::new(upstream, &conn, 1_002);
        assert!(provider
            .acoustid(&metadata, namespace, fingerprint, duration, &mut control)
            .unwrap()
            .is_empty());
    }

    assert_eq!(calls.get(), 4);
}

#[test]
fn doc_1c_complete_and_negative_cache_entries_use_30_and_7_day_ttls() {
    const DAY: i64 = 24 * 60 * 60;

    let lookup = RemoteDirectLookup::Recording("123e4567-e89b-12d3-a456-426614174000".into());
    let mut control = || ScanControl::Continue;

    let positive_conn = crate::db::open(None).unwrap();
    crate::db::migrate(&positive_conn).unwrap();
    let positive_calls = Rc::new(Cell::new(0));
    let expected = vec![identity()];
    let mut first = CachedRemoteProvider::new(
        DirectProvider {
            calls: positive_calls.clone(),
            result: expected.clone(),
        },
        &positive_conn,
        1_000,
    );
    first.direct(&lookup, &mut control).unwrap();
    let mut before_expiry = CachedRemoteProvider::new(
        DirectProvider {
            calls: positive_calls.clone(),
            result: Vec::new(),
        },
        &positive_conn,
        1_000 + 30 * DAY - 1,
    );
    assert_eq!(
        before_expiry.direct(&lookup, &mut control).unwrap(),
        expected
    );
    let mut at_expiry = CachedRemoteProvider::new(
        DirectProvider {
            calls: positive_calls.clone(),
            result: Vec::new(),
        },
        &positive_conn,
        1_000 + 30 * DAY,
    );
    assert!(at_expiry.direct(&lookup, &mut control).unwrap().is_empty());
    assert_eq!(positive_calls.get(), 2);

    let negative_conn = crate::db::open(None).unwrap();
    crate::db::migrate(&negative_conn).unwrap();
    let negative_calls = Rc::new(Cell::new(0));
    let mut first = CachedRemoteProvider::new(
        DirectProvider {
            calls: negative_calls.clone(),
            result: Vec::new(),
        },
        &negative_conn,
        2_000,
    );
    first.direct(&lookup, &mut control).unwrap();
    let mut before_expiry = CachedRemoteProvider::new(
        DirectProvider {
            calls: negative_calls.clone(),
            result: expected.clone(),
        },
        &negative_conn,
        2_000 + 7 * DAY - 1,
    );
    assert!(before_expiry
        .direct(&lookup, &mut control)
        .unwrap()
        .is_empty());
    let mut at_expiry = CachedRemoteProvider::new(
        DirectProvider {
            calls: negative_calls.clone(),
            result: expected.clone(),
        },
        &negative_conn,
        2_000 + 7 * DAY,
    );
    assert_eq!(at_expiry.direct(&lookup, &mut control).unwrap(), expected);
    assert_eq!(negative_calls.get(), 2);
}

#[test]
fn doc_1c_incomplete_responses_are_never_cached() {
    let conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    let calls = Rc::new(Cell::new(0));
    let lookup = RemoteDirectLookup::Recording("123e4567-e89b-12d3-a456-426614174000".into());
    let mut control = || ScanControl::Continue;

    let mut incomplete = CachedRemoteProvider::new(
        ErrorProvider {
            calls: calls.clone(),
        },
        &conn,
        1_000,
    );
    assert_eq!(
        incomplete.direct(&lookup, &mut control),
        Err(RemoteProviderError::InvalidResponse)
    );

    let expected = vec![identity()];
    let mut complete = CachedRemoteProvider::new(
        DirectProvider {
            calls: calls.clone(),
            result: expected.clone(),
        },
        &conn,
        1_001,
    );
    assert_eq!(complete.direct(&lookup, &mut control).unwrap(), expected);
    assert_eq!(calls.get(), 2);
}
