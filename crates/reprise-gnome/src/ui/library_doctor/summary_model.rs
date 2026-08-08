//! What the Library Doctor page shows, decided without a widget in sight.
//!
//! Two rules live here and nowhere else:
//!
//! * **Nothing that counts zero is rendered.** Not a detail line, not a block.
//!   A block with no lines left is not a block; three empty blocks are the
//!   empty state, not a page of zeros.
//! * **Every number describes the scan that produced it.** The counts come from
//!   the stored scan, the scope and the network flag come from that scan's own
//!   options — never from whatever the controls happen to say now.

use reprise_core::library_doctor::{
    group_review_rows, scan_summary, DoctorField, DoctorProblemCount, DoctorReviewFilter,
    DoctorReviewSession, DoctorScan, DoctorScanSummary, DoctorWriteReport, DoctorWriteRowState,
    ProblemClass,
};

use super::progress_card::DoctorJobKind;
use crate::ui::strings;

pub(super) const PROBLEM_CLASSES: [ProblemClass; 5] = [
    ProblemClass::CasingWhitespace,
    ProblemClass::MissingAlbumArtist,
    ProblemClass::GenreVariant,
    ProblemClass::MissingWrongYear,
    ProblemClass::MissingRecordingMbid,
];

/// Did the quiet write actually happen? The summary is only ever rendered
/// after that question has an answer, which is what keeps the applied block
/// from claiming a past tense it has not earned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum QuietOutcome {
    /// The job ran. `None` means it had nothing to write.
    Applied(Option<DoctorWriteReport>),
    /// It failed, was refused by the write gate, or never started.
    Failed,
    /// It ran and was then undone. The fixes are off disk again, so the block
    /// that reported them has nothing left to say.
    Reverted,
}

/// Which of the Doctor's screens is showing. One value, one screen — the
/// running scan and the finished result are different pages and can never be
/// half-mixed into each other.
#[derive(Debug, Clone)]
pub(super) enum DoctorPageState {
    Start,
    Running {
        kind: DoctorJobKind,
        completed: usize,
        total: usize,
        live: DoctorScanSummary,
    },
    Summary {
        scan: Box<DoctorScan>,
        quiet: QuietOutcome,
    },
    PostApply,
}

/// The two counters the running page may show. Both are forecasts: the quiet
/// write starts when the scan ends, so mid-scan nothing is on disk yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct LiveCounters {
    pub(super) will_fix_quietly: usize,
    pub(super) waiting_for_you: usize,
}

impl LiveCounters {
    pub(super) const fn from_summary(summary: &DoctorScanSummary) -> Self {
        Self {
            will_fix_quietly: summary.auto_applied_changes,
            waiting_for_you: summary.review_changes,
        }
    }
}

/// Scope, network and skipped tracks of one scan, for the muted line under the
/// result title.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ScanFacts {
    scope_kind: String,
    remote_enabled: bool,
    skipped_tracks: usize,
}

impl ScanFacts {
    fn from_scan(scan: &DoctorScan) -> Self {
        Self {
            scope_kind: scan.scope_kind.clone(),
            remote_enabled: scan.options.remote_enabled,
            skipped_tracks: scan.skipped_tracks,
        }
    }

    pub(super) fn label(&self) -> String {
        strings::doctor_scan_facts(
            &strings::text(scope_label(&self.scope_kind)),
            &strings::text(if self.remote_enabled {
                strings::DOCTOR_REMOTE_ON
            } else {
                strings::DOCTOR_REMOTE_OFF
            }),
            Some(self.skipped_tracks),
        )
    }
}

