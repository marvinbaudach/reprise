use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::source_error::{SourceError, SourceErrorKind};

use super::{collapse_whitespace, rounded_duration_seconds};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LyricsQuery {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_ms: i64,
}

impl LyricsQuery {
    pub(crate) fn canonical(&self) -> Self {
        Self {
            title: collapse_whitespace(&self.title),
            artist: collapse_whitespace(&self.artist),
            album: collapse_whitespace(&self.album),
            duration_ms: self.duration_ms.max(0),
        }
    }

    pub(crate) fn cache_identity(&self) -> String {
        let query = self.canonical();
        format!(
            "{}\u{1f}{}\u{1f}{}\u{1f}{}",
            query.artist.to_lowercase(),
            query.title.to_lowercase(),
            query.album.to_lowercase(),
            rounded_duration_seconds(query.duration_ms)
        )
    }

    pub(crate) fn has_required_metadata(&self) -> bool {
        !self.title.trim().is_empty() && !self.artist.trim().is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimedLine {
    pub start_ms: i64,
    pub text: String,
}

impl TimedLine {
    pub fn new(start_ms: i64, text: impl Into<String>) -> Self {
        Self {
            start_ms,
            text: text.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LyricsBody {
    Synced(Vec<TimedLine>),
    Plain(String),
    Instrumental,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LyricsSource {
    Tag,
    Sidecar,
    Lrclib,
    Netease,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LyricsHit {
    pub body: LyricsBody,
    pub source: LyricsSource,
}

pub type SourceHit = LyricsHit;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceOutcome {
    Hit(SourceHit),
    NotFound,
    Skipped,
    Failed,
}

pub trait LyricsProvider {
    fn source(&self) -> LyricsSource;

    fn lookup(&self, query: &LyricsQuery, track_path: Option<&Path>) -> SourceOutcome;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum LyricsError {
    #[error("track title and artist are required for a lyrics lookup")]
    MissingMetadata,
    #[error("no lyrics were found")]
    NotFound,
    #[error("the lyrics service is temporarily unavailable")]
    Temporary,
    #[error("the lyrics service returned an invalid response")]
    InvalidResponse,
}

impl From<&LyricsError> for SourceErrorKind {
    fn from(_error: &LyricsError) -> Self {
        Self::Unreachable
    }
}

impl From<LyricsError> for SourceError {
    fn from(error: LyricsError) -> Self {
        let kind = SourceErrorKind::from(&error);
        Self::new(kind, "lyrics request failed", error.to_string())
    }
}

#[cfg(test)]
#[path = "model_tests.rs"]
mod tests;
