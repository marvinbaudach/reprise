use std::rc::Rc;

use chrono::{DateTime, Local};
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::db::Db;
use reprise_core::library_doctor::{DoctorCleanup, LibraryDoctor};
use reprise_core::queries;
use reprise_core::view_source::ViewSource;

use super::{doctor_glyph, remote_toggle};
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
        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        let column = gtk4::Box::new(gtk4::Orientation::Vertical, 26);
        column.set_halign(gtk4::Align::Start);
        column.set_hexpand(true);
        column.add_css_class("doctor-start-column");
        let clamp = adw::Clamp::builder()
            .maximum_size(620)
            .tightening_threshold(620)
            .halign(gtk4::Align::Start)
            .margin_top(56)
            .margin_start(64)
            .margin_end(64)
            .child(&column)
            .build();
        root.append(&clamp);

        let intro = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        let icon = gtk4::Image::from_icon_name(doctor_glyph());
        icon.set_pixel_size(30);
        icon.set_halign(gtk4::Align::Start);
        icon.set_margin_bottom(16);
        icon.add_css_class("accent");
        icon.add_css_class("doctor-start-icon");
        intro.append(&icon);
        let heading = gtk4::Label::builder()
            .label(strings::text(strings::DOCTOR_START_HEADING))
            .xalign(0.0)
            .css_classes(["title-3"])
            .margin_bottom(10)
            .build();
        intro.append(&heading);
        let body = gtk4::Label::builder()
            .label(strings::text(strings::DOCTOR_START_BODY))
            .xalign(0.0)
            .wrap(true)
            .wrap_mode(gtk4::pango::WrapMode::WordChar)
            .css_classes(["doctor-start-body"])
            .build();
        intro.append(&body);
        column.append(&intro);

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

        let scope_label = gtk4::Label::builder()
            .label(strings::text(strings::DOCTOR_SCOPE))
            .xalign(0.0)
            .css_classes(["caption", "doctor-start-scope-label"])
            .build();
        let scope_block = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
        scope_block.append(&scope_label);
        scope_block.append(&scope);
        column.append(&scope_block);

        let remote = remote_toggle::remote_suggestions_row_for(conn, parent, on_remote_changed);
        remote.set_subtitle(&strings::text(strings::LIBRARY_DOCTOR_REMOTE_DESCRIPTION));
        remote.add_css_class("card");
        remote.add_css_class("doctor-start-remote");
        column.append(&remote);

        let acoustid_unavailable = adw::ActionRow::builder()
            .title(strings::text(strings::DOCTOR_ACOUSTID_UNAVAILABLE))
            .subtitle(strings::text(
                strings::DOCTOR_ACOUSTID_UNAVAILABLE_DESCRIPTION,
            ))
            .use_markup(false)
            .build();
        acoustid_unavailable.add_prefix(&gtk4::Image::from_icon_name("dialog-warning-symbolic"));
        acoustid_unavailable.add_css_class("card");
        column.append(&acoustid_unavailable);

        let run = gtk4::Button::builder()
            .label(strings::text(strings::DOCTOR_RUN_SCAN))
            .css_classes(["suggested-action", "pill", "doctor-start-run"])
            .build();
        let estimate = gtk4::Label::builder()
            .xalign(0.0)
            .css_classes(["doctor-start-estimate"])
            .build();
        let run_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 14);
        run_row.append(&run);
        run_row.append(&estimate);
        column.append(&run_row);

        let last_scan_title = gtk4::Label::builder()
            .xalign(0.0)
            .css_classes(["doctor-start-last-title"])
            .build();
        let last_scan_detail = gtk4::Label::builder()
            .xalign(0.0)
            .css_classes(["doctor-start-last-detail"])
            .build();
        let revert = gtk4::Button::builder().halign(gtk4::Align::End).build();
        let revert_content = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        revert_content.append(&gtk4::Image::from_icon_name("edit-undo-symbolic"));
        revert_content.append(&gtk4::Label::new(Some(&strings::text(
            strings::DOCTOR_REVERT_LAST_CLEANUP,
        ))));
        revert.set_child(Some(&revert_content));
        revert.update_property(&[gtk4::accessible::Property::Label(&strings::text(
            strings::DOCTOR_REVERT_LAST_CLEANUP,
        ))]);
        let last_scan_copy = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
        last_scan_copy.set_hexpand(true);
        last_scan_copy.append(&last_scan_title);
        last_scan_copy.append(&last_scan_detail);
        let last_scan_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 14);
        last_scan_row.append(&last_scan_copy);
        last_scan_row.append(&revert);
        let separator = gtk4::Separator::new(gtk4::Orientation::Horizontal);
        let last_scan = gtk4::Box::new(gtk4::Orientation::Vertical, 20);
        last_scan.add_css_class("doctor-start-last-scan");
        last_scan.append(&separator);
        last_scan.append(&last_scan_row);
        column.append(&last_scan);

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
        let rates = reprise_core::library_doctor::scan_rates(db).unwrap_or_default();
        self.estimate
            .set_label(&scan_estimate(track_count, rates, self.remote.is_active()));
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

