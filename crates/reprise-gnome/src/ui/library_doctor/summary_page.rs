use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use reprise_core::db::Db;
use reprise_core::library_doctor::{
    DoctorScan, DoctorScanPhase, DoctorScanSummary, DoctorWriteReport, DoctorWriteRowState,
};

use super::progress_card::DoctorJobKind;
use super::result_pages::DoctorResultPages;
use super::running_page::DoctorRunningPanel;
use super::start_page::DoctorStartPage;
use super::summary_cards;
use super::summary_model::{
    problem_title, DoctorPageState, LiveCounters, QuietOutcome, SummaryBlocks,
};
use crate::ui::strings;

/// The content column: flush left, capped, top-weighted. The mockup's
/// `max-width: 700px` inside `padding: 44px 64px`.
const CONTENT_WIDTH: i32 = 700;
const CONTENT_MARGIN_TOP: i32 = 44;
const CONTENT_MARGIN_START: i32 = 64;

struct DoctorSummaryPanel {
    root: gtk4::Box,
    heading: gtk4::Label,
    facts: gtk4::Label,
    blocks: gtk4::Box,
    applied_undo: gtk4::Button,
    review: gtk4::Button,
    scan_again: gtk4::Button,
}

impl DoctorSummaryPanel {
    fn new() -> Self {
        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 24);
        root.set_valign(gtk4::Align::Start);

        let title_block = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
        let heading = gtk4::Label::builder()
            .xalign(0.0)
            .wrap(true)
            .css_classes(["title-2"])
            .build();
        title_block.append(&heading);
        // One muted line of scan facts, directly under the title: scope,
        // network, skipped. It describes the stored scan, never the controls.
        let facts = gtk4::Label::builder()
            .xalign(0.0)
            .wrap(true)
            .css_classes(["dim-label"])
            .build();
        title_block.append(&facts);
        root.append(&title_block);

        let blocks = gtk4::Box::new(gtk4::Orientation::Vertical, 24);
        root.append(&blocks);

        let applied_undo = gtk4::Button::with_label(&strings::text(strings::DOCTOR_UNDO));
        let review = gtk4::Button::builder()
            .css_classes(["suggested-action"])
            .build();

        let footer = gtk4::Box::new(gtk4::Orientation::Horizontal, 18);
        let scan_again = gtk4::Button::builder()
            .label(strings::text(strings::DOCTOR_SCAN_AGAIN))
            .css_classes(["flat"])
            .build();
        footer.append(&scan_again);
        footer.append(
            &gtk4::Label::builder()
                .label(strings::text(strings::DOCTOR_RESULTS_KEPT))
                .xalign(0.0)
                .wrap(true)
                .css_classes(["dim-label"])
                .build(),
        );
        root.append(&footer);

