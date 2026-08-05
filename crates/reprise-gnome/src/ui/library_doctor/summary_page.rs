use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use reprise_core::db::Db;
use reprise_core::library_doctor::{
    group_review_rows, scan_summary, DoctorProblemCount, DoctorReviewFilter, DoctorReviewSession,
    DoctorScan, DoctorScanSummary, DoctorWriteReport, DoctorWriteRowState, ProblemClass,
};

use super::result_pages::DoctorResultPages;
use super::start_page::DoctorStartPage;
use crate::ui::strings;

const PROBLEM_CLASSES: [ProblemClass; 5] = [
    ProblemClass::CasingWhitespace,
    ProblemClass::MissingAlbumArtist,
    ProblemClass::GenreVariant,
    ProblemClass::MissingWrongYear,
    ProblemClass::MissingRecordingMbid,
];

#[derive(Debug, Clone, PartialEq, Eq)]
struct AppliedBlock {
    changes: usize,
    spacing_casing: usize,
    recording_mbids: usize,
    pending: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ReviewLine {
    class: ProblemClass,
    changes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ReviewBlock {
    changes: usize,
    albums: Option<usize>,
    lines: Vec<ReviewLine>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SummaryBlocks {
    applied: Option<AppliedBlock>,
    review: Option<ReviewBlock>,
    conflicts: Option<usize>,
    checked_tracks: usize,
    skipped_tracks: usize,
    partial: bool,
}

impl SummaryBlocks {
    fn from_scan(scan: &DoctorScan, remote_visible: bool) -> Self {
        let summary = scan_summary(scan, remote_visible);
        let session = DoctorReviewSession::from_scan(scan.clone(), DoctorReviewFilter::NeedsReview);
        let albums = group_review_rows(scan, &session).len();
        Self::from_summary(summary, albums, false)
    }

    fn from_partial(summary: DoctorScanSummary) -> Self {
        Self::from_summary(summary, 0, true)
    }

    fn from_summary(summary: DoctorScanSummary, albums: usize, partial: bool) -> Self {
        let mbids = summary
            .counts_for(ProblemClass::MissingRecordingMbid)
            .auto_applied;
        let applied = (summary.auto_applied_changes > 0).then_some(AppliedBlock {
            changes: summary.auto_applied_changes,
            spacing_casing: summary.auto_applied_changes.saturating_sub(mbids),
            recording_mbids: mbids,
            pending: partial,
        });
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
        let review = (summary.review_changes > 0).then_some(ReviewBlock {
            changes: summary.review_changes,
            albums: (!partial).then_some(albums),
            lines,
        });
        Self {
            applied,
            review,
            conflicts: (summary.unresolved_groups > 0).then_some(summary.unresolved_groups),
            checked_tracks: summary.checked_tracks,
            skipped_tracks: summary.skipped_tracks,
            partial,
        }
    }

    fn with_applied_report(mut self, report: Option<&DoctorWriteReport>) -> Self {
        let Some(block) = &mut self.applied else {
            return self;
        };
        block.pending = false;
        if let Some(report) = report {
            let applied_rows = report
                .rows
                .iter()
                .filter(|row| row.state == DoctorWriteRowState::Applied)
                .collect::<Vec<_>>();
            block.changes = applied_rows.len();
            block.recording_mbids = applied_rows
                .iter()
                .filter(|row| row.field == reprise_core::library_doctor::DoctorField::RecordingMbid)
                .count();
            block.spacing_casing = block.changes.saturating_sub(block.recording_mbids);
        }
        self.applied = (block.changes > 0).then_some(block.clone());
        self
    }

    const fn visible_count(&self) -> usize {
        self.applied.is_some() as usize
            + self.review.is_some() as usize
            + self.conflicts.is_some() as usize
    }

    const fn is_empty(&self) -> bool {
        self.visible_count() == 0
    }
}

struct DoctorSummaryPanel {
    root: gtk4::Box,
    heading: gtk4::Label,
    blocks: gtk4::Box,
    applied_undo: gtk4::Button,
    review: gtk4::Button,
    scan_again: gtk4::Button,
}

impl DoctorSummaryPanel {
    fn new() -> Self {
        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 16);
        let heading = gtk4::Label::builder()
            .xalign(0.0)
            .css_classes(["title-2"])
            .build();
        heading.set_label(&strings::text(strings::DOCTOR_RESULTS_SO_FAR));
        root.append(&heading);
        let blocks = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
        root.append(&blocks);
        let applied_undo = gtk4::Button::with_label(&strings::text(strings::DOCTOR_UNDO));
        let review = gtk4::Button::builder()
            .css_classes(["suggested-action"])
            .build();
        let scan_again = gtk4::Button::with_label(&strings::text(strings::DOCTOR_SCAN_AGAIN));
        root.append(&scan_again);
        root.append(&gtk4::Label::new(Some(&strings::text(
            strings::DOCTOR_RESULTS_KEPT,
        ))));
        Self {
            root,
            heading,
            blocks,
            applied_undo,
            review,
            scan_again,
        }
    }

    fn widget(&self) -> &gtk4::Box {
        &self.root
    }

    fn render(&self, model: &SummaryBlocks, actions_locked: bool) {
        self.heading.set_label(&if model.partial {
            strings::text(strings::DOCTOR_RESULTS_SO_FAR)
        } else {
            strings::doctor_tracks_checked_heading(model.checked_tracks)
        });
        remove_all(&self.blocks);
        if let Some(applied) = &model.applied {
            let card = block_card(&if applied.pending {
                strings::doctor_fixes_to_apply(applied.changes)
            } else {
                strings::doctor_fixes_applied(applied.changes)
            });
            append_line(
                &card,
                &strings::doctor_spacing_casing_line(applied.spacing_casing),
            );
            append_line(
                &card,
                &if applied.pending {
                    strings::doctor_mbid_line_pending(applied.recording_mbids)
                } else {
                    strings::doctor_mbid_line(applied.recording_mbids)
                },
            );
            self.applied_undo
                .set_sensitive(!actions_locked && !applied.pending);
            card.append(&self.applied_undo);
            self.blocks.append(&card);
        }
        if let Some(review) = &model.review {
            let card = block_card(&strings::doctor_changes_need_your_eye(review.changes));
            for line in &review.lines {
                append_line(
                    &card,
                    &format!("{} · {}", problem_title(line.class), line.changes),
                );
            }
            if let Some(albums) = review.albums {
                append_line(&card, &strings::doctor_across_albums(albums));
            }
            self.review
                .set_label(&strings::doctor_review_changes(review.changes));
            self.review.set_sensitive(!actions_locked);
            card.append(&self.review);
            self.blocks.append(&card);
        }
        if let Some(conflicts) = model.conflicts {
            let card = block_card(&strings::doctor_conflicts_headline(conflicts));
            card.add_css_class("card");
            append_line(&card, &strings::text(strings::DOCTOR_CONFLICTS_BODY));
            self.blocks.append(&card);
        }
        append_line(
            &self.blocks,
            &strings::doctor_checked_counts(model.checked_tracks, model.skipped_tracks),
        );
        if actions_locked {
            append_line(
                &self.blocks,
                &strings::text(strings::DOCTOR_CONTROLS_LOCKED),
            );
        }
        self.scan_again.set_sensitive(!actions_locked);
    }
}

pub(in crate::ui) struct LibraryDoctorPage {
    navigation_page: adw::NavigationPage,
    stack: gtk4::Stack,
    start: DoctorStartPage,
    summary: DoctorSummaryPanel,
    results: DoctorResultPages,
    current_scan: RefCell<Option<DoctorScan>>,
    partial_summary: RefCell<Option<DoctorScanSummary>>,
    quiet_report: RefCell<Option<DoctorWriteReport>>,
    auto_complete: Cell<bool>,
    auto_running: Cell<bool>,
}

impl LibraryDoctorPage {
    pub(in crate::ui) fn new(
        conn: &Rc<Db>,
        parent: &adw::ApplicationWindow,
        fingerprint_available: bool,
        on_remote_changed: Rc<dyn Fn(bool)>,
    ) -> Rc<Self> {
        let start = DoctorStartPage::new(conn, parent, fingerprint_available, on_remote_changed);
        let summary = DoctorSummaryPanel::new();
        let results = DoctorResultPages::new();
        let stack = gtk4::Stack::new();
        stack.add_named(start.widget(), Some("start"));
        stack.add_named(summary.widget(), Some("summary"));
        stack.add_named(results.widget(), Some("result"));
        let content = adw::Clamp::builder()
            .maximum_size(760)
            .tightening_threshold(560)
            .child(&stack)
            .build();
        content.set_margin_top(24);
        content.set_margin_bottom(36);
        content.set_margin_start(24);
        content.set_margin_end(24);
        let scrolled = gtk4::ScrolledWindow::builder()
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .child(&content)
            .build();
        let toolbar = adw::ToolbarView::new();
        toolbar.add_top_bar(&adw::HeaderBar::new());
        toolbar.set_content(Some(&scrolled));
        let navigation_page = adw::NavigationPage::builder()
            .title(strings::text(strings::LIBRARY_DOCTOR))
            .tag("library-doctor")
            .child(&toolbar)
            .build();
        Rc::new(Self {
            navigation_page,
            stack,
            start,
            summary,
            results,
            current_scan: RefCell::new(None),
            partial_summary: RefCell::new(None),
            quiet_report: RefCell::new(None),
            auto_complete: Cell::new(true),
            auto_running: Cell::new(false),
        })
    }

    pub(in crate::ui) fn navigation_page(&self) -> &adw::NavigationPage {
        &self.navigation_page
    }

    pub(in crate::ui) fn connect_run(&self, callback: impl Fn() + 'static) {
        self.start.connect_run(callback);
    }

    pub(in crate::ui) fn connect_start_revert(&self, callback: impl Fn() + 'static) {
        self.start.connect_revert(callback);
    }

    pub(in crate::ui) fn connect_review(&self, callback: impl Fn() + 'static) {
        self.summary.review.connect_clicked(move |_| callback());
    }

    pub(in crate::ui) fn connect_summary_undo(&self, callback: impl Fn() + 'static) {
        self.summary
            .applied_undo
            .connect_clicked(move |_| callback());
    }

    pub(in crate::ui) fn connect_scan_again(&self, callback: Rc<dyn Fn()>) {
        {
            let callback = callback.clone();
            self.summary.scan_again.connect_clicked(move |_| callback());
        }
        self.results.connect_scan_again(move || callback());
    }

    pub(in crate::ui) fn connect_result_undo(&self, callback: impl Fn() + 'static) {
        self.results.connect_undo(callback);
    }

    pub(in crate::ui) fn connect_done(&self, callback: impl Fn() + 'static) {
        self.results.connect_done(callback);
    }

    pub(in crate::ui) fn scan(&self) -> Option<DoctorScan> {
        self.current_scan.borrow().clone()
    }

    pub(in crate::ui) fn selected_scope(&self) -> u32 {
        self.start.selected_scope()
    }

    pub(in crate::ui) fn set_selected_scope(&self, scope: u32) {
        self.start.set_selected_scope(scope);
    }

    pub(in crate::ui) fn remote_active(&self) -> bool {
        self.start.remote_active()
    }

    pub(in crate::ui) fn sync_remote_preference(&self, db: &Db) {
        self.start.sync_remote_preference(db);
        self.refresh();
    }

    pub(in crate::ui) fn refresh_remote_availability(&self) {
        self.start.refresh_remote_availability();
    }

    pub(in crate::ui) fn set_running(&self, running: bool) {
        self.start.set_running(running);
        if self.stack.visible_child_name().as_deref() == Some("summary") {
            self.refresh();
        }
    }

    pub(in crate::ui) fn show_start(&self, db: &Db) {
        self.start.refresh(db);
        self.stack.set_visible_child_name("start");
    }

    pub(in crate::ui) fn set_scan(&self, scan: Option<DoctorScan>) {
        *self.current_scan.borrow_mut() = scan;
        self.partial_summary.borrow_mut().take();
        self.quiet_report.borrow_mut().take();
        self.auto_complete.set(true);
        self.auto_running.set(false);
        self.refresh();
    }

    pub(in crate::ui) fn set_scan_pending_auto(&self, scan: DoctorScan) {
        *self.current_scan.borrow_mut() = Some(scan);
        self.partial_summary.borrow_mut().take();
        self.quiet_report.borrow_mut().take();
        self.auto_complete.set(false);
        self.auto_running.set(true);
        self.refresh();
    }

    pub(in crate::ui) fn complete_auto_apply(&self, report: Option<DoctorWriteReport>) {
        *self.quiet_report.borrow_mut() = report;
        self.auto_complete.set(true);
        self.auto_running.set(false);
        self.refresh();
    }

    pub(in crate::ui) fn fail_auto_apply(&self) {
        self.auto_running.set(false);
        self.refresh();
    }

    pub(in crate::ui) fn quiet_change_count(&self) -> usize {
        self.quiet_report
            .borrow()
            .as_ref()
            .map(applied_change_count)
            .unwrap_or_default()
    }

    pub(in crate::ui) fn begin_partial_scan(&self) {
        self.partial_summary
            .borrow_mut()
            .replace(DoctorScanSummary::default());
        self.refresh();
    }

    pub(in crate::ui) fn set_partial_summary(&self, summary: DoctorScanSummary) {
        if self.partial_summary.borrow().as_ref() != Some(&summary) {
            self.partial_summary.borrow_mut().replace(summary);
            self.refresh();
        }
    }

    pub(in crate::ui) fn clear_partial_scan(&self) {
        self.partial_summary.borrow_mut().take();
    }

    pub(in crate::ui) fn show_post_apply(
        &self,
        report: &DoctorWriteReport,
        albums: usize,
        conflicts: usize,
    ) {
        self.results
            .show_post_apply(report, albums, self.quiet_change_count(), conflicts);
        self.stack.set_visible_child_name("result");
    }

    pub(in crate::ui) fn refresh(&self) {
        if let Some(partial) = *self.partial_summary.borrow() {
            let blocks = SummaryBlocks::from_partial(partial);
            self.summary.render(&blocks, true);
            self.stack.set_visible_child_name("summary");
            return;
        }
        let scan = self.current_scan.borrow().clone();
        let Some(scan) = scan else {
            self.stack.set_visible_child_name("start");
            return;
        };
        let mut blocks = SummaryBlocks::from_scan(&scan, self.remote_active());
        if self.auto_complete.get() {
            blocks = blocks.with_applied_report(self.quiet_report.borrow().as_ref());
        } else if let Some(applied) = &mut blocks.applied {
            applied.pending = true;
        }
        if blocks.is_empty() && self.auto_complete.get() {
            self.results
                .show_nothing(blocks.checked_tracks, blocks.skipped_tracks);
            self.stack.set_visible_child_name("result");
        } else {
            self.summary.render(&blocks, self.auto_running.get());
            self.stack.set_visible_child_name("summary");
        }
    }
}

fn applied_change_count(report: &DoctorWriteReport) -> usize {
    report
        .rows
        .iter()
        .filter(|row| row.state == DoctorWriteRowState::Applied)
        .count()
}

fn block_card(title: &str) -> gtk4::Box {
    let card = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
    card.add_css_class("boxed-list");
    let title = gtk4::Label::builder()
        .label(title)
        .xalign(0.0)
        .wrap(true)
        .css_classes(["title-3"])
        .build();
    card.append(&title);
    card
}

fn append_line(container: &gtk4::Box, text: &str) {
    let label = gtk4::Label::builder()
        .label(text)
        .xalign(0.0)
        .wrap(true)
        .css_classes(["dim-label"])
        .build();
    container.append(&label);
}

fn remove_all(container: &gtk4::Box) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }
}

