mod parse;
mod segments;
mod text;

pub use parse::parse;
pub use segments::{resolve_file, segments, CueSegment};

/// A position in CD frames: 75 per second, as written in `MM:SS:FF`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Frames(pub u32);

/// A parsed CUE sheet: album-level metadata and the files it describes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CueSheet {
    pub title: String,
    pub performer: String,
    pub date: Option<String>,
    pub genre: Option<String>,
    pub files: Vec<CueFile>,
}

/// One `FILE` block and the tracks that follow it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CueFile {
    pub name: String,
    pub tracks: Vec<CueTrack>,
}

/// One `TRACK`. `is_audio` is false for data tracks, which never become segments.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CueTrack {
    pub number: u32,
    pub is_audio: bool,
    pub title: String,
    pub performer: String,
    pub index00: Option<Frames>,
    pub index01: Frames,
}

/// Why a CUE sheet could not be parsed or turned into segments.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum CueError {
    #[error("CUE sheet contains no tracks")]
    EmptySheet,
    #[error("CUE sheet has no audio tracks")]
    NoAudioTracks,
    #[error("CUE sheet has invalid text encoding")]
    InvalidTextEncoding,
    #[error("invalid CUE statement on line {line}: {statement}")]
    InvalidStatement { line: usize, statement: String },
    #[error("track {track} has no INDEX 01")]
    MissingIndex01 { track: u32 },
    #[error("track number {track} occurs more than once")]
    DuplicateTrackNumber { track: u32 },
    #[error("track {track} has an invalid index: {value}")]
    InvalidIndex { track: u32, value: String },
    #[error("track {track} repeats INDEX {index:02}")]
    DuplicateIndex { track: u32, index: u32 },
    #[error("track {track} has a non-monotonic index")]
    NonMonotonicIndex { track: u32 },
    #[error("audio file {path:?} is referenced by more than one FILE block")]
    DuplicateFile { path: std::path::PathBuf },
    #[error("CUE file {file} has no resolved audio file with a duration")]
    MissingDuration { file: String },
    #[error("audio file {path:?} has an invalid duration of {duration_ms} ms")]
    InvalidDuration {
        path: std::path::PathBuf,
        duration_ms: i64,
    },
    #[error(
        "track {track} starts at {position_ms} ms, past the {duration_ms} ms duration of {path:?}"
    )]
    IndexPastEnd {
        path: std::path::PathBuf,
        track: u32,
        position_ms: i64,
        duration_ms: i64,
    },
}

#[cfg(test)]
mod parse_tests;
#[cfg(test)]
mod segments_tests;
