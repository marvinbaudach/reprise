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
        local_hit(path).map_or(SourceOutcome::Skipped, SourceOutcome::Hit)
    }
}

pub fn local_hit(path: &Path) -> Option<LyricsHit> {
    sidecar_hit(path).or_else(|| tag_hit(path))
}

fn sidecar_hit(track_path: &Path) -> Option<LyricsHit> {
    let text = std::fs::read_to_string(track_path.with_extension("lrc")).ok()?;
    body_from_text(&text).map(|body| LyricsHit {
        body,
        source: LyricsSource::Sidecar,
    })
}

fn tag_hit(track_path: &Path) -> Option<LyricsHit> {
    if let Some(body) = synced_id3_from_path(track_path) {
        return Some(LyricsHit {
            body,
            source: LyricsSource::Tag,
        });
    }
    let tagged = lofty::read_from_path(track_path).ok()?;
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

fn synced_id3_from_path(path: &Path) -> Option<LyricsBody> {
    let probe = lofty::probe::Probe::open(path)
        .ok()?
        .guess_file_type()
        .ok()?;
    let file_type = probe.file_type()?;
    let mut file = std::fs::File::open(path).ok()?;
    let options = ParseOptions::new();
    match file_type {
        FileType::Aac => {
            let parsed = lofty::aac::AacFile::read_from(&mut file, options).ok()?;
            parsed.id3v2().and_then(synced_id3_body)
        }
        FileType::Aiff => {
            let parsed = lofty::iff::aiff::AiffFile::read_from(&mut file, options).ok()?;
            parsed.id3v2().and_then(synced_id3_body)
        }
        FileType::Mpeg => {
            let parsed = lofty::mpeg::MpegFile::read_from(&mut file, options).ok()?;
            parsed.id3v2().and_then(synced_id3_body)
        }
        FileType::Wav => {
            let parsed = lofty::iff::wav::WavFile::read_from(&mut file, options).ok()?;
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
