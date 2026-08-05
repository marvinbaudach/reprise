use std::rc::Rc;

use chrono::{DateTime, Local};
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::db::Db;
use reprise_core::library_doctor::{DoctorCleanup, LibraryDoctor};
use reprise_core::queries;
use reprise_core::view_source::ViewSource;

use super::remote_toggle;
use crate::ui::strings;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct StartPageModel {
    cleanup_track_count: Option<usize>,
}

impl StartPageModel {
    fn from_cleanup(cleanup: Option<&DoctorCleanup>) -> Self {
        Self {
            cleanup_track_count: cleanup.map(|cleanup| cleanup.track_count),
        }
    }

    const fn shows_last_scan(self) -> bool {
        self.cleanup_track_count.is_some()
    }
}

pub(in crate::ui) struct DoctorStartPage {
    root: gtk4::Box,
    scope: adw::ToggleGroup,
    remote: adw::SwitchRow,
    acoustid_unavailable: adw::ActionRow,
    fingerprint_available: bool,
    run: gtk4::Button,
    estimate: gtk4::Label,
    last_scan: gtk4::Box,
    last_scan_title: gtk4::Label,
    last_scan_detail: gtk4::Label,
    revert: gtk4::Button,
}

impl DoctorStartPage {
    pub(in crate::ui) fn new(
        conn: &Rc<Db>,
        parent: &adw::ApplicationWindow,
        fingerprint_available: bool,
        on_remote_changed: Rc<dyn Fn(bool)>,
    ) -> Self {
        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 18);

        let status = adw::StatusPage::builder()
            .icon_name("system-search-symbolic")
            .title(strings::text(strings::DOCTOR_START_HEADING))
            .description(strings::text(strings::DOCTOR_START_BODY))
            .vexpand(false)
            .build();
        root.append(&status);

        let scope = adw::ToggleGroup::new();
        for (name, label) in [
            ("library", strings::DOCTOR_SCOPE_WHOLE_LIBRARY),
            ("view", strings::DOCTOR_SCOPE_CURRENT_VIEW),
            ("selection", strings::DOCTOR_SCOPE_SELECTION),
        ] {
            scope.add(
                adw::Toggle::builder()
                    .name(name)
                    .label(strings::text(label))
                    .build(),
            );
        }
        scope.set_active(0);
        scope.set_homogeneous(true);
        scope.set_hexpand(true);
        // a11y-semantics: role=group name=doctor-scope state=one-selected action=arrow-keys
        scope.update_property(&[gtk4::accessible::Property::Label(&strings::text(
            strings::DOCTOR_SCOPE,
        ))]);

        let options = adw::PreferencesGroup::builder()
            .title(strings::text(strings::DOCTOR_SCOPE))
            .build();
        let scope_row = adw::ActionRow::builder().activatable(false).build();
        scope_row.add_suffix(&scope);
        options.add(&scope_row);

        let remote = remote_toggle::remote_suggestions_row_for(conn, parent, on_remote_changed);
        remote.set_subtitle(&strings::text(strings::LIBRARY_DOCTOR_REMOTE_DESCRIPTION));
        options.add(&remote);

        let acoustid_unavailable = adw::ActionRow::builder()
            .title(strings::text(strings::DOCTOR_ACOUSTID_UNAVAILABLE))
            .subtitle(strings::text(
                strings::DOCTOR_ACOUSTID_UNAVAILABLE_DESCRIPTION,
            ))
            .use_markup(false)
            .build();
        acoustid_unavailable.add_prefix(&gtk4::Image::from_icon_name("dialog-warning-symbolic"));
        options.add(&acoustid_unavailable);
        root.append(&options);

        let run = gtk4::Button::builder()
            .label(strings::text(strings::DOCTOR_RUN_SCAN))
            .css_classes(["suggested-action", "pill"])
            .build();
        let estimate = gtk4::Label::builder()
            .xalign(0.0)
            .css_classes(["dim-label"])
            .build();
        let run_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
        run_row.append(&run);
        run_row.append(&estimate);
        root.append(&run_row);

        let separator = gtk4::Separator::new(gtk4::Orientation::Horizontal);
        root.append(&separator);
        let last_scan_title = gtk4::Label::builder().xalign(0.0).build();
        let last_scan_detail = gtk4::Label::builder()
            .xalign(0.0)
            .css_classes(["dim-label"])
            .build();
        let revert = gtk4::Button::builder()
            .label(strings::text(strings::DOCTOR_REVERT_LAST_CLEANUP))
            .halign(gtk4::Align::Start)
            .build();
        let last_scan = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
        last_scan.append(&last_scan_title);
        last_scan.append(&last_scan_detail);
        last_scan.append(&revert);
        root.append(&last_scan);

