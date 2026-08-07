use std::borrow::Cow;
use std::collections::HashMap;
use std::io::Cursor;
use std::path::Path;
use std::path::PathBuf;

use lofty::config::WriteOptions;
use lofty::id3::v2::{
    BinaryFrame, Frame, FrameId, Id3v2Tag, SyncTextContentType, SynchronizedTextFrame,
    TimestampFormat,
};
use lofty::prelude::TagExt;
use lofty::tag::{ItemKey, Tag, TagType};
use lofty::TextEncoding;
use tempfile::TempDir;

use super::*;
use crate::lyrics::{
    LyricsBody, LyricsProvider, LyricsQuery, LyricsSource, SourceOutcome, TimedLine,
};

fn query() -> LyricsQuery {
    LyricsQuery {
        title: "Synthetic Song".into(),
        artist: "Example Artist".into(),
        album: "Test Album".into(),
        duration_ms: 10_000,
    }
}

fn hit(outcome: SourceOutcome) -> (LyricsBody, LyricsSource) {
    let SourceOutcome::Hit(hit) = outcome else {
        panic!("expected a local lyrics hit");
    };
    (hit.body, hit.source)
}

fn local_provider() -> LocalProvider<'static> {
    LocalProvider {
        source: &crate::library::source::UnixLibrarySource,
    }
}

struct VecLibrarySource {
    track: PathBuf,
    content: HashMap<PathBuf, Vec<u8>>,
}

impl crate::library::source::LibrarySource for VecLibrarySource {
    fn residence_token(&self, _at: &Path) -> Option<i64> {
        None
    }

    fn mount_point(&self, _at: &Path) -> Option<PathBuf> {
        None
    }

    fn display_name(&self, at: &Path) -> Option<String> {
        crate::library::source::UnixLibrarySource.display_name(at)
    }

    fn container_name(&self, at: &Path) -> Option<String> {
        crate::library::source::UnixLibrarySource.container_name(at)
    }

    fn relative_path(&self, root: &Path, at: &Path) -> Option<PathBuf> {
        crate::library::source::UnixLibrarySource.relative_path(root, at)
    }

    fn open_read(&self, at: &Path) -> std::io::Result<crate::library::source::LibraryReadHandle> {
        let bytes = self
            .content
            .get(at)
            .cloned()
            .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::NotFound))?;
        Ok(crate::library::source::LibraryReadHandle::new(Cursor::new(
            bytes,
        )))
    }

    fn probe(
        &self,
        at: &Path,
        _links: crate::library::source::LibraryLinkMode,
    ) -> crate::library::source::LibraryPathPresence {
        if at == self.track {
            crate::library::source::LibraryPathPresence::Present(
                crate::library::source::LibraryPathMetadata {
                    is_file: true,
                    is_directory: false,
                    size: None,
                    modified: None,
                    identity: None,
                },
            )
        } else {
            crate::library::source::LibraryPathPresence::Absent
        }
    }

    fn read_directory(
        &self,
        _directory: &Path,
    ) -> Option<Vec<crate::library::source::LibraryDirectoryEntry>> {
        None
    }

    fn walk(
        &self,
        _root: &Path,
        _order: crate::library::source::LibraryWalkOrder,
        _visitor: &mut dyn crate::library::source::LibraryWalkVisitor,
    ) {
    }
}

#[test]
fn sidecar_content_can_come_from_a_vec_backed_library_source() {
    let track = PathBuf::from("content:/music/song.flac");
    let sidecar = track.with_extension("lrc");
    let source = VecLibrarySource {
        track: track.clone(),
        content: HashMap::from([(sidecar, b"[00:01.25]memory line".to_vec())]),
    };
    let provider = LocalProvider { source: &source };

    assert_eq!(
        hit(provider.lookup(&query(), Some(&track))),
        (
            LyricsBody::Synced(vec![TimedLine::new(1_250, "memory line")]),
            LyricsSource::Sidecar,
        )
    );
}

#[test]
fn lyr_1_timestamped_sidecar_is_synchronized() {
    let temp = TempDir::new().unwrap();
    let track = temp.path().join("song.flac");
    std::fs::write(&track, b"fixture").unwrap();
    std::fs::write(track.with_extension("lrc"), "[00:01.25]first line").unwrap();

    assert_eq!(
        hit(local_provider().lookup(&query(), Some(&track))),
        (
            LyricsBody::Synced(vec![TimedLine::new(1_250, "first line")]),
            LyricsSource::Sidecar,
        )
    );
}

#[test]
fn sidecar_without_timestamps_is_plain() {
    let temp = TempDir::new().unwrap();
    let track = temp.path().join("song.flac");
    std::fs::write(&track, b"fixture").unwrap();
    std::fs::write(track.with_extension("lrc"), "plain sidecar").unwrap();

    assert_eq!(
        hit(local_provider().lookup(&query(), Some(&track))),
        (
            LyricsBody::Plain("plain sidecar".into()),
            LyricsSource::Sidecar,
        )
    );
}

