use std::collections::HashMap;

use reprise_core::library_doctor::{
    DoctorCandidate, DoctorField, DoctorReviewRow, DoctorReviewRowState, DoctorReviewSession,
    DoctorScan, DoctorValue, ProposalSource, RemoteEvidenceSource,
};

use crate::ui::strings;

pub(super) const WIDE_BREAKPOINT: i32 = 640;

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ReviewLayout {
    Narrow,
    Wide,
}

#[cfg(test)]
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
pub(super) struct ReviewRowModel {
    pub(super) row: DoctorReviewRow,
    pub(super) track: String,
    pub(super) field: String,
    pub(super) current: String,
    pub(super) proposed: String,
    pub(super) confidence: ConfidencePresentation,
}

impl ReviewRowModel {
    pub(super) fn accessible_description(&self) -> String {
        strings::doctor_review_row_description(
            &self.track,
            &self.field,
            &self.current,
            &self.proposed,
            &self.confidence.label,
        )
    }
}

pub(super) fn rows_for(scan: &DoctorScan, review: &DoctorReviewSession) -> Vec<ReviewRowModel> {
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
    review
        .rows()
        .iter()
        .cloned()
        .map(|row| ReviewRowModel {
            track: titles
                .get(&row.track_id)
                .cloned()
                .unwrap_or_else(|| strings::text(strings::DOCTOR_UNKNOWN_TRACK)),
            field: strings::text(field_label(row.field)),
            current: value_text(&row.current),
            proposed: value_text(&row.proposed),
            confidence: confidence_presentation(row.source, row.confidence),
            row,
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

pub(super) const fn row_selectable(row: &DoctorReviewRow) -> bool {
    matches!(row.state, DoctorReviewRowState::Ready)
}

const fn field_label(field: DoctorField) -> &'static str {
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
        candidate_description, confidence_presentation, layout_for_width, ConfidenceTone,
        ReviewLayout,
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
}
