use super::*;

fn query() -> LyricsQuery {
    LyricsQuery {
        title: "  Synthetic   Song ".into(),
        artist: " Example Artist ".into(),
        album: " Test Album ".into(),
        duration_ms: 10_499,
    }
}

#[test]
fn canonical_identity_collapses_metadata_and_rounds_duration() {
    let canonical = query().canonical();

    assert_eq!(canonical.title, "Synthetic Song");
    assert_eq!(canonical.artist, "Example Artist");
    assert_eq!(canonical.album, "Test Album");
    assert_eq!(
        canonical.cache_identity(),
        "example artist\u{1f}synthetic song\u{1f}test album\u{1f}10"
    );
}

#[test]
fn lyrics_failures_expose_technical_context_only_through_details() {
    let error = SourceError::from(LyricsError::InvalidResponse);

    assert_eq!(error.kind(), &SourceErrorKind::Unreachable);
    assert!(!error.to_string().contains("invalid response"));
    assert!(error
        .details("2026-07-30 14:12")
        .to_string()
        .contains("lyrics service returned an invalid response"));
}

#[test]
fn lyrics_hit_keeps_body_and_provider_provenance_together() {
    let hit = LyricsHit {
        body: LyricsBody::Plain("fixture lyrics".into()),
        source: LyricsSource::Netease,
    };

    assert_eq!(hit.body, LyricsBody::Plain("fixture lyrics".into()));
    assert_eq!(hit.source, LyricsSource::Netease);
}