#[test]
fn embedded_tag_text_is_plain() {
    let temp = TempDir::new().unwrap();
    let track = copy_flac(temp.path(), "tagged.flac");
    let mut tag = Tag::new(TagType::VorbisComments);
    tag.insert_text(ItemKey::Lyrics, "embedded lyrics".into());
    tag.save_to_path(&track, WriteOptions::default()).unwrap();

    assert_eq!(
        hit(local_provider().lookup(&query(), Some(&track))),
        (
            LyricsBody::Plain("embedded lyrics".into()),
            LyricsSource::Tag,
        )
    );
}

#[test]
fn embedded_sylt_is_synchronized() {
    let temp = TempDir::new().unwrap();
    let track = temp.path().join("tagged.wav");
    write_empty_wav(&track);
    let mut tag = Id3v2Tag::new();
    let synchronized = SynchronizedTextFrame::new(
        TextEncoding::UTF8,
        *b"eng",
        TimestampFormat::MS,
        SyncTextContentType::Lyrics,
        None,
        vec![(500, "first".into()), (1_500, "second".into())],
    );
    let bytes = synchronized.as_bytes(WriteOptions::default()).unwrap();
    tag.insert(Frame::Binary(BinaryFrame::new(
        FrameId::Valid(Cow::Borrowed("SYLT")),
        bytes,
    )));
    tag.save_to_path(&track, WriteOptions::default()).unwrap();

    assert_eq!(
        hit(local_provider().lookup(&query(), Some(&track))),
        (
            LyricsBody::Synced(vec![
                TimedLine::new(500, "first"),
                TimedLine::new(1_500, "second"),
            ]),
            LyricsSource::Tag,
        )
    );
}

#[test]
fn sidecar_has_precedence_over_embedded_tag() {
    let temp = TempDir::new().unwrap();
    let track = copy_flac(temp.path(), "both.flac");
    let mut tag = Tag::new(TagType::VorbisComments);
    tag.insert_text(ItemKey::Lyrics, "tag lyrics".into());
    tag.save_to_path(&track, WriteOptions::default()).unwrap();
    std::fs::write(track.with_extension("lrc"), "sidecar lyrics").unwrap();

    assert_eq!(
        hit(local_provider().lookup(&query(), Some(&track))),
        (
            LyricsBody::Plain("sidecar lyrics".into()),
            LyricsSource::Sidecar,
        )
    );
}

#[test]
fn missing_or_unreadable_track_is_skipped() {
    let temp = TempDir::new().unwrap();
    let missing = temp.path().join("missing.flac");
    assert_eq!(
        local_provider().lookup(&query(), Some(&missing)),
        SourceOutcome::Skipped
    );
    assert_eq!(
        local_provider().lookup(&query(), Some(temp.path())),
        SourceOutcome::Skipped
    );
    assert_eq!(
        local_provider().lookup(&query(), None),
        SourceOutcome::Skipped
    );
}

fn copy_flac(directory: &Path, name: &str) -> std::path::PathBuf {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sine.flac");
    let destination = directory.join(name);
    std::fs::copy(source, &destination).unwrap();
    destination
}

fn write_empty_wav(path: &Path) {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&38u32.to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&44_100u32.to_le_bytes());
    bytes.extend_from_slice(&88_200u32.to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&2u32.to_le_bytes());
    bytes.extend_from_slice(&0i16.to_le_bytes());
    std::fs::write(path, bytes).unwrap();
}

/// The `Seek` half of the contract, which is the entire reason
/// [`LibraryReadHandle`] promises more than `Read`: lofty's tag parser seeks.
/// The sidecar test above only exercises `Read`, so without this one the
/// costlier half of the promise stands unproven — and a source that could only
/// stream would look acceptable.
///
/// The bytes are a real tagged FLAC, but they reach the parser from a `Vec`
/// through a `Cursor`; nothing in this path opens a file.
#[test]
fn embedded_tags_can_be_parsed_from_a_vec_backed_library_source() {
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sine.flac");
    let bytes = std::fs::read(&fixture).unwrap();
    let track = PathBuf::from("content:/music/song.flac");
    let source = VecLibrarySource {
        track: track.clone(),
        content: HashMap::from([(track.clone(), bytes)]),
    };

    let mut handle = crate::library::source::LibrarySource::open_read(&source, &track).unwrap();
    let probe = lofty::probe::Probe::new(&mut handle)
        .guess_file_type()
        .expect("a seekable handle must let lofty sniff the header");

    assert_eq!(
        probe.file_type(),
        Some(lofty::file::FileType::Flac),
        "the parser must reach the same verdict it would from a real file"
    );
}
