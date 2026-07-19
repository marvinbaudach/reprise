//! Typed parser for AcoustID's official lookup response shape.

use serde::Deserialize;

use super::metadata::canonical_uuid;
use super::{RemoteEvidenceSource, RemoteIdentity, RemoteProviderError, RemoteProviderResult};

#[derive(Deserialize)]
struct Response {
    status: String,
    #[serde(default)]
    error: Option<ApiError>,
    #[serde(default)]
    results: Vec<ResultMatch>,
}

#[derive(Deserialize)]
struct ApiError {
    code: Option<u64>,
}

#[derive(Deserialize)]
struct ResultMatch {
    score: Option<f64>,
    #[serde(default)]
    recordings: Vec<Recording>,
}

#[derive(Deserialize)]
struct Recording {
    id: String,
    title: Option<String>,
    duration: Option<f64>,
    #[serde(default)]
    artists: Vec<Artist>,
    #[serde(default, alias = "release-groups")]
    releasegroups: Vec<ReleaseGroup>,
}

#[derive(Deserialize)]
struct Artist {
    id: Option<String>,
    name: Option<String>,
}

#[derive(Deserialize)]
struct ReleaseGroup {
    id: Option<String>,
    title: Option<String>,
    #[serde(default)]
    artists: Vec<Artist>,
    #[serde(default)]
    releases: Vec<Release>,
}

#[derive(Deserialize)]
struct Release {
    id: Option<String>,
    title: Option<String>,
    date: Option<String>,
    #[serde(default)]
    artists: Vec<Artist>,
}

pub(super) fn parse_acoustid(body: &str) -> RemoteProviderResult {
    let response: Response =
        serde_json::from_str(body).map_err(|_| RemoteProviderError::InvalidResponse)?;
    if response.status != "ok" {
        // AcoustID code 3 is "invalid API key". Parameter and fingerprint
        // errors are request-local and must not disable the source for the job.
        return Err(if response.error.and_then(|error| error.code) == Some(3) {
            RemoteProviderError::Unavailable
        } else {
            RemoteProviderError::InvalidResponse
        });
    }
    let mut identities = Vec::new();
    for result in response.results {
        let confidence = percentage(result.score);
        for recording in &result.recordings {
            identities.extend(recording_identities(recording, confidence));
        }
    }
    Ok(identities)
}

fn recording_identities(recording: &Recording, confidence: u8) -> Vec<RemoteIdentity> {
    let Some(recording_mbid) = canonical_uuid(Some(&recording.id)) else {
        return Vec::new();
    };
    let artist = recording.artists.first();
    let artist_mbid = artist.and_then(|item| canonical_uuid(item.id.as_deref()));
    let artist_name = artist.and_then(|item| item.name.clone());
    let duration_ms = recording
        .duration
        .filter(|duration| duration.is_finite() && *duration > 0.0)
        .map(|duration| (duration * 1_000.0).round() as u64);
    if recording.releasegroups.is_empty() {
        return vec![identity(
            recording,
            recording_mbid,
            artist_mbid,
            artist_name,
            duration_ms,
            confidence,
            None,
            None,
        )];
    }
    recording
        .releasegroups
        .iter()
        .flat_map(|group| {
            if group.releases.is_empty() {
                vec![identity(
                    recording,
                    recording_mbid.clone(),
                    artist_mbid.clone(),
                    artist_name.clone(),
                    duration_ms,
                    confidence,
                    Some(group),
                    None,
                )]
            } else {
                group
                    .releases
                    .iter()
                    .map(|release| {
                        identity(
                            recording,
                            recording_mbid.clone(),
                            artist_mbid.clone(),
                            artist_name.clone(),
                            duration_ms,
                            confidence,
                            Some(group),
                            Some(release),
                        )
                    })
                    .collect()
            }
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn identity(
    recording: &Recording,
    recording_mbid: String,
    artist_mbid: Option<String>,
    artist: Option<String>,
    duration_ms: Option<u64>,
    confidence: u8,
    group: Option<&ReleaseGroup>,
    release: Option<&Release>,
) -> RemoteIdentity {
    let release_artist = release
        .and_then(|item| item.artists.first())
        .or_else(|| group.and_then(|item| item.artists.first()));
    let original_release_year = group.and_then(|item| {
        item.releases
            .iter()
            .filter_map(|release| release.date.as_deref().and_then(year))
            .min()
    });
    RemoteIdentity {
        source: RemoteEvidenceSource::AcoustId,
        confidence,
        recording_mbid: Some(recording_mbid),
        release_mbid: release.and_then(|item| canonical_uuid(item.id.as_deref())),
        release_group_mbid: group.and_then(|item| canonical_uuid(item.id.as_deref())),
        artist_mbid,
        release_artist_mbid: release_artist.and_then(|item| canonical_uuid(item.id.as_deref())),
        title: recording.title.clone(),
        artist,
        album: release
            .and_then(|item| item.title.clone())
            .or_else(|| group.and_then(|item| item.title.clone())),
        album_artist: release_artist.and_then(|item| item.name.clone()),
        release_year: release.and_then(|item| item.date.as_deref().and_then(year)),
        original_release_year,
        duration_ms,
    }
}

fn year(value: &str) -> Option<u32> {
    value.get(..4)?.parse().ok()
}

fn percentage(value: Option<f64>) -> u8 {
    value
        .map(|score| (score.clamp(0.0, 1.0) * 100.0).round() as u8)
        .unwrap_or_default()
}
