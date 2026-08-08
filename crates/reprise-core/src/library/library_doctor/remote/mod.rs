//! Deep remote-resolution boundary for Library Doctor.
//!
//! Scan orchestration knows only this module's request/result vocabulary. URL
//! shapes, cascade order, source arbitration, request deduplication and retry
//! policy stay below this seam.

mod acoustid;
#[allow(dead_code)] // MATCH-3 consumes the release matcher after its pure-function package lands.
mod album_match;
mod arbitration;
mod cache;
#[cfg(test)]
mod diagnostics;
pub(crate) mod guard_rails;
mod metadata;
mod network;
mod orchestrator;

#[cfg(test)]
mod album_match_tests;
#[cfg(test)]
mod cache_tests;
#[cfg(test)]
mod guard_rails_tests;

#[allow(unused_imports)] // MATCH-3 consumes these staged exports.
pub(crate) use album_match::{best_release, AlbumMatch, AlbumQuery};
pub(crate) use cache::CachedRemoteProvider;
pub(crate) use metadata::read_remote_metadata;
pub use metadata::{RemoteDirectLookup, RemoteTrackMetadata};
pub(crate) use network::{NetworkProvider, NoNetworkProvider};
pub(crate) use orchestrator::{ProviderRemoteResolver, RemoteResolver};
pub use orchestrator::{
    ReleaseSecondaryType, RemoteIdentity, RemoteProvider, RemoteProviderError,
    RemoteProviderResult, RemoteResolution,
};

use serde::{Deserialize, Serialize};

use super::DoctorField;

pub const REMOTE_WRITABLE_FIELDS: [DoctorField; 6] = [
    DoctorField::Title,
    DoctorField::Artist,
    DoctorField::Album,
    DoctorField::AlbumArtist,
    DoctorField::Year,
    DoctorField::RecordingMbid,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RemoteEvidenceSource {
    MusicBrainz,
    AcoustId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteEvidence {
    pub source: RemoteEvidenceSource,
    pub confidence: u8,
    pub recording_mbid: Option<String>,
    pub release_mbid: Option<String>,
    pub release_group_mbid: Option<String>,
    pub artist_mbid: Option<String>,
    #[serde(default)]
    pub release_artist_mbid: Option<String>,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub year: Option<u32>,
    pub duration_ms: Option<u64>,
    pub duration_delta_ms: Option<u64>,
}

#[cfg(test)]
pub(crate) use arbitration::arbitrate;
#[cfg(test)]
pub(crate) use orchestrator::resolve_with_provider;
