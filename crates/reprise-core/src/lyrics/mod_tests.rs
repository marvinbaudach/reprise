use std::cell::Cell;
use std::path::Path;

use tempfile::TempDir;

use super::*;
use crate::library::source::ExistingPathSource;

struct FixedProvider {
    source: LyricsSource,
    outcome: SourceOutcome,
    calls: Cell<usize>,
}

impl FixedProvider {
    fn new(source: LyricsSource, outcome: SourceOutcome) -> Self {
        Self {
            source,
            outcome,
            calls: Cell::new(0),
        }
    }
}

impl LyricsProvider for FixedProvider {
    fn source(&self) -> LyricsSource {
        self.source
    }

    fn lookup(&self, _query: &LyricsQuery, _track_path: Option<&Path>) -> SourceOutcome {
        self.calls.set(self.calls.get() + 1);
        self.outcome.clone()
    }
}

fn query() -> LyricsQuery {
    LyricsQuery {
        title: "Synthetic Song".into(),
        artist: "Example Artist".into(),
        album: "Test Album".into(),
        duration_ms: 180_000,
    }
}

fn hit(source: LyricsSource, body: LyricsBody) -> SourceOutcome {
    SourceOutcome::Hit(LyricsHit { body, source })
}

fn options(force: bool) -> LookupOptions {
    LookupOptions {
        allow_network: true,
        force,
    }
}

#[test]
fn blank_required_metadata_is_rejected_before_network_fetch() {
    let temp = TempDir::new().unwrap();
    let local = FixedProvider::new(LyricsSource::Tag, SourceOutcome::Skipped);
    let network = FixedProvider::new(LyricsSource::Lrclib, SourceOutcome::Failed);
    let mut missing = query();
    missing.artist = " ".into();

    assert_eq!(
        load_or_fetch_at(
            temp.path(),
            100,
            &missing,
            None,
            options(false),
            &[&local],
            &[&network],
        ),
        Err(LyricsError::MissingMetadata)
    );
    assert_eq!(network.calls.get(), 0);
}

#[test]
fn positive_cache_roundtrip_skips_network_and_preserves_source() {
    let temp = TempDir::new().unwrap();
    let local = FixedProvider::new(LyricsSource::Tag, SourceOutcome::Skipped);
    let lrclib = FixedProvider::new(
        LyricsSource::Lrclib,
        hit(
            LyricsSource::Lrclib,
            LyricsBody::Plain("cached fixture".into()),
        ),
    );
    let netease = FixedProvider::new(LyricsSource::Netease, SourceOutcome::NotFound);

    let first = load_or_fetch_at(
        temp.path(),
        100,
        &query(),
        None,
        options(false),
        &[&local],
        &[&lrclib, &netease],
    )
    .unwrap();
    let second = load_or_fetch_at(
        temp.path(),
        101,
        &query(),
        None,
        options(false),
        &[&local],
        &[&lrclib, &netease],
    )
    .unwrap();

    assert_eq!(first, second);
    assert_eq!(second.source, LyricsSource::Lrclib);
    assert_eq!(lrclib.calls.get(), 1);
    assert_eq!(netease.calls.get(), 1);
}

#[test]
fn incomplete_plain_cache_retries_for_synced_instead_of_becoming_permanent() {
    let temp = TempDir::new().unwrap();
    let cached_plain = LyricsHit {
        body: LyricsBody::Plain("cached fixture".into()),
        source: LyricsSource::Lrclib,
    };
    cache::write_found(temp.path(), 100, &query(), &cached_plain, false);
    let local = FixedProvider::new(LyricsSource::Tag, SourceOutcome::Skipped);
    let lrclib = FixedProvider::new(LyricsSource::Lrclib, SourceOutcome::NotFound);
    let synced = LyricsHit {
        body: LyricsBody::Synced(vec![TimedLine::new(1_000, "upgraded")]),
        source: LyricsSource::Netease,
    };
    let netease = FixedProvider::new(LyricsSource::Netease, SourceOutcome::Hit(synced.clone()));

    assert_eq!(
        load_or_fetch_at(
            temp.path(),
            101,
            &query(),
            None,
            options(false),
            &[&local],
            &[&lrclib, &netease],
        ),
        Ok(synced)
    );
    assert_eq!(lrclib.calls.get(), 1);
    assert_eq!(netease.calls.get(), 1);
}