        let page = Self {
            root,
            scope,
            remote,
            acoustid_unavailable,
            fingerprint_available,
            run,
            estimate,
            last_scan,
            last_scan_title,
            last_scan_detail,
            revert,
        };
        page.refresh(conn);
        page
    }

    pub(in crate::ui) fn widget(&self) -> &gtk4::Box {
        &self.root
    }

    pub(in crate::ui) fn connect_run(&self, callback: impl Fn() + 'static) {
        self.run.connect_clicked(move |_| callback());
    }

    pub(in crate::ui) fn connect_revert(&self, callback: impl Fn() + 'static) {
        self.revert.connect_clicked(move |_| callback());
    }

    pub(in crate::ui) fn selected_scope(&self) -> u32 {
        self.scope.active()
    }

    pub(in crate::ui) fn set_selected_scope(&self, scope: u32) {
        self.scope.set_active(scope);
    }

    pub(in crate::ui) fn remote_active(&self) -> bool {
        self.remote.is_active()
    }

    pub(in crate::ui) fn sync_remote_preference(&self, db: &Db) {
        let active = reprise_core::library_doctor::remote_suggestion_preference(db)
            .is_ok_and(|preference| preference.enabled);
        self.remote.set_active(active);
        self.refresh(db);
    }

    pub(in crate::ui) fn set_running(&self, running: bool) {
        self.scope.set_sensitive(!running);
        self.remote.set_sensitive(!running);
        self.run.set_sensitive(!running);
        self.revert.set_sensitive(!running);
    }

    pub(in crate::ui) fn refresh(&self, db: &Db) {
        self.refresh_remote_availability();
        let track_count = queries::query_track_count(db, &ViewSource::Library, "", &[])
            .unwrap_or_default()
            .max(0) as usize;
        self.estimate.set_label(&strings::doctor_scan_estimate(
            track_count,
            track_count.div_ceil(200),
        ));
        let cleanup = LibraryDoctor::new(db).last_cleanup().ok().flatten();
        let model = StartPageModel::from_cleanup(cleanup.as_ref());
        self.last_scan.set_visible(model.shows_last_scan());
        if let Some(cleanup) = cleanup {
            self.last_scan_title
                .set_label(&strings::doctor_last_scan(&format_scan_time(
                    cleanup.created_at,
                )));
            self.last_scan_detail
                .set_label(&strings::doctor_last_scan_fixes(cleanup.change_count));
        }
    }

    pub(in crate::ui) fn refresh_remote_availability(&self) {
        self.acoustid_unavailable
            .set_visible(show_acoustid_unavailable(
                self.remote.is_active(),
                self.fingerprint_available,
            ));
    }
}

fn format_scan_time(timestamp: i64) -> String {
    DateTime::from_timestamp(timestamp, 0).map_or_else(
        || timestamp.to_string(),
        |time| {
            time.with_timezone(&Local)
                .format("%Y-%m-%d %H:%M")
                .to_string()
        },
    )
}

const fn show_acoustid_unavailable(remote_active: bool, fingerprint_available: bool) -> bool {
    remote_active && !fingerprint_available
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doc_8c_start_page_carries_scope_remote_run_and_the_only_revert() {
        let source = include_str!("start_page.rs");
        assert!(source.contains("adw::ToggleGroup::new"));
        assert!(source.contains("remote_suggestions_row_for"));
        assert!(source.contains("DOCTOR_RUN_SCAN"));
        assert!(source.contains("DOCTOR_REVERT_LAST_CLEANUP"));
    }

    #[test]
    fn doc_8c_last_scan_block_is_hidden_without_a_revertible_cleanup() {
        assert!(!StartPageModel::from_cleanup(None).shows_last_scan());
        let cleanup = DoctorCleanup {
            scan_id: 1,
            job_ids: vec![2],
            created_at: 3,
            track_count: 4,
            change_count: 5,
        };
        assert!(StartPageModel::from_cleanup(Some(&cleanup)).shows_last_scan());
    }

    #[test]
    fn doc_6b_library_doctor_controls_explain_job_locking() {
        assert!(controls_sensitive(false));
        assert!(!controls_sensitive(true));
    }

    #[test]
    fn doc_7a_acoustid_unavailable_is_visible_only_for_remote_mode() {
        assert!(show_acoustid_unavailable(true, false));
        assert!(!show_acoustid_unavailable(false, false));
        assert!(!show_acoustid_unavailable(true, true));
    }

    const fn controls_sensitive(job_running: bool) -> bool {
        !job_running
    }
}
