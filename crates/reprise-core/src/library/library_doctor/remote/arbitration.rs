use std::collections::{BTreeMap, BTreeSet};

use super::super::{
    DoctorCandidate, DoctorField, DoctorGroupMember, DoctorProposal, DoctorUnresolvedGroup,
    DoctorValue, ProblemClass, ProposalSource,
};
use super::{
    RemoteEvidence, RemoteEvidenceSource, RemoteIdentity, RemoteResolution, RemoteTrackMetadata,
    REMOTE_WRITABLE_FIELDS,
};

type Ranked<'a> = Vec<(String, Vec<&'a RemoteIdentity>)>;

pub(crate) fn arbitrate(
    metadata: &RemoteTrackMetadata,
    identities: &[RemoteIdentity],
) -> RemoteResolution {
    let mut resolution = RemoteResolution::default();
    for field in REMOTE_WRITABLE_FIELDS {
        let ranked = ranked_candidates(identities, field);
        if ranked.is_empty() {
            continue;
        }
        let contradiction = candidates_contradict(metadata, field, &ranked);
        let has_lead = has_clear_lead(&ranked);
        let current = current_value(metadata, field);
        if !contradiction && has_lead {
            let (value, identities) = &ranked[0];
            let proposed = decode_value(field, value);
            if proposed == current {
                continue;
            }
            let evidence = identities
                .iter()
                .map(|identity| to_evidence(metadata, identity, field, value))
                .collect::<Vec<_>>();
            let confidence = evidence
                .iter()
                .map(|item| item.confidence)
                .min()
                .unwrap_or_default();
            resolution.proposals.push(DoctorProposal {
                track_id: 0,
                field,
                current,
                proposed,
                source: source_for(&evidence),
                confidence,
                preselected: false,
                problem_class: problem_class(field),
                evidence,
                local_fallback: None,
            });
        } else {
            resolution.groups.push(DoctorUnresolvedGroup {
                field,
                group_key: format!("remote:{field:?}"),
                candidates: ranked
                    .into_iter()
                    .map(|(value, identities)| DoctorCandidate {
                        value: decode_value(field, &value),
                        count: identities.len(),
                        evidence: identities
                            .iter()
                            .map(|identity| to_evidence(metadata, identity, field, &value))
                            .collect(),
                    })
                    .collect(),
                members: vec![DoctorGroupMember {
                    track_id: 0,
                    current,
                }],
                local_fallback: None,
            });
        }
    }
    resolution
}

pub(super) fn is_complete(metadata: &RemoteTrackMetadata, identities: &[RemoteIdentity]) -> bool {
    REMOTE_WRITABLE_FIELDS.into_iter().all(|field| {
        let ranked = ranked_candidates(identities, field);
        !ranked.is_empty()
            && has_clear_lead(&ranked)
            && !candidates_contradict(metadata, field, &ranked)
    })
}

fn ranked_candidates(identities: &[RemoteIdentity], field: DoctorField) -> Ranked<'_> {
    let year = (field == DoctorField::Year)
        .then(|| canonical_year(identities))
        .flatten();
    let mut by_value = BTreeMap::<String, Vec<&RemoteIdentity>>::new();
    for identity in identities {
        let value = match field {
            DoctorField::Title => identity.title.clone(),
            DoctorField::Artist => identity.artist.clone(),
            DoctorField::Album => identity.album.clone(),
            DoctorField::AlbumArtist => identity.album_artist.clone(),
            DoctorField::Year => year.map(|value| value.to_string()),
            DoctorField::RecordingMbid => identity.recording_mbid.clone(),
            DoctorField::Genre => None,
        };
        if let Some(value) = value {
            by_value.entry(value).or_default().push(identity);
        }
    }
    let mut ranked = by_value.into_iter().collect::<Vec<_>>();
    ranked.sort_by(|left, right| {
        max_score(&right.1)
            .cmp(&max_score(&left.1))
            .then_with(|| left.0.cmp(&right.0))
    });
    ranked
}

fn canonical_year(identities: &[RemoteIdentity]) -> Option<u32> {
    let release_ids = unique_values(
        identities
            .iter()
            .filter_map(|item| item.release_mbid.as_ref()),
    );
    if release_ids.len() == 1 {
        let years = unique_values(
            identities
                .iter()
                .filter_map(|item| item.release_year.as_ref()),
        );
        if years.len() == 1 {
            return Some(*years[0]);
        }
        if years.len() > 1 {
            return None;
        }
    }
    let group_ids = unique_values(
        identities
            .iter()
            .filter_map(|item| item.release_group_mbid.as_ref()),
    );
    if group_ids.len() != 1 {
        return None;
    }
    let years = unique_values(
        identities
            .iter()
            .filter_map(|item| item.original_release_year.as_ref()),
    );
    (years.len() == 1).then(|| *years[0])
}

fn unique_values<T: Ord>(values: impl Iterator<Item = T>) -> Vec<T> {
    values.collect::<BTreeSet<_>>().into_iter().collect()
}

fn has_clear_lead(ranked: &Ranked<'_>) -> bool {
    ranked.len() == 1 || max_score(&ranked[0].1).saturating_sub(max_score(&ranked[1].1)) >= 10
}

