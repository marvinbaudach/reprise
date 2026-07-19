use super::*;

#[test]
fn acoustid_post_body_has_exact_privacy_allowlist() {
    let form = acoustid_form("secret-client", "secret-fingerprint", 181);
    let keys = form.iter().map(|(key, _)| key.as_str()).collect::<Vec<_>>();
    assert_eq!(
        keys,
        ["client", "format", "meta", "fingerprint", "duration"]
    );
    let serialized = serde_json::to_string(&form).unwrap();
    for forbidden in [
        "path", "filename", "track_id", "rating", "history", "playlist", "device", "root",
    ] {
        assert!(!serialized.contains(forbidden));
    }
    assert_eq!(ACOUSTID_ENDPOINT, "https://api.acoustid.org/v2/lookup");
}

#[test]
fn musicbrainz_lookup_uses_trimmed_allowlisted_text_without_changing_raw_metadata() {
    let metadata = RemoteTrackMetadata::from_actual_tags(
        "ignored.flac",
        "  Real title  ",
        " Real artist ",
        " Real album ",
        "",
        None,
        &Default::default(),
        Some(180_000),
    );
    let mut terms = Vec::new();
    push_term(&mut terms, "recording", metadata.lookup_title());
    push_term(&mut terms, "artist", metadata.lookup_artist());
    push_term(&mut terms, "release", metadata.lookup_album());

    assert_eq!(
        terms,
        [
            r#"recording:"Real title""#,
            r#"artist:"Real artist""#,
            r#"release:"Real album""#,
        ]
    );
    assert_eq!(metadata.title.as_deref(), Some("  Real title  "));
    assert_eq!(metadata.artist.as_deref(), Some(" Real artist "));
    assert_eq!(metadata.album.as_deref(), Some(" Real album "));
}

#[test]
fn source_rate_limits_match_service_contracts() {
    let now = Instant::now();
    assert_eq!(
        request_delay(
            Some(now - Duration::from_millis(250)),
            now,
            MUSICBRAINZ_INTERVAL
        ),
        Duration::from_millis(750)
    );
    assert_eq!(
        request_delay(
            Some(now - Duration::from_millis(100)),
            now,
            ACOUSTID_INTERVAL
        ),
        Duration::from_millis(234)
    );
}

#[test]
fn rate_limiter_reserves_three_concurrent_slots_monotonically() {
    let now = Instant::now();
    let mut reserved = None;
    let delays = (0..3)
        .map(|_| {
            let delay = request_delay(reserved, now, ACOUSTID_INTERVAL);
            reserved = Some(now + delay);
            delay
        })
        .collect::<Vec<_>>();
    assert_eq!(
        delays,
        [
            Duration::ZERO,
            Duration::from_millis(334),
            Duration::from_millis(668)
        ]
    );
}

#[test]
fn retry_after_accepts_delta_seconds_and_http_dates() {
    let now = std::time::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
    assert_eq!(parse_retry_after("12", now), Some(Duration::from_secs(12)));
    assert_eq!(
        parse_retry_after("Tue, 14 Nov 2023 22:13:25 GMT", now),
        Some(Duration::from_secs(5))
    );
    assert_eq!(parse_retry_after("private metadata", now), None);
}

#[test]
fn official_acoustid_shape_maps_recording_artists_groups_duration_and_releases() {
    let body = r#"{
      "status":"ok","results":[{"score":0.91,"recordings":[{
        "id":"123e4567-e89b-12d3-a456-426614174000","title":"Canonical title","duration":180.5,
        "artists":[{"id":"123e4567-e89b-12d3-a456-426614174003","name":"Canonical artist"}],
        "releasegroups":[{"id":"123e4567-e89b-12d3-a456-426614174002","title":"Canonical album",
          "artists":[{"id":"323e4567-e89b-12d3-a456-426614174003","name":"Album artist"}],
          "releases":[{"id":"123e4567-e89b-12d3-a456-426614174001","title":"Canonical album","date":"2024-03-02"}]
        }]
      }]}]
    }"#;
    let identities = parse_acoustid(body).unwrap();
    assert_eq!(identities.len(), 1);
    let value = &identities[0];
    assert_eq!(value.confidence, 91);
    assert_eq!(value.artist.as_deref(), Some("Canonical artist"));
    assert_eq!(value.album.as_deref(), Some("Canonical album"));
    assert_eq!(value.album_artist.as_deref(), Some("Album artist"));
    assert_eq!(value.release_year, Some(2024));
    assert_eq!(value.duration_ms, Some(180_500));
    assert!(value.release_mbid.is_some());
    assert!(value.release_group_mbid.is_some());
}

#[test]
fn bounded_retry_uses_three_attempts_and_honors_retry_after() {
    let limiter = Mutex::new(None);
    let mut calls = 0;
    let body = request_with_retry(
        &limiter,
        Duration::ZERO,
        &mut || ScanControl::Continue,
        |_| {
            calls += 1;
            Ok(if calls < 3 {
                HttpReply {
                    status: 429,
                    retry_after: Some(Duration::ZERO),
                    body: String::new(),
                }
            } else {
                HttpReply {
                    status: 200,
                    retry_after: None,
                    body: "ok".into(),
                }
            })
        },
    )
    .unwrap();
    assert_eq!((calls, body), (3, "ok".into()));
}

