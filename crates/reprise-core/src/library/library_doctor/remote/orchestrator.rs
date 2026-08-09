use super::{
    arbitration, best_release, AlbumMatch, AlbumQuery, RemoteDirectLookup, RemoteEvidenceSource,
    RemoteTrackMetadata,
};
use crate::fingerprint::{
    FingerprintBackend, FingerprintControl, FingerprintOutcome, FingerprintProgress,
};
use crate::library::library_doctor::ScanControl;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReleaseSecondaryType {
    Compilation,
    DjMix,
    Live,
    Mixtape,
    Remix,
    Other(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteIdentity {
    pub source: RemoteEvidenceSource,
    pub confidence: u8,
    pub recording_mbid: Option<String>,
    pub release_mbid: Option<String>,
    pub release_group_mbid: Option<String>,
    pub artist_mbid: Option<String>,
    pub release_artist_mbid: Option<String>,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub album_artist: Option<String>,
    pub release_year: Option<u32>,
    pub original_release_year: Option<u32>,
    pub duration_ms: Option<u64>,
    pub secondary_types: Vec<ReleaseSecondaryType>,
    pub release_track_count: Option<u32>,
    pub release_track_titles: Vec<String>,
    pub release_distinct_track_artists: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RemoteProviderError {
    #[error("remote source unavailable")]
    Unavailable,
    #[error("remote request cancelled")]
    Cancelled,
    #[error("remote response invalid")]
    InvalidResponse,
}

pub type RemoteProviderResult = Result<Vec<RemoteIdentity>, RemoteProviderError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AlbumRequest {
    pub(crate) query: AlbumQuery,
}

/// The outcome of one release lookup. A search that ran and found nothing is
/// the same answer as one that never ran: there is no release to speak for the
/// album fields, so whatever the track resolved on its own still stands.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct AlbumResolution {
    pub(crate) album_match: Option<AlbumMatch>,
}

pub trait RemoteProvider {
    fn direct(
        &mut self,
        lookup: &RemoteDirectLookup,
        control: &mut dyn FnMut() -> ScanControl,
    ) -> RemoteProviderResult;
    fn search_musicbrainz(
        &mut self,
        metadata: &RemoteTrackMetadata,
        control: &mut dyn FnMut() -> ScanControl,
    ) -> RemoteProviderResult;
    fn search_release(
        &mut self,
        query: &AlbumQuery,
        control: &mut dyn FnMut() -> ScanControl,
    ) -> RemoteProviderResult;
    fn acoustid(
        &mut self,
        metadata: &RemoteTrackMetadata,
        fingerprint_namespace: &str,
        fingerprint: &str,
        duration_seconds: u64,
        control: &mut dyn FnMut() -> ScanControl,
    ) -> RemoteProviderResult;
}

pub(crate) trait RemoteResolver {
    fn resolve_album(
        &mut self,
        request: &AlbumRequest,
        control: &mut dyn FnMut() -> ScanControl,
    ) -> Result<AlbumResolution, RemoteProviderError> {
        let _ = (request, control);
        Ok(AlbumResolution::default())
    }

    fn resolve_track(
        &mut self,
        metadata: &RemoteTrackMetadata,
        path: &Path,
        fingerprint_backend: Option<&dyn FingerprintBackend>,
        album_match: Option<&AlbumMatch>,
        control: &mut dyn FnMut() -> ScanControl,
    ) -> Result<RemoteResolution, RemoteProviderError>;
}

pub(crate) struct ProviderRemoteResolver<P> {
    provider: P,
}

impl<P> ProviderRemoteResolver<P> {
    pub(crate) const fn new(provider: P) -> Self {
        Self { provider }
    }
}

#[cfg(test)]
impl<P> ProviderRemoteResolver<P> {
    pub(crate) fn into_provider(self) -> P {
        self.provider
    }
}

impl<P: RemoteProvider> RemoteResolver for ProviderRemoteResolver<P> {
    fn resolve_album(
        &mut self,
        request: &AlbumRequest,
        control: &mut dyn FnMut() -> ScanControl,
    ) -> Result<AlbumResolution, RemoteProviderError> {
        if control() == ScanControl::Cancel {
            return Err(RemoteProviderError::Cancelled);
        }
        let candidates = source_result(self.provider.search_release(&request.query, control))?;
        Ok(AlbumResolution {
            album_match: best_release(&request.query, &candidates),
        })
    }

    fn resolve_track(
        &mut self,
        metadata: &RemoteTrackMetadata,
        path: &Path,
        fingerprint_backend: Option<&dyn FingerprintBackend>,
        album_match: Option<&AlbumMatch>,
        control: &mut dyn FnMut() -> ScanControl,
    ) -> Result<RemoteResolution, RemoteProviderError> {
        if control() == ScanControl::Cancel {
            return Err(RemoteProviderError::Cancelled);
        }
        let mut matches = Vec::new();
        for lookup in metadata.direct_lookups() {
            if control() == ScanControl::Cancel {
                return Err(RemoteProviderError::Cancelled);
            }
            matches.extend(source_result(self.provider.direct(&lookup, control))?);
            if direct_resolution_is_complete(metadata, &matches) {
                return Ok(track_resolution(metadata, &matches, album_match));
            }
        }
        if control() == ScanControl::Cancel {
            return Err(RemoteProviderError::Cancelled);
        }
        let searched = source_result(self.provider.search_musicbrainz(metadata, control))?;
        matches.extend(searched);
        if arbitration::is_complete(metadata, &matches) {
            return Ok(track_resolution(metadata, &matches, album_match));
        }
        if metadata.valid_recording_mbid().is_some()
            || album_match.is_some_and(|album_match| album_match.exact)
        {
            return Ok(track_resolution(metadata, &matches, album_match));
        }
        let Some(backend) = fingerprint_backend else {
            return Ok(track_resolution(metadata, &matches, album_match));
        };
        let mut cancelled = false;
        let fingerprint = match backend.fingerprint(path, &mut |_: FingerprintProgress| {
            if control() == ScanControl::Cancel {
                cancelled = true;
                FingerprintControl::Cancel
            } else {
                FingerprintControl::Continue
            }
        }) {
            Ok(fingerprint) => fingerprint,
            Err(_) => return Ok(track_resolution(metadata, &matches, album_match)),
        };
        let FingerprintOutcome::Completed(fingerprint) = fingerprint else {
            return Err(RemoteProviderError::Cancelled);
        };
        if cancelled || control() == ScanControl::Cancel {
            return Err(RemoteProviderError::Cancelled);
        }
        let acoustid = source_result(self.provider.acoustid(
            metadata,
            &fingerprint.cache_namespace,
            &fingerprint.encoded,
            fingerprint.duration_seconds,
            control,
        ))?;
        matches.extend(acoustid);
        Ok(track_resolution(metadata, &matches, album_match))
    }
}

fn track_resolution(
    metadata: &RemoteTrackMetadata,
    matches: &[RemoteIdentity],
    album_match: Option<&AlbumMatch>,
) -> RemoteResolution {
    let mut resolution = if let Some(album_match) = album_match {
        arbitration::arbitrate_track_match(metadata, matches, album_match)
    } else {
        arbitration::arbitrate(metadata, matches)
    };
    if album_match.is_some() {
        resolution.proposals.retain(|proposal| {
            matches!(
                proposal.field,
                super::super::DoctorField::Title
                    | super::super::DoctorField::Artist
                    | super::super::DoctorField::RecordingMbid
            )
        });
        resolution.groups.retain(|group| {
            matches!(
                group.field,
                super::super::DoctorField::Title | super::super::DoctorField::Artist
            )
        });
    }
    resolution
}

pub(crate) fn album_resolution_for_track(
    metadata: &RemoteTrackMetadata,
    album_match: &AlbumMatch,
) -> RemoteResolution {
    let mut resolution = arbitration::arbitrate_album_match(metadata, album_match);
    resolution.proposals.retain(|proposal| {
        matches!(
            proposal.field,
            super::super::DoctorField::Album
                | super::super::DoctorField::AlbumArtist
                | super::super::DoctorField::Year
        )
    });
    for proposal in &mut resolution.proposals {
        proposal.resolved_release_mbid = album_match.identity.release_mbid.clone();
    }
    resolution.groups.retain(|group| {
        matches!(
            group.field,
            super::super::DoctorField::Album
                | super::super::DoctorField::AlbumArtist
                | super::super::DoctorField::Year
        )
    });
    resolution
}

fn source_result(result: RemoteProviderResult) -> Result<Vec<RemoteIdentity>, RemoteProviderError> {
    match result {
        Ok(matches) => Ok(matches),
        Err(RemoteProviderError::Cancelled) => Err(RemoteProviderError::Cancelled),
        Err(RemoteProviderError::Unavailable | RemoteProviderError::InvalidResponse) => {
            Ok(Vec::new())
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RemoteResolution {
    pub proposals: Vec<super::super::DoctorProposal>,
    pub groups: Vec<super::super::DoctorUnresolvedGroup>,
}

#[cfg(test)]
pub(crate) fn resolve_with_provider(
    provider: &mut dyn RemoteProvider,
    metadata: &RemoteTrackMetadata,
    fingerprint: Option<(&str, u64)>,
) -> RemoteResolution {
    let mut control = || ScanControl::Continue;
    let mut matches = Vec::new();
    for lookup in metadata.direct_lookups() {
        matches.extend(provider.direct(&lookup, &mut control).unwrap_or_default());
        if direct_resolution_is_complete(metadata, &matches) {
            return arbitration::arbitrate(metadata, &matches);
        }
    }
    let searched = provider
        .search_musicbrainz(metadata, &mut control)
        .unwrap_or_default();
    matches.extend(searched);
    if arbitration::is_complete(metadata, &matches) {
        return arbitration::arbitrate(metadata, &matches);
    }
    let Some((fingerprint, duration)) = fingerprint else {
        return arbitration::arbitrate(metadata, &matches);
    };
    let acoustid = provider
        .acoustid(metadata, "test", fingerprint, duration, &mut control)
        .unwrap_or_default();
    matches.extend(acoustid);
    arbitration::arbitrate(metadata, &matches)
}

fn direct_resolution_is_complete(
    metadata: &RemoteTrackMetadata,
    identities: &[RemoteIdentity],
) -> bool {
    arbitration::is_complete(metadata, identities)
        && identities
            .iter()
            .any(|identity| identity_confirms_embedded_ids(metadata, identity))
}

fn identity_confirms_embedded_ids(
    metadata: &RemoteTrackMetadata,
    identity: &RemoteIdentity,
) -> bool {
    metadata
        .recording_mbid
        .as_deref()
        .is_none_or(|expected| identity.recording_mbid.as_deref() == Some(expected))
        && metadata
            .release_mbid
            .as_deref()
            .is_none_or(|expected| identity.release_mbid.as_deref() == Some(expected))
        && metadata
            .release_group_mbid
            .as_deref()
            .is_none_or(|expected| identity.release_group_mbid.as_deref() == Some(expected))
        && metadata
            .artist_mbid
            .as_deref()
            .is_none_or(|expected| identity.artist_mbid.as_deref() == Some(expected))
        && metadata
            .release_artist_mbid
            .as_deref()
            .is_none_or(|expected| identity.release_artist_mbid.as_deref() == Some(expected))
}

#[cfg(test)]
mod perf_fingerprint_tests {
    use super::*;
    use crate::fingerprint::{FingerprintCapability, FingerprintError};

    #[derive(Default)]
    struct EmptyProvider;

    impl RemoteProvider for EmptyProvider {
        fn direct(
            &mut self,
            _: &RemoteDirectLookup,
            _: &mut dyn FnMut() -> ScanControl,
        ) -> RemoteProviderResult {
            Ok(Vec::new())
        }

        fn search_musicbrainz(
            &mut self,
            _: &RemoteTrackMetadata,
            _: &mut dyn FnMut() -> ScanControl,
        ) -> RemoteProviderResult {
            Ok(Vec::new())
        }

        fn search_release(
            &mut self,
            _: &AlbumQuery,
            _: &mut dyn FnMut() -> ScanControl,
        ) -> RemoteProviderResult {
            Ok(Vec::new())
        }

        fn acoustid(
            &mut self,
            _: &RemoteTrackMetadata,
            _: &str,
            _: &str,
            _: u64,
            _: &mut dyn FnMut() -> ScanControl,
        ) -> RemoteProviderResult {
            panic!("AcoustID must not run when fingerprinting is forbidden")
        }
    }

    struct ForbiddenFingerprint;

    impl FingerprintBackend for ForbiddenFingerprint {
        fn capability(&self) -> FingerprintCapability {
            FingerprintCapability::Available {
                cache_namespace: "forbidden".into(),
            }
        }

        fn fingerprint(
            &self,
            _: &Path,
            _: &mut dyn FnMut(FingerprintProgress) -> FingerprintControl,
        ) -> Result<FingerprintOutcome, FingerprintError> {
            panic!("fingerprinting must be skipped")
        }
    }

    fn metadata(recording_mbid: Option<&str>) -> RemoteTrackMetadata {
        RemoteTrackMetadata {
            title: Some("Track".into()),
            artist: Some("Artist".into()),
            album: Some("Album".into()),
            album_artist: Some("Artist".into()),
            year: Some(2024),
            recording_mbid: recording_mbid.map(str::to_owned),
            release_mbid: None,
            release_group_mbid: None,
            artist_mbid: None,
            release_artist_mbid: None,
            duration_ms: Some(180_000),
        }
    }

    fn exact_album_match() -> AlbumMatch {
        AlbumMatch {
            identity: RemoteIdentity {
                source: RemoteEvidenceSource::MusicBrainz,
                confidence: 100,
                recording_mbid: None,
                release_mbid: Some("123e4567-e89b-12d3-a456-426614174001".into()),
                release_group_mbid: None,
                artist_mbid: None,
                release_artist_mbid: None,
                title: None,
                artist: None,
                album: Some("Album".into()),
                album_artist: Some("Artist".into()),
                release_year: Some(2024),
                original_release_year: Some(2024),
                duration_ms: None,
                secondary_types: Vec::new(),
                release_track_count: Some(1),
                release_track_titles: vec!["Track".into()],
                release_distinct_track_artists: Some(1),
            },
            score: 100,
            exact: true,
        }
    }

    fn resolve(metadata: &RemoteTrackMetadata, album_match: Option<&AlbumMatch>) {
        let mut resolver = ProviderRemoteResolver::new(EmptyProvider);
        RemoteResolver::resolve_track(
            &mut resolver,
            metadata,
            Path::new("ignored"),
            Some(&ForbiddenFingerprint),
            album_match,
            &mut || ScanControl::Continue,
        )
        .unwrap();
    }

    #[test]
    fn doc_1g_a_track_with_a_recording_mbid_is_never_fingerprinted() {
        resolve(
            &metadata(Some("123e4567-e89b-12d3-a456-426614174000")),
            None,
        );
    }

    #[test]
    fn doc_1g_a_confidently_matched_album_is_never_fingerprinted() {
        resolve(&metadata(None), Some(&exact_album_match()));
    }
}