        Self {
            root,
            heading,
            facts,
            blocks,
            applied_undo,
            review,
            scan_again,
        }
    }

    fn widget(&self) -> &gtk4::Box {
        &self.root
    }

    fn render(&self, model: &SummaryBlocks, undo_available: bool) {
        self.heading
            .set_label(&strings::doctor_tracks_checked_heading(
                model.checked_tracks,
            ));
        self.facts.set_label(&model.facts.label());
        // The two action buttons outlive the cards they sit in, so that their
        // signal handlers are connected once. Detach them before the cards go.
        summary_cards::unparent_action(&self.applied_undo);
        summary_cards::unparent_action(&self.review);
        remove_all(&self.blocks);

        if let Some(applied) = &model.applied {
            let mut lines = Vec::new();
            if applied.spacing_casing > 0 {
                lines.push(strings::doctor_spacing_casing_line(applied.spacing_casing));
            }
            if applied.recording_mbids > 0 {
                lines.push(strings::doctor_mbid_line(applied.recording_mbids));
            }
            self.applied_undo.set_sensitive(undo_available);
            self.blocks.append(&summary_cards::applied_card(
                strings::doctor_already_applied(applied.changes),
                lines,
                &self.applied_undo,
            ));
        }

        if let Some(review) = &model.review {
            let mut lines = review
                .lines
                .iter()
                .map(|line| {
                    strings::doctor_review_category(&problem_title(line.class), line.changes)
                })
                .collect::<Vec<_>>();
            if review.albums > 0 {
                lines.push(strings::doctor_across_albums(review.albums));
            }
            self.review
                .set_label(&strings::doctor_review_changes(review.changes));
            self.blocks.append(&summary_cards::review_card(
                strings::doctor_needs_review(review.changes),
                lines,
                &self.review,
            ));
        }

        if let Some(conflicts) = model.conflicts {
            self.blocks.append(&summary_cards::conflicts_card(
                strings::doctor_unresolved_spellings(conflicts),
                strings::text(strings::DOCTOR_CONFLICTS_BODY),
            ));
        }
    }

    /// `DOCTOR_CONTROLS_LOCKED` is a reason, not content. It belongs to the
    /// control it disables, as a tooltip — never as a paragraph in the middle
    /// of the results.
    fn set_controls_locked(&self, locked: bool) {
        let reason = locked.then(|| strings::text(strings::DOCTOR_CONTROLS_LOCKED));
        for button in [&self.applied_undo, &self.review, &self.scan_again] {
            button.set_sensitive(!locked);
            button.set_tooltip_text(reason.as_deref());
        }
    }
}

pub(in crate::ui) struct LibraryDoctorPage {
    navigation_page: adw::NavigationPage,
    stack: gtk4::Stack,
    start: DoctorStartPage,
    running: DoctorRunningPanel,
    summary: DoctorSummaryPanel,
    results: DoctorResultPages,
    state: RefCell<DoctorPageState>,
    /// The most recent complete scan, which outlives the running screens: a
    /// cancelled job must fall back to it rather than to a blank page.
    last_scan: RefCell<Option<DoctorScan>>,
    /// What the quiet write did, once it is known. Survives a job that ends
    /// without producing a new result.
    quiet: RefCell<QuietOutcome>,
    /// Counts published by the running scan, for the two forecast counters.
    live_summary: RefCell<Option<DoctorScanSummary>>,
    undo_available: Cell<bool>,
}

