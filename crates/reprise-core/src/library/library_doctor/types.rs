use std::path::PathBuf;

use crate::queries::BrowseFilter;
use crate::view_source::ViewSource;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoctorViewSnapshot {
    pub source: ViewSource,
    pub sort_field: String,
    pub sort_dir: String,
    pub filter: String,
    pub browse: BrowseFilter,
    pub queue_ids: Vec<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DoctorScopeRequest {
    WholeLibrary,
    CurrentView(Box<DoctorViewSnapshot>),
    Selection { track_ids: Vec<i64> },
}

impl DoctorScopeRequest {
    pub(crate) fn kind(&self) -> &'static str {
        match self {
            Self::WholeLibrary => "whole_library",
            Self::CurrentView(_) => "current_view",
            Self::Selection { .. } => "selection",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoctorTrackRef {
    pub track_id: i64,
    pub path: PathBuf,
    pub file_mtime: i64,
    pub file_size: i64,
    pub device: Option<i64>,
    pub inode: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoctorTrackSnapshot {
    pub reference: DoctorTrackRef,
    pub tags: Option<crate::library::tag_edit::EditableTags>,
    pub stale: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DoctorScanOptions {
    pub remote_enabled: bool,
}

impl DoctorScanOptions {
    pub const fn local_only() -> Self {
        Self {
            remote_enabled: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrozenScope {
    Tracks(Vec<DoctorTrackRef>),
    FallbackRequired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DoctorField {
    Title,
    Artist,
    Album,
    AlbumArtist,
    Year,
    Genre,
    RecordingMbid,
}

impl DoctorField {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Title => "title",
            Self::Artist => "artist",
            Self::Album => "album",
            Self::AlbumArtist => "album_artist",
            Self::Year => "year",
            Self::Genre => "genre",
            Self::RecordingMbid => "recording_mbid",
        }
    }

    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "title" => Some(Self::Title),
            "artist" => Some(Self::Artist),
            "album" => Some(Self::Album),
            "album_artist" => Some(Self::AlbumArtist),
            "year" => Some(Self::Year),
            "genre" => Some(Self::Genre),
            "recording_mbid" => Some(Self::RecordingMbid),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DoctorValue {
    Empty,
    Text(String),
    Year(u32),
}

impl DoctorValue {
    pub(crate) fn from_text(value: &str) -> Self {
        if value.is_empty() {
            Self::Empty
        } else {
            Self::Text(value.to_owned())
        }
    }

    pub(crate) fn encode(&self) -> Option<String> {
        match self {
            Self::Empty => None,
            Self::Text(value) => Some(value.clone()),
            Self::Year(value) => Some(value.to_string()),
        }
    }

    pub(crate) fn decode(field: DoctorField, value: Option<String>) -> Self {
        match (field, value) {
            (_, None) => Self::Empty,
            (DoctorField::Year, Some(value)) => {
                value.parse().map_or_else(|_| Self::Text(value), Self::Year)
            }
            (_, Some(value)) => Self::Text(value),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProposalSource {
    Local,
    MusicBrainz,
    AcoustId,
}

impl ProposalSource {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::MusicBrainz => "musicbrainz",
            Self::AcoustId => "acoustid",
        }
    }

    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "local" => Some(Self::Local),
            "musicbrainz" => Some(Self::MusicBrainz),
            "acoustid" => Some(Self::AcoustId),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProblemClass {
    CasingWhitespace,
    MissingAlbumArtist,
    GenreVariant,
    MissingWrongYear,
    MissingRecordingMbid,
}

impl ProblemClass {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::CasingWhitespace => "casing_whitespace",
            Self::MissingAlbumArtist => "missing_album_artist",
            Self::GenreVariant => "genre_variant",
            Self::MissingWrongYear => "missing_wrong_year",
            Self::MissingRecordingMbid => "missing_recording_mbid",
        }
    }

    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "casing_whitespace" => Some(Self::CasingWhitespace),
            "missing_album_artist" => Some(Self::MissingAlbumArtist),
            "genre_variant" => Some(Self::GenreVariant),
            "missing_wrong_year" => Some(Self::MissingWrongYear),
            "missing_recording_mbid" => Some(Self::MissingRecordingMbid),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoctorProposal {
    pub track_id: i64,
    pub field: DoctorField,
    pub current: DoctorValue,
    pub proposed: DoctorValue,
    pub source: ProposalSource,
    pub confidence: u8,
    pub preselected: bool,
    pub problem_class: ProblemClass,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoctorCandidate {
    pub value: DoctorValue,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoctorGroupMember {
    pub track_id: i64,
    pub current: DoctorValue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoctorUnresolvedGroup {
    pub field: DoctorField,
    pub group_key: String,
    pub candidates: Vec<DoctorCandidate>,
    pub members: Vec<DoctorGroupMember>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoctorScan {
    pub id: i64,
    pub scope_kind: String,
    pub created_at: i64,
    pub options: DoctorScanOptions,
    pub checked_tracks: usize,
    pub skipped_tracks: usize,
    pub track_ids: Vec<i64>,
    pub tracks: Vec<DoctorTrackSnapshot>,
    pub proposals: Vec<DoctorProposal>,
    pub unresolved_groups: Vec<DoctorUnresolvedGroup>,
}

impl DoctorScan {
    pub fn stale_track_ids(&self) -> Vec<i64> {
        self.tracks
            .iter()
            .filter(|track| track.stale)
            .map(|track| track.reference.track_id)
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalScanRequest {
    pub scope: DoctorScopeRequest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanControl {
    Continue,
    Cancel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DoctorScanProgress {
    pub completed_tracks: usize,
    pub total_tracks: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DoctorScanOutcome {
    Completed(DoctorScan),
    Cancelled { previous_scan_id: Option<i64> },
    ScopeFallbackRequired,
}

#[derive(Debug, thiserror::Error)]
pub enum DoctorError {
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("stored Library Doctor data is invalid: {0}")]
    InvalidStoredData(String),
}
