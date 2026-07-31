use std::cell::Cell;
use std::path::Path;

use super::*;
use crate::lyrics::{
    LyricsBody, LyricsError, LyricsHit, LyricsProvider, LyricsQuery, LyricsSource, SourceOutcome,
    TimedLine,
};

struct FakeProvider {
    source: LyricsSource,
    outcome: SourceOutcome,
    calls: Cell<usize>,
}

impl FakeProvider {
    fn new(source: LyricsSource, outcome: SourceOutcome) -> Self {
        Self {
            source,
            outcome,
            calls: Cell::new(0),
        }
    }
}

impl LyricsProvider for FakeProvider {
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

#[test]
fn lyr_5_synced_from_the_third_source_beats_plain_from_the_first() {
    let local = FakeProvider::new(
        LyricsSource::Tag,
        hit(LyricsSource::Tag, LyricsBody::Plain("tag text".into())),
    );
    let lrclib = FakeProvider::new(LyricsSource::Lrclib, SourceOutcome::NotFound);
    let netease = FakeProvider::new(
        LyricsSource::Netease,
        hit(
            LyricsSource::Netease,
            LyricsBody::Synced(vec![TimedLine::new(1_000, "synced")]),
        ),
    );

    let report = run_chain(&query(), None, &[&local], &[&lrclib, &netease]);

    assert_eq!(
        report.result,
        Ok(LyricsHit {
            body: LyricsBody::Synced(vec![TimedLine::new(1_000, "synced")]),
            source: LyricsSource::Netease,
        })
    );
}

#[test]
fn first_plain_wins_when_no_provider_has_synced_lyrics() {
    let tag = FakeProvider::new(
        LyricsSource::Tag,
        hit(LyricsSource::Tag, LyricsBody::Plain("first".into())),
    );
    let lrclib = FakeProvider::new(
        LyricsSource::Lrclib,
        hit(LyricsSource::Lrclib, LyricsBody::Plain("second".into())),
    );

    let report = run_chain(&query(), None, &[&tag], &[&lrclib]);

    assert_eq!(
        report.result,
        Ok(LyricsHit {
            body: LyricsBody::Plain("first".into()),
            source: LyricsSource::Tag,
        })
    );
}

#[test]
fn instrumental_stops_the_chain_but_does_not_replace_local_text() {
    let tag = FakeProvider::new(
        LyricsSource::Tag,
        hit(LyricsSource::Tag, LyricsBody::Plain("local".into())),
    );
    let lrclib = FakeProvider::new(
        LyricsSource::Lrclib,
        hit(LyricsSource::Lrclib, LyricsBody::Instrumental),
    );
    let netease = FakeProvider::new(
        LyricsSource::Netease,
        hit(
            LyricsSource::Netease,
            LyricsBody::Synced(vec![TimedLine::new(0, "must not run")]),
        ),
    );

    let report = run_chain(&query(), None, &[&tag], &[&lrclib, &netease]);

    assert_eq!(
        report.result,
        Ok(LyricsHit {
            body: LyricsBody::Plain("local".into()),
            source: LyricsSource::Tag,
        })
    );
    assert_eq!(netease.calls.get(), 0);
}

#[test]
fn all_skipped_or_failed_is_temporary_without_negative_consensus() {
    let local = FakeProvider::new(LyricsSource::Tag, SourceOutcome::Skipped);
    let lrclib = FakeProvider::new(LyricsSource::Lrclib, SourceOutcome::Failed);
    let netease = FakeProvider::new(LyricsSource::Netease, SourceOutcome::Skipped);

    let report = run_chain(&query(), None, &[&local], &[&lrclib, &netease]);

    assert_eq!(report.result, Err(LyricsError::Temporary));
    assert!(!report.network_consensus_not_found);
}

#[test]
fn every_network_provider_not_found_is_a_negative_consensus() {
    let local = FakeProvider::new(LyricsSource::Tag, SourceOutcome::Skipped);
    let lrclib = FakeProvider::new(LyricsSource::Lrclib, SourceOutcome::NotFound);
    let netease = FakeProvider::new(LyricsSource::Netease, SourceOutcome::NotFound);

    let report = run_chain(&query(), None, &[&local], &[&lrclib, &netease]);

    assert_eq!(report.result, Err(LyricsError::NotFound));
    assert!(report.network_consensus_not_found);
}

#[test]
fn mixed_not_found_and_failed_is_temporary_without_negative_consensus() {
    let local = FakeProvider::new(LyricsSource::Tag, SourceOutcome::Skipped);
    let lrclib = FakeProvider::new(LyricsSource::Lrclib, SourceOutcome::NotFound);
    let netease = FakeProvider::new(LyricsSource::Netease, SourceOutcome::Failed);

    let report = run_chain(&query(), None, &[&local], &[&lrclib, &netease]);

    assert_eq!(report.result, Err(LyricsError::Temporary));
    assert!(!report.network_consensus_not_found);
}
