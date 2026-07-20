use std::cmp::Ordering;
use std::fmt::Write;

use md5::{Digest, Md5};
use serde::{Deserialize, Serialize};

use super::{CriteriaMode, EnergyCurve, MixCandidate, MixIntent, MixPlannerError};

const ARTIST_GAP: usize = 4;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SelectionReason {
    IntensityMatch,
    BrightnessMatch,
    DynamicityMatch,
    RhythmicityMatch,
    GenreMatch,
    RelatedArtist,
    FamiliarityMatch,
    ArtistDiversity,
    DurationFit,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MixDiagnostic {
    ArtistGapRelaxed,
    DurationUnderfilled,
    MissingAudioEvidence,
    MissingGenreEvidence,
    MissingRelatedArtistEvidence,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MixDraftTrack {
    pub track_id: i64,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_ms: i64,
    pub score: f64,
    pub profile_intensity: f64,
    pub reasons: Vec<SelectionReason>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MixDraft {
    pub draft_id: String,
    pub intent: MixIntent,
    pub tracks: Vec<MixDraftTrack>,
    pub total_duration_ms: i64,
    pub analyzed_candidates: usize,
    pub total_candidates: usize,
    pub diagnostics: Vec<MixDiagnostic>,
}

struct ScoredCandidate {
    candidate: MixCandidate,
    score: f64,
    reasons: Vec<SelectionReason>,
}

pub fn plan_candidates(
    intent: &MixIntent,
    candidates: Vec<MixCandidate>,
) -> Result<MixDraft, MixPlannerError> {
    if candidates.is_empty() {
        return Err(MixPlannerError::InvalidIntent("no eligible mix candidates"));
    }
    let total_candidates = candidates.len();
    let analyzed_candidates = candidates
        .iter()
        .filter(|candidate| candidate.profile.is_some())
        .count();
    let max_plays = candidates
        .iter()
        .map(|candidate| candidate.play_count.max(0))
        .max()
        .unwrap_or(0) as f64;
    let mut scored = candidates
        .into_iter()
        .map(|candidate| score_candidate(intent, candidate, max_plays))
        .collect::<Result<Vec<_>, _>>()?;
    scored.sort_by(|left, right| {
        left.score
            .partial_cmp(&right.score)
            .unwrap_or(Ordering::Equal)
            .then_with(|| left.candidate.track_id.cmp(&right.candidate.track_id))
    });

    let mut selected = Vec::<ScoredCandidate>::new();
    let mut total_duration_ms = 0_i64;
    let mut diagnostics = Vec::new();
    while !scored.is_empty() && total_duration_ms < intent.target_duration_ms() {
        let allowed = scored.iter().position(|item| {
            selected
                .iter()
                .rev()
                .take(ARTIST_GAP - 1)
                .all(|recent| recent.candidate.artist != item.candidate.artist)
        });
        let index = match allowed {
            Some(index) => index,
            None => {
                if !diagnostics.contains(&MixDiagnostic::ArtistGapRelaxed) {
                    diagnostics.push(MixDiagnostic::ArtistGapRelaxed);
                }
                0
            }
        };
        let next_duration = scored[index].candidate.duration_ms.max(0);
        if !selected.is_empty() && total_duration_ms + next_duration > intent.target_duration_ms() {
            let under = intent.target_duration_ms() - total_duration_ms;
            let over = total_duration_ms + next_duration - intent.target_duration_ms();
            if under < over {
                break;
            }
        }
        let mut chosen = scored.remove(index);
        chosen.reasons.push(SelectionReason::ArtistDiversity);
        chosen.reasons.push(SelectionReason::DurationFit);
        total_duration_ms += next_duration;
        selected.push(chosen);
    }
    if total_duration_ms < intent.target_duration_ms() {
        diagnostics.push(MixDiagnostic::DurationUnderfilled);
    }
    reorder_for_curve(&mut selected, intent.energy_curve());
    let tracks = selected
        .into_iter()
        .map(|item| {
            let intensity = item
                .candidate
                .profile
                .map_or(0.0, |profile| profile.intensity);
            MixDraftTrack {
                track_id: item.candidate.track_id,
                title: item.candidate.title,
                artist: item.candidate.artist,
                album: item.candidate.album,
                duration_ms: item.candidate.duration_ms,
                score: item.score,
                profile_intensity: intensity,
                reasons: item.reasons,
            }
        })
        .collect::<Vec<_>>();
    let draft_id = deterministic_id(intent, &tracks)?;
    Ok(MixDraft {
        draft_id,
        intent: intent.clone(),
        tracks,
        total_duration_ms,
        analyzed_candidates,
        total_candidates,
        diagnostics,
    })
}

fn score_candidate(
    intent: &MixIntent,
    candidate: MixCandidate,
    max_plays: f64,
) -> Result<ScoredCandidate, MixPlannerError> {
    let mut score = 0.0;
    let mut reasons = Vec::new();
    if matches!(
        intent.criteria(),
        CriteriaMode::AudioCharacter | CriteriaMode::Balanced
    ) {
        let profile = candidate.profile.ok_or(MixPlannerError::InvalidIntent(
            "audio-character candidate has no current profile",
        ))?;
        let values = [
            profile.intensity,
            profile.brightness,
            profile.dynamicity,
            profile.rhythmicity,
        ];
        let target = intent.target().values();
        let differences =
            std::array::from_fn::<_, 4, _>(|index| (values[index] - target[index]).abs());
        let profile_distance = differences
            .iter()
            .map(|difference| difference * difference)
            .sum::<f64>();
        score += profile_distance
            * match intent.variety() {
                super::Variety::Cohesive => 1.5,
                super::Variety::Balanced => 1.0,
                super::Variety::Wide => 0.6,
            };
        let best = differences
            .iter()
            .enumerate()
            .min_by(|left, right| left.1.partial_cmp(right.1).unwrap_or(Ordering::Equal))
            .map_or(0, |(index, _)| index);
        reasons.push(
            [
                SelectionReason::IntensityMatch,
                SelectionReason::BrightnessMatch,
                SelectionReason::DynamicityMatch,
                SelectionReason::RhythmicityMatch,
            ][best],
        );
    }
    if intent.criteria() == CriteriaMode::RelatedArtists {
        reasons.push(SelectionReason::RelatedArtist);
    }
    if matches!(
        intent.criteria(),
        CriteriaMode::Genre | CriteriaMode::Balanced
    ) {
        let genre = crate::library::group_key::normalize_group_key(&candidate.genre);
        if intent.target_genres().iter().any(|target| target == &genre) {
            reasons.push(SelectionReason::GenreMatch);
        } else if genre.is_empty() {
            score += 1.5;
        } else {
            score += 1.0;
        }
    }
    if max_plays > 0.0 {
        let familiarity = candidate.play_count.max(0) as f64 / max_plays;
        score += match intent.familiarity() {
            super::Familiarity::Familiar => (1.0 - familiarity) * 0.25,
            super::Familiarity::Balanced => 0.0,
            super::Familiarity::Discover => familiarity * 0.25,
        };
        reasons.push(SelectionReason::FamiliarityMatch);
    }
    Ok(ScoredCandidate {
        candidate,
        score,
        reasons,
    })
}

fn reorder_for_curve(selected: &mut [ScoredCandidate], curve: EnergyCurve) {
    let intensity = |item: &ScoredCandidate| item.candidate.profile.map_or(0.0, |p| p.intensity);
    match curve {
        EnergyCurve::Flat => {}
        EnergyCurve::Rise => selected.sort_by(|left, right| {
            intensity(left)
                .partial_cmp(&intensity(right))
                .unwrap_or(Ordering::Equal)
        }),
        EnergyCurve::Fall => selected.sort_by(|left, right| {
            intensity(right)
                .partial_cmp(&intensity(left))
                .unwrap_or(Ordering::Equal)
        }),
        EnergyCurve::Arc => {
            selected.sort_by(|left, right| {
                intensity(left)
                    .partial_cmp(&intensity(right))
                    .unwrap_or(Ordering::Equal)
            });
            let midpoint = selected.len().div_ceil(2);
            selected[midpoint..].reverse();
        }
    }
}

fn deterministic_id(
    intent: &MixIntent,
    tracks: &[MixDraftTrack],
) -> Result<String, MixPlannerError> {
    let mut digest = Md5::new();
    digest.update(intent.to_json()?.as_bytes());
    for track in tracks {
        digest.update(track.track_id.to_le_bytes());
    }
    let mut id = String::with_capacity(32);
    for byte in digest.finalize() {
        let _ = write!(id, "{byte:02x}");
    }
    Ok(id)
}
