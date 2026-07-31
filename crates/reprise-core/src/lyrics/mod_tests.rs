use std::cell::Cell;
use std::path::Path;

use tempfile::TempDir;

use super::*;

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
