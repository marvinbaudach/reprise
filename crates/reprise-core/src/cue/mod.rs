mod parse;
mod text;

pub use parse::parse;

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
}

#[cfg(test)]
mod parse_tests;
