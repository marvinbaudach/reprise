use super::{arbitration, RemoteDirectLookup, RemoteEvidenceSource, RemoteTrackMetadata};
use crate::fingerprint::{
    FingerprintBackend, FingerprintControl, FingerprintOutcome, FingerprintProgress,
};
use crate::library::library_doctor::ScanControl;
use serde::{Deserialize, Serialize};
use std::path::Path;

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
    fn resolve_track(
        &mut self,
        metadata: &RemoteTrackMetadata,
        path: &Path,
        fingerprint_backend: Option<&dyn FingerprintBackend>,
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
    fn resolve_track(
        &mut self,
        metadata: &RemoteTrackMetadata,
        path: &Path,
        fingerprint_backend: Option<&dyn FingerprintBackend>,
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
                return Ok(arbitration::arbitrate(metadata, &matches));
            }
        }
        if control() == ScanControl::Cancel {
            return Err(RemoteProviderError::Cancelled);
        }
        let searched = source_result(self.provider.search_musicbrainz(metadata, control))?;
        matches.extend(searched);
        if arbitration::is_complete(metadata, &matches) {
            return Ok(arbitration::arbitrate(metadata, &matches));
        }
        let Some(backend) = fingerprint_backend else {
            return Ok(arbitration::arbitrate(metadata, &matches));
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
            Err(_) => return Ok(arbitration::arbitrate(metadata, &matches)),
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
        Ok(arbitration::arbitrate(metadata, &matches))
    }
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