#[test]
fn auth_failure_opens_source_without_retry() {
    let limiter = Mutex::new(None);
    let mut calls = 0;
    let error = request_with_retry(
        &limiter,
        Duration::ZERO,
        &mut || ScanControl::Continue,
        |_| {
            calls += 1;
            Ok(HttpReply {
                status: 401,
                retry_after: None,
                body: "private response".into(),
            })
        },
    )
    .unwrap_err();
    assert_eq!(calls, 1);
    assert_eq!(error, RemoteProviderError::Unavailable);
    assert!(!error.to_string().contains("private response"));
}

#[test]
fn cancellation_interrupts_wait_and_backoff() {
    let mut checks = 0;
    let mut control = || {
        checks += 1;
        if checks >= 2 {
            ScanControl::Cancel
        } else {
            ScanControl::Continue
        }
    };
    assert_eq!(
        cancellable_sleep(Duration::from_millis(100), &mut control),
        Err(RemoteProviderError::Cancelled)
    );
}

#[test]
fn errors_never_include_keys_fingerprints_or_metadata() {
    for error in [
        RemoteProviderError::Unavailable,
        RemoteProviderError::Cancelled,
        RemoteProviderError::InvalidResponse,
    ] {
        let message = error.to_string();
        for secret in [
            "secret-client",
            "secret-fingerprint",
            "Real title",
            "https://",
        ] {
            assert!(!message.contains(secret));
        }
    }
}

#[test]
fn production_memoization_is_exact_and_does_not_cache_cancellation() {
    let mut provider = NetworkProvider::new();
    let mut calls = 0;
    let first = provider.memoized(NetworkSource::MusicBrainz, "exact-a".into(), || {
        calls += 1;
        Ok(Vec::new())
    });
    let second = provider.memoized(NetworkSource::MusicBrainz, "exact-a".into(), || {
        calls += 1;
        Ok(Vec::new())
    });
    provider
        .memoized(NetworkSource::MusicBrainz, "exact-b".into(), || {
            calls += 1;
            Err(RemoteProviderError::Cancelled)
        })
        .unwrap_err();
    provider
        .memoized(NetworkSource::MusicBrainz, "exact-b".into(), || {
            calls += 1;
            Ok(Vec::new())
        })
        .unwrap();
    assert_eq!((first, second), (Ok(Vec::new()), Ok(Vec::new())));
    assert_eq!(calls, 3);
}

#[test]
fn doc_1c_production_memoization_does_not_cache_incomplete_responses() {
    let mut provider = NetworkProvider::new();
    let mut calls = 0;
    assert_eq!(
        provider.memoized(NetworkSource::MusicBrainz, "incomplete".into(), || {
            calls += 1;
            Err(RemoteProviderError::InvalidResponse)
        }),
        Err(RemoteProviderError::InvalidResponse)
    );
    assert_eq!(
        provider.memoized(NetworkSource::MusicBrainz, "incomplete".into(), || {
            calls += 1;
            Ok(Vec::new())
        }),
        Ok(Vec::new())
    );
    assert_eq!(calls, 2);
}

#[test]
fn authentication_circuit_is_per_source() {
    let mut provider = NetworkProvider::new();
    let mut calls = 0;
    provider
        .memoized(NetworkSource::MusicBrainz, "mb-auth".into(), || {
            calls += 1;
            Err(RemoteProviderError::Unavailable)
        })
        .unwrap_err();
    provider
        .memoized(NetworkSource::MusicBrainz, "mb-skipped".into(), || {
            calls += 1;
            Ok(Vec::new())
        })
        .unwrap_err();
    provider
        .memoized(NetworkSource::AcoustId, "ac-still-open".into(), || {
            calls += 1;
            Ok(Vec::new())
        })
        .unwrap();
    assert_eq!(calls, 2);
}

#[test]
fn acoustid_parameter_errors_do_not_open_auth_circuit_but_invalid_key_does() {
    for code in [1, 2] {
        let mut provider = NetworkProvider::new();
        let mut calls = 0;
        let error = provider
            .memoized(NetworkSource::AcoustId, format!("invalid-{code}"), || {
                calls += 1;
                parse_acoustid(&format!(
                    r#"{{"status":"error","error":{{"code":{code}}}}}"#
                ))
            })
            .unwrap_err();
        assert_eq!(error, RemoteProviderError::InvalidResponse);
        provider
            .memoized(NetworkSource::AcoustId, format!("after-{code}"), || {
                calls += 1;
                Ok(Vec::new())
            })
            .unwrap();
        assert_eq!(calls, 2);
    }

    let mut provider = NetworkProvider::new();
    let mut calls = 0;
    let error = provider
        .memoized(NetworkSource::AcoustId, "invalid-key".into(), || {
            calls += 1;
            parse_acoustid(r#"{"status":"error","error":{"code":3}}"#)
        })
        .unwrap_err();
    assert_eq!(error, RemoteProviderError::Unavailable);
    assert_eq!(
        provider.memoized(NetworkSource::AcoustId, "after-key".into(), || {
            calls += 1;
            Ok(Vec::new())
        }),
        Err(RemoteProviderError::Unavailable)
    );
    assert_eq!(calls, 1);
}