#[test]
fn precomputed_classification_rechecks_a_cache_entry_that_changed_before_lookup() {
    let temp = TempDir::new().unwrap();
    let local = FixedProvider::new(LyricsSource::Tag, SourceOutcome::Skipped);
    let network = FixedProvider::new(LyricsSource::Lrclib, SourceOutcome::Failed);
    let decision = cache::decision_at(temp.path(), 99, &query());
    cache::write_not_found(temp.path(), 100, &query());

    let result = load_or_fetch_with_cache_context_at(
        temp.path(),
        101,
        &query(),
        None,
        options(false),
        Some(&decision),
        LookupProviders {
            source: &UnixLibrarySource,
            local: &[&local],
            network: &[&network],
        },
    );

    assert_eq!(result, Err(LyricsError::NotFound));
    assert_eq!(network.calls.get(), 0);
}

#[test]
fn precomputed_classification_rechecks_an_unchanged_record_after_its_ttl_expires() {
    let temp = TempDir::new().unwrap();
    let local = FixedProvider::new(LyricsSource::Tag, SourceOutcome::Skipped);
    let expected = LyricsHit {
        body: LyricsBody::Synced(vec![TimedLine::new(1_000, "fresh")]),
        source: LyricsSource::Lrclib,
    };
    let network = FixedProvider::new(LyricsSource::Lrclib, SourceOutcome::Hit(expected.clone()));
    cache::write_not_found(temp.path(), 100, &query());
    let decision = cache::decision_at(temp.path(), 100 + cache::NEGATIVE_TTL_SECONDS, &query());

    let result = load_or_fetch_with_cache_context_at(
        temp.path(),
        101 + cache::NEGATIVE_TTL_SECONDS,
        &query(),
        None,
        options(false),
        Some(&decision),
        LookupProviders {
            source: &UnixLibrarySource,
            local: &[&local],
            network: &[&network],
        },
    );

    assert_eq!(result, Ok(expected));
    assert_eq!(network.calls.get(), 1);
}

#[test]
fn attempted_plain_upgrade_is_throttled_even_when_one_provider_fails() {
    let temp = TempDir::new().unwrap();
    let cached_plain = LyricsHit {
        body: LyricsBody::Plain("cached fixture".into()),
        source: LyricsSource::Lrclib,
    };
    cache::write_found(temp.path(), 100, &query(), &cached_plain, false);
    let local = FixedProvider::new(LyricsSource::Tag, SourceOutcome::Skipped);
    let lrclib = FixedProvider::new(LyricsSource::Lrclib, SourceOutcome::NotFound);
    let netease = FixedProvider::new(LyricsSource::Netease, SourceOutcome::Failed);

    assert_eq!(
        load_or_fetch_at(
            temp.path(),
            101,
            &query(),
            None,
            options(false),
            &[&local],
            &[&lrclib, &netease],
        ),
        Ok(cached_plain)
    );
    assert_eq!(
        cache::needs_fetch_at(temp.path(), 102, &query()),
        NeedsFetch::Skip
    );
}

#[test]
fn all_network_not_found_writes_negative_cache_but_mixed_failure_does_not() {
    let consensus = TempDir::new().unwrap();
    let local = FixedProvider::new(LyricsSource::Tag, SourceOutcome::Skipped);
    let lrclib = FixedProvider::new(LyricsSource::Lrclib, SourceOutcome::NotFound);
    let netease = FixedProvider::new(LyricsSource::Netease, SourceOutcome::NotFound);
    assert_eq!(
        load_or_fetch_at(
            consensus.path(),
            100,
            &query(),
            None,
            options(false),
            &[&local],
            &[&lrclib, &netease],
        ),
        Err(LyricsError::NotFound)
    );
    assert_eq!(
        cache::needs_fetch_at(consensus.path(), 101, &query()),
        NeedsFetch::Skip
    );

    let mixed = TempDir::new().unwrap();
    let failed = FixedProvider::new(LyricsSource::Netease, SourceOutcome::Failed);
    assert_eq!(
        load_or_fetch_at(
            mixed.path(),
            100,
            &query(),
            None,
            options(false),
            &[&local],
            &[&lrclib, &failed],
        ),
        Err(LyricsError::Temporary)
    );
    assert_eq!(
        cache::needs_fetch_at(mixed.path(), 101, &query()),
        NeedsFetch::Fetch
    );
}

