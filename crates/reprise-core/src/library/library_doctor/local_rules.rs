use std::collections::HashMap;

use super::{
    DoctorCandidate, DoctorField, DoctorGroupMember, DoctorProposal, DoctorTrackRef,
    DoctorUnresolvedGroup, DoctorValue, ProblemClass, ProposalSource,
};
use crate::library::tag_edit::EditableTags;

pub(super) struct ReadTrack {
    pub reference: DoctorTrackRef,
    pub tags: EditableTags,
}

pub(super) fn proposals_for(
    tracks: &[ReadTrack],
) -> (Vec<DoctorProposal>, Vec<DoctorUnresolvedGroup>) {
    let mut proposals = Vec::new();
    let mut unresolved = Vec::new();
    add_title_trims(tracks, &mut proposals);
    add_missing_album_artists(tracks, &mut proposals);
    for field in [
        DoctorField::Artist,
        DoctorField::Album,
        DoctorField::AlbumArtist,
        DoctorField::Genre,
    ] {
        add_grouped_field(tracks, field, &mut proposals, &mut unresolved);
    }
    (proposals, unresolved)
}

fn add_title_trims(tracks: &[ReadTrack], proposals: &mut Vec<DoctorProposal>) {
    for track in tracks {
        let trimmed = track.tags.title.trim();
        if track.tags.title != trimmed {
            proposals.push(local_proposal(
                track.reference.track_id,
                DoctorField::Title,
                &track.tags.title,
                trimmed,
                ProblemClass::CasingWhitespace,
            ));
        }
    }
}

fn add_missing_album_artists(tracks: &[ReadTrack], proposals: &mut Vec<DoctorProposal>) {
    for track in tracks {
        if !track.tags.album_artist.trim().is_empty() || track.tags.artist.trim().is_empty() {
            continue;
        }
        proposals.push(local_proposal(
            track.reference.track_id,
            DoctorField::AlbumArtist,
            &track.tags.album_artist,
            track.tags.artist.trim(),
            ProblemClass::MissingAlbumArtist,
        ));
    }
}

fn add_grouped_field(
    tracks: &[ReadTrack],
    field: DoctorField,
    proposals: &mut Vec<DoctorProposal>,
    unresolved: &mut Vec<DoctorUnresolvedGroup>,
) {
    let mut groups: HashMap<String, Vec<(i64, &str)>> = HashMap::new();
    for track in tracks {
        let raw = field_value(&track.tags, field);
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        let key = crate::library::group_key::normalize_group_key(trimmed);
        if key.is_empty() {
            if raw != trimmed {
                proposals.push(local_proposal(
                    track.reference.track_id,
                    field,
                    raw,
                    trimmed,
                    if field == DoctorField::Genre {
                        ProblemClass::GenreVariant
                    } else {
                        ProblemClass::CasingWhitespace
                    },
                ));
            }
            continue;
        }
        groups
            .entry(key)
            .or_default()
            .push((track.reference.track_id, raw));
    }
    let mut groups = groups.into_iter().collect::<Vec<_>>();
    groups.sort_by(|left, right| left.0.cmp(&right.0));
    for (group_key, members) in groups {
        let mut counts = HashMap::<String, usize>::new();
        for (_, raw) in &members {
            *counts.entry(raw.trim().to_owned()).or_default() += 1;
        }
        let maximum = counts.values().copied().max().unwrap_or_default();
        let mut winners = counts
            .iter()
            .filter(|(_, count)| **count == maximum)
            .map(|(value, _)| value.clone())
            .collect::<Vec<_>>();
        winners.sort();
        if winners.len() != 1 {
            let mut candidates = counts
                .into_iter()
                .map(|(value, count)| DoctorCandidate {
                    value: DoctorValue::Text(value),
                    count,
                    evidence: Vec::new(),
                })
                .collect::<Vec<_>>();
            candidates.sort_by(|left, right| value_text(&left.value).cmp(value_text(&right.value)));
            unresolved.push(DoctorUnresolvedGroup {
                field,
                group_key,
                candidates,
                members: members
                    .iter()
                    .map(|(track_id, current)| DoctorGroupMember {
                        track_id: *track_id,
                        current: DoctorValue::from_text(current),
                    })
                    .collect(),
                local_fallback: None,
            });
            continue;
        }
        let winner = &winners[0];
        for (track_id, raw) in members {
            if raw == winner {
                continue;
            }
            proposals.push(local_proposal(
                track_id,
                field,
                raw,
                winner,
                if field == DoctorField::Genre {
                    ProblemClass::GenreVariant
                } else {
                    ProblemClass::CasingWhitespace
                },
            ));
        }
    }
}

fn field_value(tags: &EditableTags, field: DoctorField) -> &str {
    match field {
        DoctorField::Artist => &tags.artist,
        DoctorField::Album => &tags.album,
        DoctorField::AlbumArtist => &tags.album_artist,
        DoctorField::Genre => &tags.genre,
        _ => "",
    }
}

fn local_proposal(
    track_id: i64,
    field: DoctorField,
    current: &str,
    proposed: &str,
    problem_class: ProblemClass,
) -> DoctorProposal {
    DoctorProposal {
        track_id,
        field,
        current: DoctorValue::from_text(current),
        proposed: DoctorValue::from_text(proposed),
        source: ProposalSource::Local,
        confidence: 100,
        preselected: true,
        problem_class,
        evidence: Vec::new(),
        local_fallback: None,
    }
}

fn value_text(value: &DoctorValue) -> &str {
    match value {
        DoctorValue::Text(value) => value,
        DoctorValue::Empty | DoctorValue::Year(_) => "",
    }
}
