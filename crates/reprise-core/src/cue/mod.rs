mod parse;
mod segments;
mod text;

pub use parse::parse;
pub use segments::{resolve_file, segments, CueSegment};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Frames(pub u32);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CueSheet {
    pub title: String,
    pub performer: String,
    pub date: Option<String>,
    pub genre: Option<String>,
    pub files: Vec<CueFile>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CueFile {
    pub name: String,
    pub tracks: Vec<CueTrack>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CueTrack {
    pub number: u32,
    pub title: String,
    pub performer: String,
    pub index00: Option<Frames>,
    pub index01: Frames,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum CueError {
    #[error("invalid CUE statement on line {line}: {statement}")]
    InvalidStatement { line: usize, statement: String },
    #[error("track {track} has no INDEX 01")]
    MissingIndex01 { track: u32 },
    #[error("track number {track} occurs more than once")]
    DuplicateTrackNumber { track: u32 },
    #[error("track {track} has an invalid index: {value}")]
    InvalidIndex { track: u32, value: String },
    #[error("track {track} has a non-monotonic index")]
    NonMonotonicIndex { track: u32 },
    #[error("no duration was supplied for CUE file {file}")]
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