#[test]
fn forced_refresh_keeps_positive_cache_on_temporary_failure() {
    let temp = TempDir::new().unwrap();
    let local = FixedProvider::new(LyricsSource::Tag, SourceOutcome::Skipped);
    let found = FixedProvider::new(
        LyricsSource::Lrclib,
        hit(
            LyricsSource::Lrclib,
            LyricsBody::Plain("safe cached text".into()),
        ),
    );
    let not_found = FixedProvider::new(LyricsSource::Netease, SourceOutcome::NotFound);
    let cached = load_or_fetch_at(
        temp.path(),
        10,
        &query(),
        None,
        options(false),
        &[&local],
        &[&found, &not_found],
    )
    .unwrap();

    let failed = FixedProvider::new(LyricsSource::Lrclib, SourceOutcome::Failed);
    assert_eq!(
        load_or_fetch_at(
            temp.path(),
            20,
            &query(),
            None,
            options(true),
            &[&local],
            &[&failed],
        ),
        Ok(cached)
    );
}

#[test]
fn local_synced_hit_never_creates_a_cache_entry() {
    let temp = TempDir::new().unwrap();
    let local = FixedProvider::new(
        LyricsSource::Sidecar,
        hit(
            LyricsSource::Sidecar,
            LyricsBody::Synced(vec![TimedLine::new(1_000, "local")]),
        ),
    );
    let network = FixedProvider::new(LyricsSource::Lrclib, SourceOutcome::Failed);

    let result = load_or_fetch_at(
        temp.path(),
        100,
        &query(),
        Some(Path::new("/fixture/song.flac")),
        options(false),
        &[&local],
        &[&network],
    )
    .unwrap();

    assert_eq!(result.source, LyricsSource::Sidecar);
    assert!(!cache::cache_file(temp.path(), &query()).exists());
    assert_eq!(network.calls.get(), 0);
}

#[test]
fn a_network_lookup_reads_every_local_provider_exactly_once() {
    let temp = TempDir::new().unwrap();
    let local = FixedProvider::new(
        LyricsSource::Tag,
        hit(LyricsSource::Tag, LyricsBody::Plain("local text".into())),
    );
    let network = FixedProvider::new(LyricsSource::Lrclib, SourceOutcome::NotFound);

    let result = load_or_fetch_at(
        temp.path(),
        100,
        &query(),
        Some(Path::new("/fixture/song.flac")),
        options(false),
        &[&local],
        &[&network],
    )
    .unwrap();

    assert_eq!(result.source, LyricsSource::Tag);
    assert_eq!(
        local.calls.get(),
        1,
        "a sidecar read and a tag parse per track are too expensive to repeat"
    );
}

#[test]
fn local_only_lookup_returns_local_text_and_never_calls_network() {
    let temp = TempDir::new().unwrap();
    let local = FixedProvider::new(
        LyricsSource::Tag,
        hit(LyricsSource::Tag, LyricsBody::Plain("local text".into())),
    );
    let network = FixedProvider::new(LyricsSource::Lrclib, SourceOutcome::Failed);

    let result = load_or_fetch_at(
        temp.path(),
        100,
        &query(),
        None,
        LookupOptions {
            allow_network: false,
            force: false,
        },
        &[&local],
        &[&network],
    )
    .unwrap();

    assert_eq!(result.source, LyricsSource::Tag);
    assert_eq!(network.calls.get(), 0);
}

#[test]
fn lyr_7_a_synchronized_network_hit_writes_a_sidecar_beside_the_track() {
    let temp = TempDir::new().unwrap();
    let track = temp.path().join("song.flac");
    std::fs::write(&track, b"fixture").unwrap();
    let local = FixedProvider::new(LyricsSource::Tag, SourceOutcome::Skipped);
    let lines = vec![TimedLine::new(1_230, "Downloaded line")];
    let network = FixedProvider::new(
        LyricsSource::Lrclib,
        hit(LyricsSource::Lrclib, LyricsBody::Synced(lines.clone())),
    );

    let result = load_or_fetch_at(
        temp.path(),
        100,
        &query(),
        Some(&track),
        options(false),
        &[&local],
        &[&network],
    )
    .unwrap();

    assert_eq!(result.body, LyricsBody::Synced(lines.clone()));
    assert_eq!(
        parse_lrc(&std::fs::read_to_string(track.with_extension("lrc")).unwrap()),
        lines
    );
}

