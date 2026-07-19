use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::library_doctor::{scan_summary, DoctorProblemCount, DoctorScan, ProblemClass};
use rusqlite::Connection;

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
}

pub(in crate::ui) struct LibraryDoctorPage {
    navigation_page: adw::NavigationPage,
    scope: adw::ComboRow,
    remote: adw::SwitchRow,
    acoustid_unavailable: adw::ActionRow,
    fingerprint_available: bool,
    run: gtk4::Button,
    progress: gtk4::Box,
    progress_bar: gtk4::ProgressBar,
    progress_label: gtk4::Label,
    cancel: gtk4::Button,
    review_all: gtk4::Button,
    review_safe: gtk4::Button,
    empty: adw::StatusPage,
    results: gtk4::Box,
    safe_row: adw::ActionRow,
    review_row: adw::ActionRow,
    unresolved_row: adw::ActionRow,
    checked_row: adw::ActionRow,
    problem_rows: Vec<(ProblemClass, adw::ActionRow)>,
    current_scan: RefCell<Option<DoctorScan>>,
}

impl LibraryDoctorPage {
    pub(in crate::ui) fn new(
        conn: &Rc<RefCell<Connection>>,
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
        let remote = preference_library_doctor::remote_suggestions_row_for(
            conn,
            parent,
            true,
            on_remote_changed,
        );
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
        let progress_bar = gtk4::ProgressBar::builder().hexpand(true).build();
        let progress_label = gtk4::Label::builder()
            .label(strings::text(strings::DOCTOR_SCANNING))
            .xalign(0.0)
            .hexpand(true)
            .build();
        let cancel = gtk4::Button::builder()
            .label(strings::text(strings::CANCEL))
            .build();
        let progress_header = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
        progress_header.append(&progress_label);
        progress_header.append(&cancel);
        let progress = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
        progress.append(&progress_header);
        progress.append(&progress_bar);
        progress.set_visible(false);

        let safe_row = summary_row(strings::DOCTOR_SAFE_FIXES);
        let review_row = summary_row(strings::DOCTOR_SUGGESTIONS);
        let unresolved_row = summary_row(strings::DOCTOR_UNRESOLVED_GROUPS);
        let checked_row = summary_row(strings::DOCTOR_TRACKS_CHECKED);
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
        content.append(&progress);
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
            progress,
            progress_bar,
            progress_label,
            cancel,
            review_all,
            review_safe,
            empty,
            results,
            safe_row,
            review_row,
            unresolved_row,
            checked_row,
            problem_rows,
            current_scan: RefCell::new(None),
        })
    }

    pub(in crate::ui) fn navigation_page(&self) -> &adw::NavigationPage {
        &self.navigation_page
    }

    pub(in crate::ui) fn connect_run(&self, callback: impl Fn() + 'static) {
        self.run.connect_clicked(move |_| callback());
    }

    pub(in crate::ui) fn connect_cancel(&self, callback: impl Fn() + 'static) {
        self.cancel.connect_clicked(move |_| callback());
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

    pub(in crate::ui) fn sync_remote_preference(&self, conn: &Connection) {
        let active = reprise_core::library_doctor::remote_suggestion_preference(conn)
            .is_ok_and(|preference| preference.enabled);
        self.remote.set_active(active);
        self.refresh();
    }

    pub(in crate::ui) fn set_running(&self, running: bool) {
        self.scope.set_sensitive(!running);
        self.remote.set_sensitive(!running);
        self.run.set_sensitive(!running);
        self.progress.set_visible(running);
        if running {
            self.progress_bar.set_fraction(0.0);
            self.progress_label
                .set_text(&strings::text(strings::DOCTOR_SCANNING));
        }
    }

    pub(in crate::ui) fn set_progress(&self, completed: usize, total: usize) {
        let fraction = if total == 0 {
            0.0
        } else {
            completed as f64 / total as f64
        };
        self.progress_bar.set_fraction(fraction.clamp(0.0, 1.0));
        self.progress_label.set_text(&format!(
            "{} {completed}/{total}",
            strings::text(strings::DOCTOR_SCANNING)
        ));
    }

    pub(in crate::ui) fn set_scan(&self, scan: Option<DoctorScan>) {
        *self.current_scan.borrow_mut() = scan;
        self.refresh();
    }

    pub(in crate::ui) fn refresh(&self) {
        self.acoustid_unavailable
            .set_visible(show_acoustid_unavailable(
                self.remote.is_active(),
                self.fingerprint_available,
            ));
        let scan = self.current_scan.borrow().clone();
        let Some(scan) = scan else {
            self.empty.set_visible(true);
            self.results.set_visible(false);
            return;
        };
        let model = SummaryModel::from_scan(&scan, self.remote.is_active());
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
        }
        self.review_all.set_label(&strings::doctor_review_changes(
            model.safe_changes + model.review_changes,
        ));
        self.review_safe
            .set_label(&strings::doctor_review_safe_fixes(model.safe_changes));
        self.review_all
            .set_sensitive(model.safe_changes + model.review_changes + model.unresolved_groups > 0);
        self.review_safe.set_sensitive(model.safe_changes > 0);
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

#[cfg(test)]
mod tests {
    use super::SummaryModel;
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
    fn doc_1d_acoustid_unavailable_is_visible_only_for_remote_mode() {
        assert!(super::show_acoustid_unavailable(true, false));
        assert!(!super::show_acoustid_unavailable(false, false));
        assert!(!super::show_acoustid_unavailable(true, true));
    }
}
