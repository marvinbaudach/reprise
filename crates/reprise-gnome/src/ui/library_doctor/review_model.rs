use std::collections::{HashMap, HashSet};

use reprise_core::library_doctor::{
    group_review_rows, DoctorCandidate, DoctorField, DoctorReviewDisplayRow, DoctorReviewRow,
    DoctorReviewRowId, DoctorReviewRowState, DoctorReviewSession, DoctorScan, DoctorValue,
    DoctorWriteRowState, ProblemClass, ProposalSource, RemoteEvidenceSource,
};

use crate::ui::strings;

pub(super) const WIDE_BREAKPOINT: i32 = 640;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ReviewLayout {
    Narrow,
    Wide,
}

pub(super) const fn layout_for_width(width: i32) -> ReviewLayout {
    if width < WIDE_BREAKPOINT {
        ReviewLayout::Narrow
    } else {
        ReviewLayout::Wide
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ConfidenceTone {
    Accent,
    Normal,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ConfidencePresentation {
    pub(super) label: String,
    pub(super) tone: ConfidenceTone,
    pub(super) warning: bool,
}

pub(super) fn confidence_presentation(
    source: ProposalSource,
    confidence: u8,
) -> ConfidencePresentation {
    match source {
        ProposalSource::Local => ConfidencePresentation {
            label: strings::text(strings::DOCTOR_SOURCE_LOCAL),
            tone: ConfidenceTone::Accent,
            warning: false,
        },
        ProposalSource::MusicBrainz | ProposalSource::AcoustId => {
            let source = strings::text(match source {
                ProposalSource::MusicBrainz => strings::DOCTOR_SOURCE_MUSICBRAINZ,
                ProposalSource::AcoustId => strings::DOCTOR_SOURCE_ACOUSTID,
                ProposalSource::Local => unreachable!(),
            });
            let low = confidence < 50;
            ConfidencePresentation {
                label: if low {
                    strings::doctor_low_confidence(&source, confidence)
                } else {
                    strings::doctor_remote_confidence(&source, confidence)
                },
                tone: if confidence >= 85 {
                    ConfidenceTone::Normal
                } else if confidence >= 50 {
                    ConfidenceTone::Warning
                } else {
                    ConfidenceTone::Error
                },
                warning: low,
            }
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct ReviewOutcome {
    pub(super) state: DoctorWriteRowState,
    pub(super) error: Option<String>,
}

#[derive(Debug, Clone)]
pub(super) struct ReviewRowModel {
    pub(super) row: DoctorReviewRow,
    pub(super) row_ids: Vec<DoctorReviewRowId>,
    pub(super) selectable_row_ids: Vec<DoctorReviewRowId>,
    pub(super) track_ids: Vec<i64>,
    pub(super) album_position: usize,
    pub(super) row_position: usize,
    pub(super) album_key: String,
    pub(super) album_title: String,
    pub(super) album_artist: String,
    pub(super) album_track_count: usize,
    pub(super) selected_change_count: usize,
    #[allow(dead_code)] // Consumed by REV-5 in the immediately following package.
    pub(super) is_album_wide: bool,
    pub(super) track: String,
    pub(super) field: String,
    pub(super) current: String,
    pub(super) proposed: String,
    pub(super) confidence: ConfidencePresentation,
    pub(super) outcome: Option<ReviewOutcome>,
}

impl ReviewRowModel {
    pub(super) fn accessible_description(&self) -> String {
        let description = strings::doctor_review_row_description(
            &self.track,
            &self.field,
            &self.current,
            &self.proposed,
            &self.confidence.label,
        );
        self.outcome
            .as_ref()
            .and_then(|outcome| outcome.error.as_deref())
            .map_or(description.clone(), |error| {
                format!("{description} {error}")
            })
    }
}

pub(super) fn grouped_rows_for(
    scan: &DoctorScan,
    review: &DoctorReviewSession,
    outcomes: &HashMap<reprise_core::library_doctor::DoctorReviewRowId, ReviewOutcome>,
) -> Vec<ReviewRowModel> {
    let titles = scan
        .tracks
        .iter()
        .map(|track| {
            let title = track
                .tags
                .as_ref()
                .map(|tags| tags.title.trim())
                .filter(|title| !title.is_empty())
                .map_or_else(
                    || strings::text(strings::DOCTOR_UNKNOWN_TRACK),
                    str::to_owned,
                );
            (track.reference.track_id, title)
        })
        .collect::<HashMap<_, _>>();
    let rows_by_id = review
        .rows()
        .iter()
        .map(|row| (row.id, row))
        .collect::<HashMap<_, _>>();
    group_review_rows(scan, review)
        .into_iter()
        .enumerate()
        .flat_map(|(album_position, album)| {
            let rows_by_id = &rows_by_id;
            let titles = &titles;
            let album_key = album.key;
            let album_title = album.title;
            let album_artist = album.artist;
            let album_track_count = album.track_count;
            album
                .rows
                .into_iter()
                .enumerate()
                .filter_map(move |(row_position, display)| {
                    let (row_ids, track_ids, track, is_album_wide) = match display {
                        DoctorReviewDisplayRow::Track { row_id, track_id } => (
                            vec![row_id],
                            vec![track_id],
                            titles
                                .get(&track_id)
                                .cloned()
                                .unwrap_or_else(|| strings::text(strings::DOCTOR_UNKNOWN_TRACK)),
                            false,
                        ),
                        DoctorReviewDisplayRow::AllTracks {
                            row_ids,
                            track_count,
                        } => {
                            let track_ids = row_ids
                                .iter()
                                .filter_map(|id| rows_by_id.get(id).map(|row| row.track_id))
                                .collect();
                            (
                                row_ids,
                                track_ids,
                                strings::doctor_all_tracks(track_count),
                                true,
                            )
                        }
                    };
                    let first = rows_by_id.get(row_ids.first()?)?;
                    let selectable_row_ids = row_ids
                        .iter()
                        .filter(|id| {
                            rows_by_id
                                .get(id)
                                .is_some_and(|row| row.state == DoctorReviewRowState::Ready)
                        })
                        .copied()
                        .collect::<Vec<_>>();
                    let selected_change_count = row_ids
                        .iter()
                        .filter_map(|id| rows_by_id.get(id))
                        .filter(|row| row.selected && row.state == DoctorReviewRowState::Ready)
                        .count();
                    let mut row = (*first).clone();
                    row.selected = !selectable_row_ids.is_empty()
                        && selected_change_count == selectable_row_ids.len();
                    Some(ReviewRowModel {
                        field: strings::text(field_label(row.field)),
                        current: value_text(&row.current),
                        proposed: value_text(&row.proposed),
                        confidence: confidence_presentation(row.source, row.confidence),
                        outcome: outcomes.get(&row.id).cloned(),
                        row,
                        row_ids,
                        selectable_row_ids,
                        track_ids,
                        album_position,
                        row_position,
                        album_key: album_key.clone(),
                        album_title: album_title.clone(),
                        album_artist: album_artist.clone(),
                        album_track_count,
                        selected_change_count,
                        is_album_wide,
                        track,
                    })
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ReviewCategory {
    Casing,
    Year,
    Genre,
}

impl ReviewCategory {
    pub(super) const fn name(self) -> &'static str {
        match self {
            Self::Casing => "casing",
            Self::Year => "year",
            Self::Genre => "genre",
        }
    }

    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::Casing => strings::DOCTOR_FILTER_CASING,
            Self::Year => strings::DOCTOR_FILTER_YEAR,
            Self::Genre => strings::DOCTOR_FILTER_GENRE,
        }
    }

    pub(super) const fn matches(self, class: ProblemClass) -> bool {
        match self {
            Self::Casing => matches!(
                class,
                ProblemClass::CasingWhitespace | ProblemClass::MissingAlbumArtist
            ),
            Self::Year => matches!(class, ProblemClass::MissingWrongYear),
            Self::Genre => matches!(class, ProblemClass::GenreVariant),
        }
    }

    pub(super) fn problem_classes(self) -> HashSet<ProblemClass> {
        [
            ProblemClass::CasingWhitespace,
            ProblemClass::MissingAlbumArtist,
            ProblemClass::GenreVariant,
            ProblemClass::MissingWrongYear,
            ProblemClass::MissingRecordingMbid,
        ]
        .into_iter()
        .filter(|class| self.matches(*class))
        .collect()
    }
}

pub(super) fn available_categories(review: &DoctorReviewSession) -> Vec<ReviewCategory> {
    [
        ReviewCategory::Casing,
        ReviewCategory::Year,
        ReviewCategory::Genre,
    ]
    .into_iter()
    .filter(|category| {
        review
            .rows()
            .iter()
            .any(|row| category.matches(row.problem_class))
    })
    .collect()
}

pub(super) fn value_text(value: &DoctorValue) -> String {
    match value {
        DoctorValue::Empty => strings::text(strings::DOCTOR_EMPTY_VALUE),
        DoctorValue::Text(value) => value.clone(),
        DoctorValue::Year(value) => value.to_string(),
    }
}

/// What the pill reads: the spelling and how many tracks carry it. The
/// evidence behind it goes to [`candidate_description`], which feeds the
/// tooltip and the accessible description — printed on the button itself it
/// is a line wider than the window.
pub(super) fn candidate_label(candidate: &DoctorCandidate) -> String {
    strings::doctor_candidate(&value_text(&candidate.value), candidate.count)
}

pub(super) fn candidate_description(candidate: &DoctorCandidate) -> String {
    let mut parts = vec![strings::doctor_candidate(
        &value_text(&candidate.value),
        candidate.count,
    )];
    for evidence in &candidate.evidence {
        let source = strings::text(match evidence.source {
            RemoteEvidenceSource::MusicBrainz => strings::DOCTOR_SOURCE_MUSICBRAINZ,
            RemoteEvidenceSource::AcoustId => strings::DOCTOR_SOURCE_ACOUSTID,
        });
        parts.push(strings::doctor_remote_confidence(
            &source,
            evidence.confidence,
        ));
        for (label, value) in [
            (strings::TAG_ARTIST, evidence.artist.as_deref()),
            (strings::TAG_TITLE, evidence.title.as_deref()),
            (strings::TAG_ALBUM, evidence.album.as_deref()),
        ] {
            if let Some(value) = value.filter(|value| !value.is_empty()) {
                parts.push(strings::doctor_evidence_value(&strings::text(label), value));
            }
        }
        if let Some(year) = evidence.year {
            parts.push(strings::doctor_evidence_value(
                &strings::text(strings::TAG_YEAR),
                &year.to_string(),
            ));
        }
        if let Some(duration_ms) = evidence.duration_ms {
            parts.push(strings::doctor_duration_ms(duration_ms));
        }
        if let Some(delta_ms) = evidence.duration_delta_ms {
            parts.push(strings::doctor_duration_delta_ms(delta_ms));
        }
    }
    parts.join(" · ")
}

pub(super) fn row_selectable(model: &ReviewRowModel) -> bool {
    !model.selectable_row_ids.is_empty()
        && !matches!(
            model.outcome.as_ref().map(|outcome| outcome.state),
            Some(DoctorWriteRowState::Applied)
        )
}

pub(super) const fn outcome_label(outcome: DoctorWriteRowState) -> &'static str {
    match outcome {
        DoctorWriteRowState::Applied => strings::DOCTOR_STATUS_APPLIED,
        DoctorWriteRowState::Reverted => strings::DOCTOR_STATUS_REVERTED,
        DoctorWriteRowState::Cancelled => strings::DOCTOR_STATUS_REMAINING,
        DoctorWriteRowState::Conflict => strings::DOCTOR_STATUS_CONFLICT,
        DoctorWriteRowState::Unavailable => strings::DOCTOR_STATUS_STALE,
        DoctorWriteRowState::Failed => strings::DOCTOR_STATUS_FAILED,
    }
}

pub(super) const fn field_label(field: DoctorField) -> &'static str {
    match field {
        DoctorField::Title => strings::TAG_TITLE,
        DoctorField::Artist => strings::TAG_ARTIST,
        DoctorField::Album => strings::TAG_ALBUM,
        DoctorField::AlbumArtist => strings::TAG_ALBUM_ARTIST,
        DoctorField::Year => strings::TAG_YEAR,
        DoctorField::Genre => strings::TAG_GENRE,
        DoctorField::RecordingMbid => strings::DOCTOR_RECORDING_MBID,
    }
}

#[cfg(test)]
mod tests {
    use reprise_core::library_doctor::{
        DoctorCandidate, DoctorValue, ProposalSource, RemoteEvidence, RemoteEvidenceSource,
    };

    use super::{
        candidate_description, candidate_label, confidence_presentation, layout_for_width,
        ConfidenceTone, ReviewLayout,
    };

    #[test]
    fn doc_4b_confidence_uses_redundant_source_text_tone_and_warning() {
        let local = confidence_presentation(ProposalSource::Local, 20);
        assert_eq!(local.label, "Local");
        assert_eq!(local.tone, ConfidenceTone::Accent);
        assert!(!local.warning);

        let high = confidence_presentation(ProposalSource::MusicBrainz, 85);
        assert_eq!(high.label, "MusicBrainz · 85%");
        assert_eq!(high.tone, ConfidenceTone::Normal);
        assert!(!high.warning);

        let medium = confidence_presentation(ProposalSource::AcoustId, 50);
        assert_eq!(medium.label, "AcoustID · 50%");
        assert_eq!(medium.tone, ConfidenceTone::Warning);
        assert!(!medium.warning);

        let low = confidence_presentation(ProposalSource::AcoustId, 49);
        assert_eq!(low.label, "AcoustID · 49% · low confidence");
        assert_eq!(low.tone, ConfidenceTone::Error);
        assert!(low.warning);
    }

    #[test]
    fn doc_3b_breakpoint_changes_layout_without_changing_row_identity() {
        assert_eq!(layout_for_width(639), ReviewLayout::Narrow);
        assert_eq!(layout_for_width(640), ReviewLayout::Wide);
    }

    #[test]
    fn doc_4b_manual_candidate_exposes_available_remote_evidence() {
        let candidate = DoctorCandidate {
            value: DoctorValue::Text("Canonical title".into()),
            count: 2,
            evidence: vec![RemoteEvidence {
                source: RemoteEvidenceSource::MusicBrainz,
                confidence: 78,
                recording_mbid: None,
                release_mbid: None,
                release_group_mbid: None,
                artist_mbid: None,
                release_artist_mbid: None,
                title: Some("Canonical title".into()),
                artist: Some("Canonical artist".into()),
                album: Some("Canonical album".into()),
                year: Some(1999),
                duration_ms: Some(120_000),
                duration_delta_ms: Some(250),
            }],
        };

        let description = candidate_description(&candidate);

        assert!(description.contains("MusicBrainz · 78%"));
        assert!(description.contains("Canonical artist"));
        assert!(description.contains("Canonical title"));
        assert!(description.contains("Canonical album"));
        assert!(description.contains("1999"));
        assert!(description.contains("250 ms"));
    }

    /// The pill is a choice between spellings, so it shows the spelling and
    /// how often it occurs. Its evidence — source, confidence, the matched
    /// release's artist, title, album, year and duration — belongs to the
    /// description a screen reader reads and the tooltip a pointer reveals.
    /// Printed on the button it produced a single line wider than the window,
    /// truncated mid-sentence, with the spelling itself pushed off the page.
    #[test]
    fn doc_4b_a_candidate_pill_shows_the_spelling_and_leaves_evidence_to_its_description() {
        let candidate = DoctorCandidate {
            value: DoctorValue::Text("The Beatles".into()),
            count: 9,
            evidence: vec![RemoteEvidence {
                source: RemoteEvidenceSource::MusicBrainz,
                confidence: 100,
                recording_mbid: None,
                release_mbid: None,
                release_group_mbid: None,
                artist_mbid: None,
                release_artist_mbid: None,
                title: Some("Dehumanized".into()),
                artist: Some("Bring Me the Horizon".into()),
                album: Some("Count Your Blessings".into()),
                year: Some(2026),
                duration_ms: Some(268_000),
                duration_delta_ms: Some(12),
            }],
        };

        let label = candidate_label(&candidate);
        let description = candidate_description(&candidate);

        assert!(label.contains("The Beatles"), "the spelling must be shown");
        assert!(label.contains('9'), "so must how often it occurs");
        for evidence in ["MusicBrainz", "Bring Me the Horizon", "268000", "2026"] {
            assert!(
                !label.contains(evidence),
                "evidence leaked onto the pill: {label}"
            );
        }
        assert!(
            description.contains("MusicBrainz") && description.contains("Bring Me the Horizon"),
            "the description still carries the full evidence"
        );
    }
}
