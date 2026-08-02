use std::io::Read;
use std::path::Path;

use lofty::config::ParseOptions;
use lofty::file::{AudioFile, FileType, TaggedFileExt};
use lofty::id3::v2::{Frame, Id3v2Tag, SyncTextContentType, TimestampFormat};
use lofty::tag::ItemKey;

use super::{
    parse_lrc, LyricsBody, LyricsHit, LyricsProvider, LyricsQuery, LyricsSource, SourceOutcome,
    TimedLine,
};

pub struct LocalProvider<'a> {
    pub(crate) source: &'a dyn crate::library::source::LibrarySource,
}

impl LyricsProvider for LocalProvider<'_> {
    fn source(&self) -> LyricsSource {
        LyricsSource::Tag
    }

    fn lookup(&self, _query: &LyricsQuery, track_path: Option<&Path>) -> SourceOutcome {
        let Some(path) = track_path.filter(|path| {
            self.source
                .probe(path, crate::library::source::LibraryLinkMode::Follow)
                .is_some_and(|metadata| metadata.is_file)
        }) else {
            return SourceOutcome::Skipped;
        };
        local_hit_with_source(self.source, path).map_or(SourceOutcome::Skipped, SourceOutcome::Hit)
    }
}

pub fn local_hit(path: &Path) -> Option<LyricsHit> {
    local_hit_with_source(&crate::library::source::UnixLibrarySource, path)
}

pub fn local_hit_with_source(
    source: &dyn crate::library::source::LibrarySource,
    path: &Path,
) -> Option<LyricsHit> {
    sidecar_hit(source, path).or_else(|| tag_hit(source, path))
}

fn sidecar_hit(
    source: &dyn crate::library::source::LibrarySource,
    track_path: &Path,
) -> Option<LyricsHit> {
    let mut reader = source.open_read(&track_path.with_extension("lrc")).ok()?;
    let mut text = String::new();
    reader.read_to_string(&mut text).ok()?;
    body_from_text(&text).map(|body| LyricsHit {
        body,
        source: LyricsSource::Sidecar,
    })
}

fn tag_hit(
    source: &dyn crate::library::source::LibrarySource,
    track_path: &Path,
) -> Option<LyricsHit> {
    if let Some(body) = synced_id3_from_source(source, track_path) {
        return Some(LyricsHit {
            body,
            source: LyricsSource::Tag,
        });
    }
    let file_type = FileType::from_path(track_path)?;
    let reader = source.open_read(track_path).ok()?;
    let tagged = lofty::probe::Probe::with_file_type(reader, file_type)
        .read()
        .ok()?;
    for tag in tagged.tags() {
        if let Some(text) = tag.get_string(ItemKey::Lyrics) {
            if let Some(body) = body_from_text(text) {
                return Some(LyricsHit {
                    body,
                    source: LyricsSource::Tag,
                });
            }
        }
        if let Some(text) = tag.get_string(ItemKey::UnsyncLyrics) {
            let text = text.trim();
            if !text.is_empty() {
                return Some(LyricsHit {
                    body: LyricsBody::Plain(text.to_string()),
                    source: LyricsSource::Tag,
                });
            }
        }
    }
    None
}

fn synced_id3_from_source(
    source: &dyn crate::library::source::LibrarySource,
    path: &Path,
) -> Option<LyricsBody> {
    // Seed the extension guess before sniffing, which is what the previous
    // `Probe::open(path)` did for free. `guess_file_type` is
    // `self.f_ty = sniffed.or(self.f_ty)`, so an unseeded probe turns a failed
    // sniff into "unknown format" instead of falling back to the extension —
    // and lofty documents sniffing as failing on legitimate files whose header
    // sits past its junk-byte budget. Without this seed a `.mp3` with a long
    // preamble silently loses its synced (`SYLT`) lyrics: the caller falls
    // through to the generic tag path, which never looks at `SYLT` at all.
    let probe = lofty::probe::Probe::new(source.open_read(path).ok()?)
        .set_file_type(FileType::from_path(path)?)
        .guess_file_type()
        .ok()?;
    let file_type = probe.file_type()?;
    let mut reader = probe.into_inner();
    let options = ParseOptions::new();
    match file_type {
        FileType::Aac => {
            let parsed = lofty::aac::AacFile::read_from(&mut reader, options).ok()?;
            parsed.id3v2().and_then(synced_id3_body)
        }
        FileType::Aiff => {
            let parsed = lofty::iff::aiff::AiffFile::read_from(&mut reader, options).ok()?;
            parsed.id3v2().and_then(synced_id3_body)
        }
        FileType::Mpeg => {
            let parsed = lofty::mpeg::MpegFile::read_from(&mut reader, options).ok()?;
            parsed.id3v2().and_then(synced_id3_body)
        }
        FileType::Wav => {
            let parsed = lofty::iff::wav::WavFile::read_from(&mut reader, options).ok()?;
            parsed.id3v2().and_then(synced_id3_body)
        }
        _ => None,
    }
}

fn synced_id3_body(id3: &Id3v2Tag) -> Option<LyricsBody> {
    let mut lines = Vec::new();
    for frame in id3 {
        let Frame::Binary(binary) = frame else {
            continue;
        };
        if binary.id().as_str() != "SYLT" {
            continue;
        }
        let Ok(text) =
            lofty::id3::v2::SynchronizedTextFrame::parse(binary.data.as_ref(), binary.flags())
        else {
            continue;
        };
        if text.content_type != SyncTextContentType::Lyrics
            || text.timestamp_format != TimestampFormat::MS
        {
            continue;
        }
        lines.extend(
            text.content.iter().map(|(start_ms, text)| {
                TimedLine::new(i64::from(*start_ms), text.trim().to_string())
            }),
        );
    }
    lines.sort_by_key(|line| line.start_ms);
    (!lines.is_empty()).then_some(LyricsBody::Synced(lines))
}

fn body_from_text(text: &str) -> Option<LyricsBody> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let synced = parse_lrc(text);
    if synced.is_empty() {
        Some(LyricsBody::Plain(text.to_string()))
    } else {
        Some(LyricsBody::Synced(synced))
    }
}

#[cfg(test)]
#[path = "local_tests.rs"]
mod tests;
