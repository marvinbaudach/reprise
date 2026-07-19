//! Minimal navigation surface for a Library Doctor job without the plugin UI.

use libadwaita as adw;

use super::progress_card::DoctorJobKind;
use crate::ui::strings;

pub(super) struct LibraryDoctorJobPage {
    navigation_page: adw::NavigationPage,
    status: adw::StatusPage,
}

impl LibraryDoctorJobPage {
    pub(super) fn new() -> Self {
        let status = adw::StatusPage::builder()
            .icon_name("system-run-symbolic")
            .title(strings::text(strings::LIBRARY_DOCTOR))
            .description(strings::text(strings::DOCTOR_JOB_PAGE_DESCRIPTION))
            .build();
        let navigation_page = adw::NavigationPage::builder()
            .title(strings::text(strings::LIBRARY_DOCTOR))
            .tag("library-doctor-job")
            .child(&status)
            .build();
        Self {
            navigation_page,
            status,
        }
    }

    pub(super) fn navigation_page(&self) -> &adw::NavigationPage {
        &self.navigation_page
    }

    pub(super) fn set_running(&self, kind: DoctorJobKind) {
        self.status.set_title(&strings::text(match kind {
            DoctorJobKind::Scan => strings::DOCTOR_SCANNING,
            DoctorJobKind::Apply => strings::DOCTOR_UPDATING_TAGS,
            DoctorJobKind::Revert => strings::DOCTOR_REVERTING_TAGS,
        }));
        self.status
            .set_description(Some(&strings::text(strings::DOCTOR_JOB_PAGE_DESCRIPTION)));
    }

    pub(super) fn set_result(&self, updated: usize, remaining: usize, reverted: bool) {
        self.status.set_title(&if reverted {
            strings::doctor_tags_reverted(updated)
        } else {
            strings::doctor_tags_updated(updated)
        });
        self.status
            .set_description(Some(&strings::doctor_cleanup_summary(updated, remaining)));
    }

    pub(super) fn set_error(&self, error: &str) {
        self.status
            .set_title(&strings::text(strings::DOCTOR_JOB_FAILED));
        self.status.set_description(Some(error));
    }
}