impl LibraryDoctorPage {
    pub(in crate::ui) fn new(
        conn: &Rc<Db>,
        parent: &adw::ApplicationWindow,
        fingerprint_available: bool,
        on_remote_changed: Rc<dyn Fn(bool)>,
    ) -> Rc<Self> {
        let start = DoctorStartPage::new(conn, parent, fingerprint_available, on_remote_changed);
        let running = DoctorRunningPanel::new();
        let summary = DoctorSummaryPanel::new();
        let results = DoctorResultPages::new();
        let stack = gtk4::Stack::new();
        stack.add_named(start.widget(), Some("start"));
        stack.add_named(running.widget(), Some("running"));
        stack.add_named(summary.widget(), Some("summary"));
        stack.add_named(results.widget(), Some("result"));
        // `AdwClamp` centres its child, which is what put this page's title and
        // footer in the middle of a mostly empty screen. Left-align the clamp
        // itself and stop it expanding, so the column starts at the content
        // edge and the page stays top-weighted.
        let content = adw::Clamp::builder()
            .maximum_size(CONTENT_WIDTH)
            .tightening_threshold(CONTENT_WIDTH)
            .halign(gtk4::Align::Start)
            .valign(gtk4::Align::Start)
            .hexpand(false)
            .child(&stack)
            .build();
        content.set_margin_top(CONTENT_MARGIN_TOP);
        content.set_margin_bottom(36);
        content.set_margin_start(CONTENT_MARGIN_START);
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
            running,
            summary,
            results,
            state: RefCell::new(DoctorPageState::Start),
            last_scan: RefCell::new(None),
            quiet: RefCell::new(QuietOutcome::Applied(None)),
            live_summary: RefCell::new(None),
            undo_available: Cell::new(false),
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

    pub(in crate::ui) fn connect_cancel(&self, callback: impl Fn() + 'static) {
        self.running.connect_cancel(callback);
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
        self.last_scan.borrow().clone()
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

    pub(in crate::ui) fn show_start(&self, db: &Db) {
        self.start.refresh(db);
        *self.state.borrow_mut() = DoctorPageState::Start;
        self.refresh();
    }

    /// A scan restored from the database, or none at all. Its quiet write
    /// happened in whatever session produced it, so the counts the scan carries
    /// are the truth about it.
    pub(in crate::ui) fn set_scan(&self, scan: Option<DoctorScan>, undo_available: bool) {
        self.undo_available.set(undo_available);
        *self.quiet.borrow_mut() = QuietOutcome::Applied(None);
        *self.last_scan.borrow_mut() = scan;
        *self.live_summary.borrow_mut() = None;
        let quiet = QuietOutcome::Applied(None);
        self.settle(quiet);
    }

    pub(in crate::ui) fn begin_job(&self, kind: DoctorJobKind, total: usize) {
        if kind == DoctorJobKind::Scan {
            *self.live_summary.borrow_mut() = Some(DoctorScanSummary::default());
        }
        self.update_job(kind, 0, total);
    }

    pub(in crate::ui) fn update_job(&self, kind: DoctorJobKind, completed: usize, total: usize) {
        let live = self.live_summary.borrow().unwrap_or_default();
        *self.state.borrow_mut() = DoctorPageState::Running {
            kind,
            phase: (kind == DoctorJobKind::Scan).then_some(DoctorScanPhase::ReadingTags),
            completed,
            total,
            live,
        };
        self.refresh();
    }

    pub(in crate::ui) fn update_scan_job(
        &self,
        phase: DoctorScanPhase,
        completed: usize,
        total: usize,
    ) {
        let live = self.live_summary.borrow().unwrap_or_default();
        *self.state.borrow_mut() = DoctorPageState::Running {
            kind: DoctorJobKind::Scan,
            phase: Some(phase),
            completed,
            total,
            live,
        };
        self.refresh();
    }

    pub(in crate::ui) fn set_live_summary(&self, summary: DoctorScanSummary) {
        if self.live_summary.borrow().as_ref() == Some(&summary) {
            return;
        }
        *self.live_summary.borrow_mut() = Some(summary);
        // Read the running progress out of the state and drop the borrow before
        // writing it back: holding a `Ref` across a `borrow_mut` is the classic
        // way to turn a signal callback into a panic.
        let running = match &*self.state.borrow() {
            DoctorPageState::Running {
                kind,
                phase,
                completed,
                total,
                ..
            } => Some((*kind, *phase, *completed, *total)),
            _ => None,
        };
        if let Some((kind, phase, completed, total)) = running {
            if let Some(phase) = phase {
                self.update_scan_job(phase, completed, total);
            } else {
                self.update_job(kind, completed, total);
            }
        }
    }

    /// The scan produced a result, and the quiet write for it is starting. The
    /// page stays on the running screen: a summary that says "already applied"
    /// may not appear before the write that makes it true.
    pub(in crate::ui) fn begin_quiet_write(&self, scan: DoctorScan, total: usize) {
        *self.last_scan.borrow_mut() = Some(scan);
        self.begin_job(DoctorJobKind::Apply, total);
    }

    pub(in crate::ui) fn complete_auto_apply(&self, report: Option<DoctorWriteReport>) {
        let changes = report
            .as_ref()
            .map(applied_change_count)
            .unwrap_or_default();
        self.undo_available.set(changes > 0);
        self.settle(QuietOutcome::Applied(report));
    }

    pub(in crate::ui) fn fail_auto_apply(&self) {
        self.undo_available.set(false);
        self.settle(QuietOutcome::Failed);
    }

    /// A finished revert took the quiet fixes back off disk, so the applied
    /// block has nothing left to report and `Undo` nothing left to undo.
    pub(in crate::ui) fn mark_reverted(&self) {
        self.undo_available.set(false);
        self.settle(QuietOutcome::Reverted);
    }

    /// A job ended without producing a new result — cancelled, failed to start,
    /// or a revert that is already accounted for. Fall back to whatever the
    /// page legitimately knows.
    pub(in crate::ui) fn end_job(&self) {
        let quiet = self.quiet.borrow().clone();
        self.settle(quiet);
    }

    fn settle(&self, quiet: QuietOutcome) {
        *self.quiet.borrow_mut() = quiet.clone();
        let scan = self.last_scan.borrow().clone();
        *self.state.borrow_mut() = match scan {
            Some(scan) => DoctorPageState::Summary {
                scan: Box::new(scan),
                quiet,
            },
            None => DoctorPageState::Start,
        };
        self.refresh();
    }

    pub(in crate::ui) fn set_controls_locked(&self, locked: bool) {
        self.summary.set_controls_locked(locked);
        self.start.set_running(locked);
    }

    pub(in crate::ui) fn quiet_change_count(&self) -> usize {
        match &*self.quiet.borrow() {
            QuietOutcome::Applied(Some(report)) => applied_change_count(report),
            _ => 0,
        }
    }

    pub(in crate::ui) fn show_post_apply(
        &self,
        report: &DoctorWriteReport,
        albums: usize,
        conflicts: usize,
    ) {
        self.results
            .show_post_apply(report, albums, self.quiet_change_count(), conflicts);
        *self.state.borrow_mut() = DoctorPageState::PostApply;
        self.refresh();
    }

    pub(in crate::ui) fn refresh(&self) {
        let state = self.state.borrow().clone();
        match state {
            DoctorPageState::Start => self.stack.set_visible_child_name("start"),
            DoctorPageState::PostApply => self.stack.set_visible_child_name("result"),
            DoctorPageState::Running {
                kind,
                phase,
                completed,
                total,
                live,
            } => {
                if let Some(phase) = phase {
                    self.running.render_scan(
                        phase,
                        completed,
                        total,
                        LiveCounters::from_summary(&live),
                    );
                } else {
                    self.running
                        .render(kind, completed, total, LiveCounters::from_summary(&live));
                }
                self.running.set_cancellable(true);
                self.stack.set_visible_child_name("running");
            }
            DoctorPageState::Summary { scan, quiet } => {
                let blocks = SummaryBlocks::from_scan(&scan, self.remote_active(), &quiet);
                if blocks.is_empty() {
                    self.results.show_nothing(
                        blocks.checked_tracks,
                        blocks.skipped_tracks,
                        &blocks.facts.label(),
                    );
                    self.stack.set_visible_child_name("result");
                } else {
                    self.summary.render(&blocks, self.undo_available.get());
                    self.stack.set_visible_child_name("summary");
                }
            }
        }
    }
}

#[cfg(test)]
impl LibraryDoctorPage {
    /// Which screen is showing, by the stack's own page name.
    pub(super) fn visible_screen(&self) -> Option<String> {
        self.stack.visible_child_name().map(|name| name.to_string())
    }

    /// The box that holds the result cards, for measuring what got rendered.
    pub(super) fn result_cards(&self) -> &gtk4::Box {
        &self.summary.blocks
    }

    pub(super) fn undo_button(&self) -> &gtk4::Button {
        &self.summary.applied_undo
    }

    pub(super) fn review_button(&self) -> &gtk4::Button {
        &self.summary.review
    }

    pub(super) fn scan_again_button(&self) -> &gtk4::Button {
        &self.summary.scan_again
    }
}

fn applied_change_count(report: &DoctorWriteReport) -> usize {
    report
        .rows
        .iter()
        .filter(|row| row.state == DoctorWriteRowState::Applied)
        .count()
}

fn remove_all(container: &gtk4::Box) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }
}
