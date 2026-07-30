use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::db::Db;
use reprise_core::library_doctor::{
    scan_summary, DoctorProblemCount, DoctorScan, DoctorScanSummary, DoctorWriteReport,
    ProblemClass,
};

use crate::ui::preferences::preference_library_doctor;
use crate::ui::strings;

const PROBLEM_CLASSES: [ProblemClass; 5] = [
    ProblemClass::CasingWhitespace,
    ProblemClass::MissingAlbumArtist,
    ProblemClass::GenreVariant,
    ProblemClass::MissingWrongYear,
    ProblemClass::MissingRecordingMbid,
];

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProblemRowModel {
    class: ProblemClass,
    safe: usize,
    review: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SummaryModel {
    safe_changes: usize,
    review_changes: usize,
    unresolved_groups: usize,
    checked_tracks: usize,
    skipped_tracks: usize,
    problem_rows: Vec<ProblemRowModel>,
}

impl SummaryModel {
    fn from_scan(scan: &DoctorScan, remote_visible: bool) -> Self {
        let summary = scan_summary(scan, remote_visible);
        Self {
            safe_changes: summary.safe_changes,
            review_changes: summary.review_changes,
            unresolved_groups: summary.unresolved_groups,
            checked_tracks: summary.checked_tracks,
            skipped_tracks: summary.skipped_tracks,
            problem_rows: PROBLEM_CLASSES
                .into_iter()
                .map(|class| {
                    let DoctorProblemCount { safe, review } = summary.counts_for(class);
                    ProblemRowModel {
                        class,
                        safe,
                        review,
                    }
                })
                .collect(),
        }
    }

    fn from_summary(summary: DoctorScanSummary) -> Self {
        Self {
            safe_changes: summary.safe_changes,
            review_changes: summary.review_changes,
            unresolved_groups: summary.unresolved_groups,
            checked_tracks: summary.checked_tracks,
            skipped_tracks: summary.skipped_tracks,
            problem_rows: PROBLEM_CLASSES
                .into_iter()
                .map(|class| {
                    let DoctorProblemCount { safe, review } = summary.counts_for(class);
                    ProblemRowModel {
                        class,
                        safe,
                        review,
                    }
                })
                .collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SummaryDisplay {
    model: SummaryModel,
    partial: bool,
}

impl SummaryDisplay {
    fn review_available(&self) -> bool {
        !self.partial
            && self.model.safe_changes + self.model.review_changes + self.model.unresolved_groups
                > 0
    }
}

fn summary_display(
    partial: Option<DoctorScanSummary>,
    complete: Option<&DoctorScan>,
    remote_visible: bool,
) -> Option<SummaryDisplay> {
    if let Some(summary) = partial {
        return Some(SummaryDisplay {
            model: SummaryModel::from_summary(summary),
            partial: true,
        });
    }
    complete.map(|scan| SummaryDisplay {
        model: SummaryModel::from_scan(scan, remote_visible),
        partial: false,
    })
}

pub(in crate::ui) struct LibraryDoctorPage {
    navigation_page: adw::NavigationPage,
    scope: adw::ComboRow,
    remote: adw::SwitchRow,
    acoustid_unavailable: adw::ActionRow,
    fingerprint_available: bool,
    run: gtk4::Button,
    review_all: gtk4::Button,
    review_safe: gtk4::Button,
    review_actions: gtk4::Box,
    summary: adw::PreferencesGroup,
    empty: adw::StatusPage,
    results: gtk4::Box,
    safe_row: adw::ActionRow,
    review_row: adw::ActionRow,
    unresolved_row: adw::ActionRow,
    checked_row: adw::ActionRow,
    write_row: adw::ActionRow,
    problem_rows: Vec<(ProblemClass, adw::ActionRow)>,
    current_scan: RefCell<Option<DoctorScan>>,
    partial_summary: RefCell<Option<DoctorScanSummary>>,
}

impl LibraryDoctorPage {
    pub(in crate::ui) fn new(
        conn: &Rc<Db>,
        parent: &adw::ApplicationWindow,
        fingerprint_available: bool,
        on_remote_changed: Rc<dyn Fn(bool)>,
    ) -> Rc<Self> {
        let scope_model = gtk4::StringList::new(&[
            &strings::text(strings::DOCTOR_SCOPE_WHOLE_LIBRARY),
            &strings::text(strings::DOCTOR_SCOPE_CURRENT_VIEW),
            &strings::text(strings::DOCTOR_SCOPE_SELECTION),
        ]);
        let scope = adw::ComboRow::builder()
            .title(strings::text(strings::DOCTOR_SCOPE))
            .model(&scope_model)
            .selected(0)
            .build();
        let remote =
            preference_library_doctor::remote_suggestions_row_for(conn, parent, on_remote_changed);
        let options = adw::PreferencesGroup::builder()
            .title(strings::text(strings::DOCTOR_SCAN_OPTIONS))
            .build();
        options.add(&scope);
        options.add(&remote);
        let warning_icon = gtk4::Image::from_icon_name("dialog-warning-symbolic");
        let acoustid_unavailable = adw::ActionRow::builder()
            .title(strings::text(strings::DOCTOR_ACOUSTID_UNAVAILABLE))
            .subtitle(strings::text(
                strings::DOCTOR_ACOUSTID_UNAVAILABLE_DESCRIPTION,
            ))
            .use_markup(false)
            .build();
        acoustid_unavailable.add_prefix(&warning_icon);
        acoustid_unavailable.set_visible(show_acoustid_unavailable(
            remote.is_active(),
            fingerprint_available,
        ));
        options.add(&acoustid_unavailable);

        let run = gtk4::Button::builder()
            .label(strings::text(strings::DOCTOR_RUN_SCAN))
            .css_classes(["suggested-action", "pill"])
            .halign(gtk4::Align::Start)
            .build();
        let safe_row = summary_row(strings::DOCTOR_SAFE_FIXES);
        let review_row = summary_row(strings::DOCTOR_SUGGESTIONS);
        let unresolved_row = summary_row(strings::DOCTOR_UNRESOLVED_GROUPS);
        let checked_row = summary_row(strings::DOCTOR_TRACKS_CHECKED);
        let write_row = summary_row(strings::DOCTOR_CLEANUP_STATUS);
        write_row.set_visible(false);
        let problem_rows = PROBLEM_CLASSES
            .into_iter()
            .map(|class| (class, summary_row(problem_title(class))))
            .collect::<Vec<_>>();
        let summary = adw::PreferencesGroup::builder()
            .title(strings::text(strings::DOCTOR_RESULTS))
            .build();
        summary.add(&safe_row);
        summary.add(&review_row);
        summary.add(&unresolved_row);
        summary.add(&checked_row);
        summary.add(&write_row);
        for (_, row) in &problem_rows {
            summary.add(row);
        }

        let review_all = gtk4::Button::builder()
            .label(strings::text(strings::DOCTOR_REVIEW_CHANGES))
            .css_classes(["suggested-action"])
            .build();
        let review_safe = gtk4::Button::builder()
            .label(strings::text(strings::DOCTOR_REVIEW_SAFE))
            .build();
        let review_actions = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
        review_actions.append(&review_all);
        review_actions.append(&review_safe);
        // DOC-13 wires these actions to the review surface. Keep them out of
        // the tab order until that navigation is available.
        review_actions.set_visible(false);

        let results = gtk4::Box::new(gtk4::Orientation::Vertical, 18);
        results.append(&summary);
        results.append(&review_actions);
        results.set_visible(false);

        let empty = adw::StatusPage::builder()
            .icon_name("system-search-symbolic")
            .title(strings::text(strings::DOCTOR_NO_RESULTS))
            .description(strings::text(strings::DOCTOR_NO_RESULTS_DESCRIPTION))
            .vexpand(false)
            .build();

        let content = gtk4::Box::new(gtk4::Orientation::Vertical, 24);
        content.set_margin_top(24);
        content.set_margin_bottom(36);
        content.set_margin_start(24);
        content.set_margin_end(24);
        content.append(&options);
        content.append(&run);
        content.append(&empty);
        content.append(&results);
        let clamp = adw::Clamp::builder()
            .maximum_size(760)
            .tightening_threshold(560)
            .child(&content)
            .build();
        let scrolled = gtk4::ScrolledWindow::builder()
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .child(&clamp)
            .build();
        let header = adw::HeaderBar::new();
        let toolbar = adw::ToolbarView::new();
        toolbar.add_top_bar(&header);
        toolbar.set_content(Some(&scrolled));
        let navigation_page = adw::NavigationPage::builder()
            .title(strings::text(strings::LIBRARY_DOCTOR))
            .tag("library-doctor")
            .child(&toolbar)
            .build();
        Rc::new(Self {
            navigation_page,
            scope,
            remote,
            acoustid_unavailable,
            fingerprint_available,
            run,
            review_all,
            review_safe,
            review_actions,
            summary,
            empty,
            results,
            safe_row,
            review_row,
            unresolved_row,
            checked_row,
            write_row,
            problem_rows,
            current_scan: RefCell::new(None),
            partial_summary: RefCell::new(None),
        })
    }

    pub(in crate::ui) fn navigation_page(&self) -> &adw::NavigationPage {
        &self.navigation_page
    }

    pub(in crate::ui) fn connect_run(&self, callback: impl Fn() + 'static) {
        self.run.connect_clicked(move |_| callback());
    }

    pub(in crate::ui) fn connect_review_all(&self, callback: impl Fn() + 'static) {
        self.review_all.connect_clicked(move |_| callback());
    }

    pub(in crate::ui) fn connect_review_safe(&self, callback: impl Fn() + 'static) {
        self.review_safe.connect_clicked(move |_| callback());
    }

    pub(in crate::ui) fn scan(&self) -> Option<DoctorScan> {
        self.current_scan.borrow().clone()
    }

    pub(in crate::ui) fn set_write_report(&self, report: &DoctorWriteReport, reverted: bool) {
        let remaining = report.cancelled_tracks
            + report.failed_tracks
            + report.conflict_tracks
            + report.unavailable_tracks;
        self.write_row.set_title(&strings::text(if reverted {
            strings::DOCTOR_REVERT_STATUS
        } else {
            strings::DOCTOR_CLEANUP_STATUS
        }));
        self.write_row
            .set_subtitle(&strings::doctor_cleanup_summary(
                report.updated_tracks,
                remaining,
            ));
        self.write_row.set_visible(true);
    }

    pub(in crate::ui) fn selected_scope(&self) -> u32 {
        self.scope.selected()
    }

    pub(in crate::ui) fn set_selected_scope(&self, scope: u32) {
        self.scope.set_selected(scope);
    }

    pub(in crate::ui) fn remote_active(&self) -> bool {
        self.remote.is_active()
    }

    pub(in crate::ui) fn sync_remote_preference(&self, db: &Db) {
        let active = reprise_core::library_doctor::remote_suggestion_preference(db)
            .is_ok_and(|preference| preference.enabled);
        self.remote.set_active(active);
        self.refresh();
    }

    pub(in crate::ui) fn set_running(&self, running: bool) {
        self.scope.set_sensitive(!running);
        self.remote.set_sensitive(!running);
        self.run.set_sensitive(!running);
    }

    pub(in crate::ui) fn set_scan(&self, scan: Option<DoctorScan>) {
        self.write_row.set_visible(false);
        self.partial_summary.borrow_mut().take();
        *self.current_scan.borrow_mut() = scan;
        self.refresh();
    }

    pub(in crate::ui) fn begin_partial_scan(&self) {
        self.partial_summary
            .borrow_mut()
            .replace(DoctorScanSummary::default());
        self.refresh();
    }

    pub(in crate::ui) fn set_partial_summary(&self, summary: DoctorScanSummary) {
        if self.partial_summary.borrow().as_ref() == Some(&summary) {
            return;
        }
        self.partial_summary.borrow_mut().replace(summary);
        self.refresh();
    }

    pub(in crate::ui) fn clear_partial_scan(&self) {
        self.partial_summary.borrow_mut().take();
        self.refresh();
    }

    pub(in crate::ui) fn refresh(&self) {
        self.acoustid_unavailable
            .set_visible(show_acoustid_unavailable(
                self.remote.is_active(),
                self.fingerprint_available,
            ));
        let partial = *self.partial_summary.borrow();
        let complete = self.current_scan.borrow().clone();
        let display = summary_display(partial, complete.as_ref(), self.remote.is_active());
        let Some(display) = display else {
            self.empty.set_visible(true);
            self.results.set_visible(false);
            return;
        };
        let review_available = display.review_available();
        let SummaryDisplay { model, partial } = display;
        self.summary.set_title(&strings::text(if partial {
            strings::DOCTOR_RESULTS_SO_FAR
        } else {
            strings::DOCTOR_RESULTS
        }));
        self.safe_row
            .set_subtitle(&strings::doctor_change_count(model.safe_changes));
        self.review_row
            .set_subtitle(&strings::doctor_change_count(model.review_changes));
        self.unresolved_row
            .set_subtitle(&strings::doctor_group_count(model.unresolved_groups));
        self.checked_row
            .set_subtitle(&strings::doctor_checked_counts(
                model.checked_tracks,
                model.skipped_tracks,
            ));
        for (class, row) in &self.problem_rows {
            let counts = model
                .problem_rows
                .iter()
                .find(|item| item.class == *class)
                .expect("every fixed problem class must be projected");
            row.set_subtitle(&strings::doctor_problem_counts(counts.safe, counts.review));
            row.set_visible(problem_class_visible(*class, self.remote.is_active()));
        }
        self.review_all.set_label(&strings::doctor_review_changes(
            model.safe_changes + model.review_changes,
        ));
        self.review_safe
            .set_label(&strings::doctor_review_safe_fixes(model.safe_changes));
        self.review_all
            .set_sensitive(model.safe_changes + model.review_changes + model.unresolved_groups > 0);
        self.review_safe.set_sensitive(model.safe_changes > 0);
        self.review_actions.set_visible(review_available);
        self.empty.set_visible(false);
        self.results.set_visible(true);
    }
}

const fn show_acoustid_unavailable(remote_active: bool, fingerprint_available: bool) -> bool {
    remote_active && !fingerprint_available
}

fn summary_row(title: &'static str) -> adw::ActionRow {
    adw::ActionRow::builder()
        .title(strings::text(title))
        .use_markup(false)
        .build()
}

const fn problem_title(class: ProblemClass) -> &'static str {
    match class {
        ProblemClass::CasingWhitespace => strings::DOCTOR_CASING_WHITESPACE,
        ProblemClass::MissingAlbumArtist => strings::DOCTOR_MISSING_ALBUM_ARTIST,
        ProblemClass::GenreVariant => strings::DOCTOR_GENRE_VARIANTS,
        ProblemClass::MissingWrongYear => strings::DOCTOR_MISSING_WRONG_YEAR,
        ProblemClass::MissingRecordingMbid => strings::DOCTOR_MISSING_RECORDING_MBID,
    }
}

const fn problem_class_visible(class: ProblemClass, remote_visible: bool) -> bool {
    remote_visible
        || !matches!(
            class,
            ProblemClass::MissingWrongYear | ProblemClass::MissingRecordingMbid
        )
}

#[cfg(test)]
mod tests {
    use super::{summary_display, SummaryModel};
    use reprise_core::library_doctor::*;

    fn proposal(source: ProposalSource, class: ProblemClass) -> DoctorProposal {
        DoctorProposal {
            track_id: 1,
            field: DoctorField::Artist,
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

    #[test]
    fn doc_2b_root_model_keeps_safe_review_and_track_counts_separate() {
        let scan = DoctorScan {
            id: 1,
            scope_kind: "whole_library".into(),
            created_at: 2,
            options: DoctorScanOptions {
                remote_enabled: true,
            },
            checked_tracks: 8,
            skipped_tracks: 2,
            track_ids: vec![1],
            tracks: Vec::new(),
            proposals: vec![
                proposal(ProposalSource::Local, ProblemClass::CasingWhitespace),
                proposal(
                    ProposalSource::MusicBrainz,
                    ProblemClass::MissingRecordingMbid,
                ),
            ],
            unresolved_groups: Vec::new(),
        };

        let model = SummaryModel::from_scan(&scan, true);

        assert_eq!(model.safe_changes, 1);
        assert_eq!(model.review_changes, 1);
        assert_eq!(model.checked_tracks, 8);
        assert_eq!(model.skipped_tracks, 2);
        assert_eq!(model.problem_rows.len(), 5);
        assert_eq!(model.problem_rows[0].safe, 1);
        assert_eq!(model.problem_rows[4].review, 1);
    }

    #[test]
    fn doc_2c_running_scan_prefers_partial_results_without_enabling_review() {
        let mut partial = DoctorScanSummary::default();
        partial.safe_changes = 3;
        partial.review_changes = 2;
        partial.checked_tracks = 4;

        let display = summary_display(Some(partial), None, true)
            .expect("a running scan must expose its partial result");

        assert!(display.partial);
        assert!(!display.review_available());
        assert_eq!(display.model.safe_changes, 3);
        assert_eq!(display.model.review_changes, 2);
        assert_eq!(display.model.checked_tracks, 4);
    }

    #[test]
    fn doc_7a_acoustid_unavailable_is_visible_only_for_remote_mode() {
        assert!(super::show_acoustid_unavailable(true, false));
        assert!(!super::show_acoustid_unavailable(false, false));
        assert!(!super::show_acoustid_unavailable(true, true));
    }

    #[test]
    fn doc_2b_remote_only_problem_classes_disappear_with_remote_results() {
        assert!(super::problem_class_visible(
            ProblemClass::MissingWrongYear,
            true
        ));
        assert!(!super::problem_class_visible(
            ProblemClass::MissingWrongYear,
            false
        ));
        assert!(!super::problem_class_visible(
            ProblemClass::MissingRecordingMbid,
            false
        ));
        assert!(super::problem_class_visible(
            ProblemClass::GenreVariant,
            false
        ));
    }
}
