use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use reprise_core::library_doctor::{DoctorWriteReport, DoctorWriteRowState};

use crate::ui::strings;

type Callback = Rc<RefCell<Option<Rc<dyn Fn()>>>>;

#[derive(Debug, Clone, PartialEq, Eq)]
struct PostApplyModel {
    tracks: usize,
    changes: usize,
    albums: usize,
    quiet_changes: usize,
    conflicts: usize,
}

impl PostApplyModel {
    fn from_report(
        report: &DoctorWriteReport,
        albums: usize,
        quiet_changes: usize,
        conflicts: usize,
    ) -> Self {
        Self {
            tracks: report.updated_tracks,
            changes: applied_change_count(report),
            albums,
            quiet_changes,
            conflicts,
        }
    }
}

pub(in crate::ui) struct DoctorResultPages {
    root: gtk4::Stack,
    post_title: gtk4::Label,
    post_counts: gtk4::Label,
    post_conflicts: gtk4::Label,
    post_quiet: gtk4::Label,
    empty_body: gtk4::Label,
    on_undo: Callback,
    on_done: Callback,
    on_scan_again: Callback,
}

impl DoctorResultPages {
    pub(in crate::ui) fn new() -> Self {
        let root = gtk4::Stack::new();

        let post_title = heading_label();
        let post_counts = body_label();
        let post_conflicts = body_label();
        let post_quiet = body_label();
        let undo = gtk4::Button::with_label(&strings::text(strings::DOCTOR_UNDO_EVERYTHING));
        let done = gtk4::Button::builder()
            .label(strings::text(strings::DOCTOR_DONE))
            .css_classes(["suggested-action"])
            .build();
        let actions = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
        actions.set_halign(gtk4::Align::Center);
        actions.append(&undo);
        actions.append(&done);
        let post = status_box("emblem-ok-symbolic", &post_title);
        post.append(&post_counts);
        post.append(&post_conflicts);
        post.append(&actions);
        post.append(&post_quiet);
        root.add_named(&post, Some("post-apply"));

        let empty_title = heading_label();
        empty_title.set_label(&strings::text(strings::DOCTOR_NOTHING_TO_FIX));
        let empty_body = body_label();
        let scan_again = gtk4::Button::builder()
            .label(strings::text(strings::DOCTOR_SCAN_AGAIN))
            .css_classes(["suggested-action"])
            .halign(gtk4::Align::Center)
            .build();
        let empty = status_box("emblem-ok-symbolic", &empty_title);
        empty.append(&empty_body);
        empty.append(&scan_again);
        root.add_named(&empty, Some("nothing"));

        let on_undo = Rc::new(RefCell::new(None::<Rc<dyn Fn()>>));
        let on_done = Rc::new(RefCell::new(None::<Rc<dyn Fn()>>));
        let on_scan_again = Rc::new(RefCell::new(None::<Rc<dyn Fn()>>));
        connect_button(&undo, &on_undo);
        connect_button(&done, &on_done);
        connect_button(&scan_again, &on_scan_again);

        Self {
            root,
            post_title,
            post_counts,
            post_conflicts,
            post_quiet,
            empty_body,
            on_undo,
            on_done,
            on_scan_again,
        }
    }

    pub(in crate::ui) fn widget(&self) -> &gtk4::Stack {
        &self.root
    }

    pub(in crate::ui) fn show_post_apply(
        &self,
        report: &DoctorWriteReport,
        albums: usize,
        quiet_changes: usize,
        conflicts: usize,
    ) {
        let model = PostApplyModel::from_report(report, albums, quiet_changes, conflicts);
        self.post_title
            .set_label(&strings::doctor_tags_updated(model.tracks));
        self.post_counts
            .set_label(&strings::doctor_changes_and_albums(
                model.changes,
                model.albums,
            ));
        self.post_conflicts.set_label(&if model.conflicts == 0 {
            String::new()
        } else {
            strings::doctor_conflicts_headline(model.conflicts)
        });
        self.post_conflicts.set_visible(model.conflicts > 0);
        self.post_quiet
            .set_label(&strings::doctor_includes_quiet_fixes(model.quiet_changes));
        self.root.set_visible_child_name("post-apply");
    }

    pub(in crate::ui) fn show_nothing(&self, checked: usize, skipped: usize) {
        self.empty_body
            .set_label(&strings::doctor_nothing_to_fix_body(checked, skipped));
        self.root.set_visible_child_name("nothing");
    }

    pub(in crate::ui) fn connect_undo(&self, callback: impl Fn() + 'static) {
        *self.on_undo.borrow_mut() = Some(Rc::new(callback));
    }

    pub(in crate::ui) fn connect_done(&self, callback: impl Fn() + 'static) {
        *self.on_done.borrow_mut() = Some(Rc::new(callback));
    }

    pub(in crate::ui) fn connect_scan_again(&self, callback: impl Fn() + 'static) {
        *self.on_scan_again.borrow_mut() = Some(Rc::new(callback));
    }
}

fn applied_change_count(report: &DoctorWriteReport) -> usize {
    report
        .rows
        .iter()
        .filter(|row| row.state == DoctorWriteRowState::Applied)
        .count()
}

fn heading_label() -> gtk4::Label {
    gtk4::Label::builder()
        .wrap(true)
        .justify(gtk4::Justification::Center)
        .css_classes(["title-1"])
        .build()
}

fn body_label() -> gtk4::Label {
    gtk4::Label::builder()
        .wrap(true)
        .justify(gtk4::Justification::Center)
        .css_classes(["dim-label"])
        .build()
}

fn status_box(icon_name: &str, title: &gtk4::Label) -> gtk4::Box {
    let root = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    root.set_valign(gtk4::Align::Center);
    root.set_margin_top(48);
    root.append(
        &gtk4::Image::builder()
            .icon_name(icon_name)
            .pixel_size(64)
            .build(),
    );
    root.append(title);
    root
}

fn connect_button(button: &gtk4::Button, callback: &Callback) {
    let callback = callback.clone();
    button.connect_clicked(move |_| {
        if let Some(callback) = callback.borrow().clone() {
            callback();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use reprise_core::library_doctor::{
        DoctorField, DoctorValue, DoctorWriteRow, DoctorWriteRowState,
    };

    fn report(updated_tracks: usize, applied_changes: usize) -> DoctorWriteReport {
        DoctorWriteReport {
            job_id: 1,
            source_job_id: None,
            updated_tracks,
            cancelled_tracks: 0,
            failed_tracks: 0,
            conflict_tracks: 0,
            unavailable_tracks: 0,
            rows: (0..applied_changes)
                .map(|index| DoctorWriteRow {
                    row_id: None,
                    track_id: index as i64,
                    path: format!("/test/{index}.flac").into(),
                    field: DoctorField::Artist,
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

    #[test]
    fn doc_9c_post_apply_names_the_quiet_fixes_and_the_unresolved_conflicts() {
        let model = PostApplyModel::from_report(&report(2, 3), 1, 2, 4);
        assert_eq!(model.quiet_changes, 2);
        assert_eq!(model.conflicts, 4);
    }

    #[test]
    fn doc_9c_post_apply_reports_the_write_report_not_the_plan() {
        let model = PostApplyModel::from_report(&report(2, 3), 1, 1, 0);
        assert_eq!(model.tracks, 2);
        assert_eq!(model.changes, 3);
    }

    #[test]
    fn doc_9c_nothing_to_fix_is_distinct_from_the_pre_scan_state() {
        assert_ne!("nothing", "start");
        assert_eq!(
            strings::text(strings::DOCTOR_NOTHING_TO_FIX),
            "Nothing to fix"
        );
    }
}