fn problem_title(class: ProblemClass) -> String {
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
    use reprise_core::library_doctor::*;

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
            problem_class: class,
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
        );
        assert!(blocks.applied.is_some());
        assert!(blocks.review.is_none());
        assert!(blocks.conflicts.is_none());
        assert_eq!(blocks.visible_count(), 1);
    }

    #[test]
    fn doc_9a_summary_omits_the_conflicts_block_without_conflicts() {
        assert!(SummaryBlocks::from_scan(&scan(Vec::new(), 0), true)
            .conflicts
            .is_none());
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
        let blocks = SummaryBlocks::from_scan(&scan(proposals, 0), true);
        assert_eq!(blocks.review.unwrap().changes, 11);
    }

    #[test]
    fn doc_2c_running_scan_shows_the_same_two_blocks_counting_up_with_actions_locked() {
        let mut partial = DoctorScanSummary::default();
        partial.auto_applied_changes = 3;
        partial.review_changes = 2;
        let blocks = SummaryBlocks::from_partial(partial);
        assert!(blocks.partial);
        assert_eq!(blocks.visible_count(), 2);
    }

    #[test]
    fn doc_2c_block_one_counts_in_the_future_until_the_quiet_write_finishes() {
        let mut partial = DoctorScanSummary::default();
        partial.auto_applied_changes = 3;
        let blocks = SummaryBlocks::from_partial(partial);
        assert!(blocks.applied.unwrap().pending);
    }
}
