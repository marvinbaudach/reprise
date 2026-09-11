use std::cell::Cell;
use std::path::Path;

use lofty::config::WriteOptions;
use lofty::prelude::TagExt;
use lofty::tag::{ItemKey, Tag, TagType};
use tempfile::TempDir;

use super::*;

struct SyncedProvider {
    calls: Cell<usize>,
}

impl LyricsProvider for SyncedProvider {
    fn source(&self) -> LyricsSource {
        LyricsSource::Lrclib
    }

    fn lookup(&self, _query: &LyricsQuery, _track_path: Option<&Path>) -> SourceOutcome {
        self.calls.set(self.calls.get() + 1);
        SourceOutcome::Hit(LyricsHit {
            body: LyricsBody::Synced(vec![TimedLine::new(1_000, "different recording")]),
            source: LyricsSource::Lrclib,
        })
    }
}

fn fixture() -> (TempDir, std::path::PathBuf) {
    let temp = TempDir::new().unwrap();
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sine.flac");
    let track = temp.path().join("tagged.flac");
    std::fs::copy(source, &track).unwrap();
    let mut tag = Tag::new(TagType::VorbisComments);
    tag.insert_text(ItemKey::Lyrics, "curated tag lyrics".into());
    tag.save_to_path(&track, WriteOptions::default()).unwrap();
    (temp, track)
}

fn lookup(force: bool, temp: &TempDir, track: &Path, network: &dyn LyricsProvider) -> LyricsHit {
    let local = LocalProvider {
        source: &UnixLibrarySource,
    };
    load_or_fetch_at(
        temp.path(),
        100,
        &LyricsQuery {
            title: "Synthetic Song".into(),
            artist: "Example Artist".into(),
            album: "Test Album".into(),
            duration_ms: 180_000,
        },
        Some(track),
        LookupOptions {
            allow_network: true,
            force,
        },
        &[&local],
        &[network],
    )
    .unwrap()
}

#[test]
fn tag_lyrics_are_not_obscured_by_a_downloaded_synced_sidecar() {
    let (temp, track) = fixture();
    let network = SyncedProvider {
        calls: Cell::new(0),
    };

    let result = lookup(false, &temp, &track, &network);

    assert_eq!(
        result,
        LyricsHit {
            body: LyricsBody::Plain("curated tag lyrics".into()),
            source: LyricsSource::Tag,
        }
    );
    assert_eq!(network.calls.get(), 0);
    assert!(!track.with_extension("lrc").exists());
}

#[test]
fn forced_tag_lyrics_upgrade_reaches_network_and_writes_synced_sidecar() {
    let (temp, track) = fixture();
    let network = SyncedProvider {
        calls: Cell::new(0),
    };

    let result = lookup(true, &temp, &track, &network);

    assert!(matches!(result.body, LyricsBody::Synced(_)));
    assert_eq!(result.source, LyricsSource::Lrclib);
    assert_eq!(network.calls.get(), 1);
    assert!(track.with_extension("lrc").exists());
}