fn candidates_contradict(
    metadata: &RemoteTrackMetadata,
    field: DoctorField,
    ranked: &Ranked<'_>,
) -> bool {
    let identities = ranked
        .iter()
        .flat_map(|(_, identities)| identities.iter().copied())
        .collect::<Vec<_>>();
    sources_disagree(ranked)
        || identities.iter().any(|identity| {
            conflicts_with_local(metadata, field, identity)
                || duration_conflict(metadata.duration_ms, identity.duration_ms)
        })
        || identities.iter().enumerate().any(|(index, left)| {
            identities[index + 1..]
                .iter()
                .any(|right| identities_conflict(field, left, right))
        })
}

fn sources_disagree(ranked: &Ranked<'_>) -> bool {
    ranked.iter().enumerate().any(|(index, (_, left))| {
        ranked[index + 1..].iter().any(|(_, right)| {
            left.iter()
                .any(|left| right.iter().any(|right| left.source != right.source))
        })
    })
}

fn conflicts_with_local(
    metadata: &RemoteTrackMetadata,
    field: DoctorField,
    identity: &RemoteIdentity,
) -> bool {
    let recording = option_conflict(&metadata.recording_mbid, &identity.recording_mbid);
    match field {
        DoctorField::Title | DoctorField::RecordingMbid => recording,
        DoctorField::Artist => {
            recording || option_conflict(&metadata.artist_mbid, &identity.artist_mbid)
        }
        DoctorField::Album | DoctorField::AlbumArtist | DoctorField::Year => {
            recording
                || option_conflict(&metadata.release_mbid, &identity.release_mbid)
                || option_conflict(&metadata.release_group_mbid, &identity.release_group_mbid)
                || (field == DoctorField::AlbumArtist
                    && option_conflict(
                        &metadata.release_artist_mbid,
                        &identity.release_artist_mbid,
                    ))
        }
        DoctorField::Genre => false,
    }
}

fn identities_conflict(field: DoctorField, left: &RemoteIdentity, right: &RemoteIdentity) -> bool {
    let recording = option_conflict(&left.recording_mbid, &right.recording_mbid);
    let identity = match field {
        DoctorField::Title | DoctorField::RecordingMbid => recording,
        DoctorField::Artist => recording || option_conflict(&left.artist_mbid, &right.artist_mbid),
        DoctorField::Album | DoctorField::AlbumArtist | DoctorField::Year => {
            recording
                || option_conflict(&left.release_group_mbid, &right.release_group_mbid)
                || (field == DoctorField::AlbumArtist
                    && option_conflict(&left.release_artist_mbid, &right.release_artist_mbid))
        }
        DoctorField::Genre => false,
    };
    identity || duration_conflict(left.duration_ms, right.duration_ms)
}

fn duration_conflict(left: Option<u64>, right: Option<u64>) -> bool {
    matches!((left, right), (Some(left), Some(right)) if left.abs_diff(right) > 2_000)
}

fn option_conflict(left: &Option<String>, right: &Option<String>) -> bool {
    matches!((left, right), (Some(left), Some(right)) if left != right)
}

fn max_score(identities: &[&RemoteIdentity]) -> u8 {
    identities
        .iter()
        .map(|identity| identity.confidence)
        .max()
        .unwrap_or_default()
}

fn to_evidence(
    metadata: &RemoteTrackMetadata,
    identity: &RemoteIdentity,
    field: DoctorField,
    value: &str,
) -> RemoteEvidence {
    RemoteEvidence {
        source: identity.source,
        confidence: identity.confidence,
        recording_mbid: identity.recording_mbid.clone(),
        release_mbid: identity.release_mbid.clone(),
        release_group_mbid: identity.release_group_mbid.clone(),
        artist_mbid: identity.artist_mbid.clone(),
        release_artist_mbid: identity.release_artist_mbid.clone(),
        title: identity.title.clone(),
        artist: identity.artist.clone(),
        album: identity.album.clone(),
        year: if field == DoctorField::Year {
            value.parse().ok()
        } else {
            identity.release_year.or(identity.original_release_year)
        },
        duration_ms: identity.duration_ms,
        duration_delta_ms: identity
            .duration_ms
            .zip(metadata.duration_ms)
            .map(|(left, right)| left.abs_diff(right)),
    }
}

fn source_for(evidence: &[RemoteEvidence]) -> ProposalSource {
    match evidence.first().map(|value| value.source) {
        Some(RemoteEvidenceSource::AcoustId) => ProposalSource::AcoustId,
        _ => ProposalSource::MusicBrainz,
    }
}

fn decode_value(field: DoctorField, value: &str) -> DoctorValue {
    DoctorValue::decode(field, Some(value.to_owned()))
}

fn current_value(metadata: &RemoteTrackMetadata, field: DoctorField) -> DoctorValue {
    match field {
        DoctorField::Title => DoctorValue::decode(field, metadata.title.clone()),
        DoctorField::Artist => DoctorValue::decode(field, metadata.artist.clone()),
        DoctorField::Album => DoctorValue::decode(field, metadata.album.clone()),
        DoctorField::AlbumArtist => DoctorValue::decode(field, metadata.album_artist.clone()),
        DoctorField::Year => metadata.year.map_or(DoctorValue::Empty, DoctorValue::Year),
        DoctorField::RecordingMbid => DoctorValue::decode(field, metadata.recording_mbid.clone()),
        DoctorField::Genre => DoctorValue::Empty,
    }
}

fn problem_class(field: DoctorField) -> ProblemClass {
    match field {
        DoctorField::Year => ProblemClass::MissingWrongYear,
        DoctorField::RecordingMbid => ProblemClass::MissingRecordingMbid,
        _ => ProblemClass::CasingWhitespace,
    }
}