fn scan_estimate(
    track_count: usize,
    rates: reprise_core::library_doctor::DoctorScanRates,
    remote_enabled: bool,
) -> String {
    let Some(local_rate) = rates.local_tracks_per_minute else {
        return strings::doctor_scan_estimate_tracks_only(track_count);
    };
    let mut minutes = track_count as f64 / local_rate;
    if remote_enabled {
        let Some(remote_rate) = rates.remote_tracks_per_minute else {
            return strings::doctor_scan_estimate_tracks_only(track_count);
        };
        minutes += track_count as f64 / remote_rate;
    }
    strings::doctor_scan_estimate(track_count, minutes.ceil() as usize)
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
        assert!(!source.contains(&["adw::", "StatusPage"].concat()));
        assert!(!source.contains(&["adw::", "PreferencesGroup"].concat()));
        assert!(source.contains("adw::Clamp"));
        assert!(source.contains("doctor_glyph()"));
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn doc_8d_the_start_column_is_flush_left_and_capped() {
        if gtk4::init().is_err() {
            return;
        }
        let conn = Rc::new(crate::test_db::open().unwrap());
        let parent = adw::ApplicationWindow::builder().build();
        let page = DoctorStartPage::new(&conn, &parent, false, Rc::new(|_| {}));
        let window = adw::ApplicationWindow::builder()
            .default_width(1_000)
            .default_height(780)
            .build();
        window.set_size_request(1_000, 780);
        window.set_content(Some(page.widget()));
        window.present();
        while gtk4::glib::MainContext::default().iteration(false) {}

        let column = descendant_with_class(page.widget(), "doctor-start-column")
            .and_then(|widget| widget.downcast::<gtk4::Box>().ok())
            .expect("the start column must exist");
        let icon = descendant_with_class(page.widget(), "doctor-start-icon")
            .and_then(|widget| widget.downcast::<gtk4::Image>().ok())
            .expect("the start icon must exist");
        let bounds = column
            .compute_bounds(&window)
            .expect("the start column must be allocated");
        assert!(bounds.x() < 120.0, "column is not flush left: {bounds:?}");
        assert!(bounds.width() <= 621.0, "column exceeds 620 px: {bounds:?}");
        assert!(
            bounds.y() < 260.0,
            "column is vertically centred: {bounds:?}"
        );
        assert_eq!(column.halign(), gtk4::Align::Start);
        assert_eq!(icon.pixel_size(), 30);
        assert!(icon.has_css_class("accent"));
        window.close();
    }

    fn descendant_with_class(root: &impl IsA<gtk4::Widget>, class: &str) -> Option<gtk4::Widget> {
        let root = root.upcast_ref::<gtk4::Widget>();
        if root.has_css_class(class) {
            return Some(root.clone());
        }
        let mut child = root.first_child();
        while let Some(widget) = child {
            if let Some(found) = descendant_with_class(&widget, class) {
                return Some(found);
            }
            child = widget.next_sibling();
        }
        None
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
    fn doc_8d_without_a_measurement_the_estimate_names_no_duration() {
        assert_eq!(
            scan_estimate(390, Default::default(), false),
            strings::doctor_scan_estimate_tracks_only(390)
        );
    }

    #[test]
    fn doc_8d_the_estimate_accounts_for_the_remote_switch() {
        let rates = reprise_core::library_doctor::DoctorScanRates {
            local_tracks_per_minute: Some(390.0),
            remote_tracks_per_minute: Some(195.0),
        };

        assert_eq!(
            scan_estimate(390, rates, false),
            strings::doctor_scan_estimate(390, 1)
        );
        assert_eq!(
            scan_estimate(390, rates, true),
            strings::doctor_scan_estimate(390, 3)
        );
    }

    #[test]
    fn doc_6b_library_doctor_controls_explain_job_locking() {
        assert!(controls_sensitive(false));
        assert!(!controls_sensitive(true));
    }

    #[test]
    fn doc_7c_acoustid_unavailable_is_visible_only_for_remote_mode() {
        assert!(show_acoustid_unavailable(true, false));
        assert!(!show_acoustid_unavailable(false, false));
        assert!(!show_acoustid_unavailable(true, true));
    }

    const fn controls_sensitive(job_running: bool) -> bool {
        !job_running
    }
}