#[test]
fn lyr_7_plain_instrumental_and_local_hits_never_write_a_sidecar() {
    for body in [
        LyricsBody::Plain("Plain network text".into()),
        LyricsBody::Instrumental,
    ] {
        let temp = TempDir::new().unwrap();
        let track = temp.path().join("song.flac");
        std::fs::write(&track, b"fixture").unwrap();
        let local = FixedProvider::new(LyricsSource::Tag, SourceOutcome::Skipped);
        let network = FixedProvider::new(LyricsSource::Lrclib, hit(LyricsSource::Lrclib, body));

        load_or_fetch_at(
            temp.path(),
            100,
            &query(),
            Some(&track),
            options(false),
            &[&local],
            &[&network],
        )
        .unwrap();

        assert!(!track.with_extension("lrc").exists());
    }

    let temp = TempDir::new().unwrap();
    let track = temp.path().join("local.flac");
    std::fs::write(&track, b"fixture").unwrap();
    let local = FixedProvider::new(
        LyricsSource::Tag,
        hit(
            LyricsSource::Tag,
            LyricsBody::Synced(vec![TimedLine::new(1_000, "Local text")]),
        ),
    );

    load_or_fetch_at(
        temp.path(),
        100,
        &query(),
        Some(&track),
        options(false),
        &[&local],
        &[],
    )
    .unwrap();

    assert!(!track.with_extension("lrc").exists());
}

#[test]
fn lyr_7_a_network_hit_without_a_track_path_remains_cache_only() {
    let temp = TempDir::new().unwrap();
    let local = FixedProvider::new(LyricsSource::Tag, SourceOutcome::Skipped);
    let network = FixedProvider::new(
        LyricsSource::Lrclib,
        hit(
            LyricsSource::Lrclib,
            LyricsBody::Synced(vec![TimedLine::new(1_000, "Network text")]),
        ),
    );

    let result = load_or_fetch_at(
        temp.path(),
        100,
        &query(),
        None,
        options(false),
        &[&local],
        &[&network],
    );

    assert!(result.is_ok());
    assert!(cache::cache_file(temp.path(), &query()).exists());
}

#[test]
fn lyr_7_a_sidecar_write_failure_never_changes_the_lookup_result() {
    let temp = TempDir::new().unwrap();
    let cache_dir = temp.path().join("cache");
    std::fs::create_dir(&cache_dir).unwrap();
    let track = temp.path().join("missing-music/song.flac");
    let local = FixedProvider::new(LyricsSource::Tag, SourceOutcome::Skipped);
    let expected = LyricsHit {
        body: LyricsBody::Synced(vec![TimedLine::new(1_000, "Network text")]),
        source: LyricsSource::Lrclib,
    };
    let network = FixedProvider::new(LyricsSource::Lrclib, SourceOutcome::Hit(expected.clone()));

    let result = load_or_fetch_with_cache_context_at(
        &cache_dir,
        100,
        &query(),
        Some(&track),
        options(false),
        None,
        LookupProviders {
            source: &ExistingPathSource::FILE,
            local: &[&local],
            network: &[&network],
        },
    );

    assert_eq!(result, Ok(expected));
    assert!(cache::cache_file(&cache_dir, &query()).exists());
    assert!(!track.with_extension("lrc").exists());
}

#[test]
fn lyr_7_the_next_lookup_finds_the_written_sidecar_without_network() {
    let temp = TempDir::new().unwrap();
    let track = temp.path().join("song.flac");
    std::fs::write(&track, b"fixture").unwrap();
    let skipped_local = FixedProvider::new(LyricsSource::Tag, SourceOutcome::Skipped);
    let lines = vec![TimedLine::new(1_230, "Downloaded line")];
    let network = FixedProvider::new(
        LyricsSource::Lrclib,
        hit(LyricsSource::Lrclib, LyricsBody::Synced(lines.clone())),
    );
    load_or_fetch_at(
        temp.path(),
        100,
        &query(),
        Some(&track),
        options(false),
        &[&skipped_local],
        &[&network],
    )
    .unwrap();
    let local = LocalProvider {
        source: &crate::library::source::UnixLibrarySource,
    };

    let result = load_or_fetch_at(
        temp.path(),
        101,
        &query(),
        Some(&track),
        LookupOptions {
            allow_network: false,
            force: false,
        },
        &[&local],
        &[&network],
    )
    .unwrap();

    assert_eq!(
        result,
        LyricsHit {
            body: LyricsBody::Synced(lines),
            source: LyricsSource::Sidecar,
        }
    );
    assert_eq!(network.calls.get(), 1);
}