/// `DoctorScopeRequest::kind()` in the core writes these three strings; an
/// unknown one can only mean a scan stored by a newer build, so fall back to
/// the widest truthful label rather than inventing one.
fn scope_label(scope_kind: &str) -> &'static str {
    match scope_kind {
        "current_view" => strings::DOCTOR_SCOPE_CURRENT_VIEW,
        "selection" => strings::DOCTOR_SCOPE_SELECTION,
        _ => strings::DOCTOR_SCOPE_WHOLE_LIBRARY,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct AppliedBlock {
    pub(super) changes: usize,
    pub(super) spacing_casing: usize,
    pub(super) recording_mbids: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ReviewLine {
    pub(super) class: ProblemClass,
    pub(super) changes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ReviewBlock {
    pub(super) changes: usize,
    pub(super) albums: usize,
    pub(super) lines: Vec<ReviewLine>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SummaryBlocks {
    pub(super) applied: Option<AppliedBlock>,
    pub(super) review: Option<ReviewBlock>,
    pub(super) conflicts: Option<usize>,
    pub(super) checked_tracks: usize,
    pub(super) skipped_tracks: usize,
    pub(super) facts: ScanFacts,
}

impl SummaryBlocks {
    pub(super) fn from_scan(scan: &DoctorScan, remote_visible: bool, quiet: &QuietOutcome) -> Self {
        let summary = scan_summary(scan, remote_visible);
        let session = DoctorReviewSession::from_scan(scan.clone(), DoctorReviewFilter::NeedsReview);
        let albums = group_review_rows(scan, &session).len();
        let lines = PROBLEM_CLASSES
            .into_iter()
            .filter_map(|class| {
                let DoctorProblemCount { review, .. } = summary.counts_for(class);
                (review > 0).then_some(ReviewLine {
                    class,
                    changes: review,
                })
            })
            .collect();
        Self {
            applied: applied_block(&summary, quiet),
            review: (summary.review_changes > 0).then_some(ReviewBlock {
                changes: summary.review_changes,
                albums,
                lines,
            }),
            conflicts: (summary.unresolved_groups > 0).then_some(summary.unresolved_groups),
            checked_tracks: summary.checked_tracks,
            skipped_tracks: summary.skipped_tracks,
            facts: ScanFacts::from_scan(scan),
        }
    }

    pub(super) const fn visible_count(&self) -> usize {
        self.applied.is_some() as usize
            + self.review.is_some() as usize
            + self.conflicts.is_some() as usize
    }

    pub(super) const fn is_empty(&self) -> bool {
        self.visible_count() == 0
    }
}

/// The applied block reports the write, not the plan.
///
/// A failed or refused quiet job leaves no block at all: nothing was written,
/// so nothing may say it was. The failure reaches the user as the toast
/// `abandon_auto_apply` already shows, and the findings stay reachable through
/// the sidebar entry it refreshes.
fn applied_block(summary: &DoctorScanSummary, quiet: &QuietOutcome) -> Option<AppliedBlock> {
    let report = match quiet {
        QuietOutcome::Failed | QuietOutcome::Reverted => return None,
        QuietOutcome::Applied(report) => report.as_ref(),
    };
    let Some(report) = report else {
        // The job ran with an empty plan. Fall back to what the scan itself
        // classified as auto-applied, which is zero in that case.
        let mbids = summary
            .counts_for(ProblemClass::MissingRecordingMbid)
            .auto_applied;
        return (summary.auto_applied_changes > 0).then_some(AppliedBlock {
            changes: summary.auto_applied_changes,
            spacing_casing: summary.auto_applied_changes.saturating_sub(mbids),
            recording_mbids: mbids,
        });
    };
    let applied_rows = report
        .rows
        .iter()
        .filter(|row| row.state == DoctorWriteRowState::Applied)
        .collect::<Vec<_>>();
    let changes = applied_rows.len();
    let recording_mbids = applied_rows
        .iter()
        .filter(|row| row.field == DoctorField::RecordingMbid)
        .count();
    (changes > 0).then_some(AppliedBlock {
        changes,
        spacing_casing: changes.saturating_sub(recording_mbids),
        recording_mbids,
    })
}

pub(super) fn problem_title(class: ProblemClass) -> String {
    strings::text(match class {
        ProblemClass::CasingWhitespace => strings::DOCTOR_CASING_WHITESPACE,
        ProblemClass::MissingAlbumArtist => strings::DOCTOR_MISSING_ALBUM_ARTIST,
        ProblemClass::GenreVariant => strings::DOCTOR_GENRE_VARIANTS,
        ProblemClass::MissingWrongYear => strings::DOCTOR_MISSING_WRONG_YEAR,
        ProblemClass::MissingRecordingMbid => strings::DOCTOR_MISSING_RECORDING_MBID,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use reprise_core::library_doctor::{
        DoctorProposal, DoctorScanOptions, DoctorUnresolvedGroup, DoctorValue, DoctorWriteRow,
        ProposalSource,
    };

    fn proposal(source: ProposalSource, class: ProblemClass, track_id: i64) -> DoctorProposal {
        DoctorProposal {
            track_id,
            field: if class == ProblemClass::MissingRecordingMbid {
                DoctorField::RecordingMbid
            } else {
                DoctorField::Artist
            },
            current: DoctorValue::Text("old".into()),
            proposed: DoctorValue::Text("new".into()),
            source,
            confidence: 91,
            preselected: source == ProposalSource::Local,
            never_preselect: false,
            problem_class: class,
            resolved_release_mbid: None,
            evidence: Vec::new(),
            local_fallback: None,
        }
    }

    fn scan(proposals: Vec<DoctorProposal>, groups: usize) -> DoctorScan {
        DoctorScan {
            id: 1,
            scope_kind: "whole_library".into(),
            created_at: 2,
            options: DoctorScanOptions::local_only(),
            checked_tracks: 2,
            skipped_tracks: 0,
            track_ids: vec![1, 2],
            tracks: Vec::new(),
            proposals,
            unresolved_groups: (0..groups)
                .map(|index| DoctorUnresolvedGroup {
                    field: DoctorField::Artist,
                    group_key: index.to_string(),
                    candidates: Vec::new(),
                    members: Vec::new(),
                    local_fallback: None,
                })
                .collect(),
        }
    }

    fn write_report(applied: usize, mbids: usize) -> DoctorWriteReport {
        DoctorWriteReport {
            job_id: 7,
            source_job_id: None,
            updated_tracks: applied,
            cancelled_tracks: 0,
            failed_tracks: 0,
            conflict_tracks: 0,
            unavailable_tracks: 0,
            rows: (0..applied)
                .map(|index| DoctorWriteRow {
                    row_id: None,
                    track_id: index as i64,
                    path: format!("/test/{index}.flac").into(),
                    field: if index < mbids {
                        DoctorField::RecordingMbid
                    } else {
                        DoctorField::Artist
                    },
                    expected: DoctorValue::Text("old".into()),
                    proposed: DoctorValue::Text("new".into()),
                    state: DoctorWriteRowState::Applied,
                    file_written: true,
                    error_kind: None,
                    error: None,
                })
                .collect(),
        }
    }

    fn applied(report: DoctorWriteReport) -> QuietOutcome {
        QuietOutcome::Applied(Some(report))
    }

    #[test]
    fn doc_9a_summary_renders_three_blocks_and_never_a_zero_row() {
        let blocks = SummaryBlocks::from_scan(
            &scan(
                vec![proposal(
                    ProposalSource::Local,
                    ProblemClass::CasingWhitespace,
                    1,
                )],
                0,
            ),
            true,
            &applied(write_report(1, 0)),
        );
        assert!(blocks.applied.is_some());
        assert!(blocks.review.is_none());
        assert!(blocks.conflicts.is_none());
        assert_eq!(blocks.visible_count(), 1);
    }

    #[test]
    fn doc_9a_summary_omits_the_conflicts_block_without_conflicts() {
        assert!(
            SummaryBlocks::from_scan(&scan(Vec::new(), 0), true, &QuietOutcome::Applied(None))
                .conflicts
                .is_none()
        );
    }

    #[test]
    fn doc_9a_every_visible_count_is_a_written_change_count() {
        let proposals = (1..=11)
            .map(|track_id| {
                proposal(
                    ProposalSource::MusicBrainz,
                    ProblemClass::MissingAlbumArtist,
                    track_id,
                )
            })
            .collect();
        let blocks =
            SummaryBlocks::from_scan(&scan(proposals, 0), true, &QuietOutcome::Applied(None));
        assert_eq!(blocks.review.unwrap().changes, 11);
    }

    #[test]
    fn doc_9a_a_scan_with_nothing_to_show_is_the_empty_state() {
        let blocks =
            SummaryBlocks::from_scan(&scan(Vec::new(), 0), true, &QuietOutcome::Applied(None));
        assert!(blocks.is_empty());
    }

    #[test]
    fn doc_9a_a_detail_line_that_would_read_zero_is_not_emitted() {
        // Only MusicBrainz IDs were written, so the spacing/casing line has
        // nothing to say and must not appear as "0 stray spaces…".
        let blocks = SummaryBlocks::from_scan(
            &scan(
                vec![proposal(
                    ProposalSource::Local,
                    ProblemClass::MissingRecordingMbid,
                    1,
                )],
                0,
            ),
            true,
            &applied(write_report(2, 2)),
        );
        let block = blocks.applied.expect("MusicBrainz IDs were written");
        assert_eq!(block.recording_mbids, 2);
        assert_eq!(block.spacing_casing, 0);
    }

    #[test]
    fn doc_9a_review_lines_only_exist_for_classes_with_findings() {
        let blocks = SummaryBlocks::from_scan(
            &scan(
                vec![proposal(
                    ProposalSource::MusicBrainz,
                    ProblemClass::MissingWrongYear,
                    1,
                )],
                0,
            ),
            true,
            &QuietOutcome::Applied(None),
        );
        let review = blocks.review.expect("one year change needs review");
        assert_eq!(review.lines.len(), 1);
        assert_eq!(review.lines[0].class, ProblemClass::MissingWrongYear);
    }

    #[test]
    fn doc_9a_a_failed_quiet_write_claims_nothing() {
        let blocks = SummaryBlocks::from_scan(
            &scan(
                vec![proposal(
                    ProposalSource::Local,
                    ProblemClass::CasingWhitespace,
                    1,
                )],
                0,
            ),
            true,
            &QuietOutcome::Failed,
        );
        assert!(
            blocks.applied.is_none(),
            "nothing was written, so nothing may say it was"
        );
    }

    #[test]
    fn doc_9a_the_applied_block_reports_the_write_not_the_plan() {
        let blocks = SummaryBlocks::from_scan(
            &scan(
                (1..=9)
                    .map(|id| proposal(ProposalSource::Local, ProblemClass::CasingWhitespace, id))
                    .collect(),
                0,
            ),
            true,
            &applied(write_report(4, 1)),
        );
        let block = blocks.applied.expect("four rows were written");
        assert_eq!(block.changes, 4);
        assert_eq!(block.recording_mbids, 1);
        assert_eq!(block.spacing_casing, 3);
    }

    #[test]
    fn doc_9a_scan_facts_describe_the_scan_not_the_controls() {
        let mut scan = scan(Vec::new(), 0);
        scan.scope_kind = "selection".into();
        scan.skipped_tracks = 3;
        // `remote_visible = true` is the live control; the scan itself ran
        // local-only and the facts line has to say so.
        let blocks = SummaryBlocks::from_scan(&scan, true, &QuietOutcome::Applied(None));
        let facts = blocks.facts.label();
        assert!(facts.starts_with("Selection"), "{facts}");
        assert!(facts.contains("MusicBrainz off"), "{facts}");
        assert!(facts.contains("3 skipped"), "{facts}");
    }

    #[test]
    fn doc_9a_scan_facts_stay_silent_about_zero_skipped_tracks() {
        let blocks =
            SummaryBlocks::from_scan(&scan(Vec::new(), 0), false, &QuietOutcome::Applied(None));
        assert_eq!(blocks.facts.label(), "Whole Library · MusicBrainz off");
    }

    #[test]
    fn doc_2c_the_running_page_counters_are_forecasts_from_the_live_summary() {
        let mut live = DoctorScanSummary::default();
        live.auto_applied_changes = 3;
        live.review_changes = 2;
        let counters = LiveCounters::from_summary(&live);
        assert_eq!(counters.will_fix_quietly, 3);
        assert_eq!(counters.waiting_for_you, 2);
    }

    #[test]
    fn doc_9a_singular_forms_go_through_ngettext() {
        assert_eq!(strings::doctor_needs_review(1), "1 change needs your eye");
        assert_eq!(strings::doctor_needs_review(2), "2 changes need your eye");
        assert_eq!(strings::doctor_already_applied(1), "1 fix already applied");
        assert_eq!(
            strings::doctor_unresolved_spellings(1),
            "1 spelling conflict, no clear winner"
        );
        assert_eq!(strings::doctor_across_albums(1), "across 1 album");
        assert_eq!(
            strings::doctor_spacing_casing_line(1),
            "1 stray space and casing correction"
        );
    }
}
