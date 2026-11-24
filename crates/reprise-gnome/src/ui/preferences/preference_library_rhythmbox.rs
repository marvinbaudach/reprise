//! The Library page's way into the Rhythmbox import after first run (START-8).

use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;

use super::strings;

/// Builds the row. `available` is whether a `rhythmdb.xml` exists, `scanning`
/// whether a library scan runs right now. Without a database the row stays in
/// place, insensitive, and says why, so the import is never a silent absence.
pub(super) fn build_rhythmbox_import_row(
    available: bool,
    scanning: bool,
    callback: Rc<dyn Fn()>,
) -> adw::ActionRow {
    let row = super::action_row(strings::RHYTHMBOX_PREFERENCES_IMPORT, callback);
    apply_state(&row, available, scanning);
    row
}

/// A missing database outranks a running scan: it is the reason that does not
/// go away when the scan ends.
fn apply_state(row: &adw::ActionRow, available: bool, scanning: bool) {
    let subtitle = match (available, scanning) {
        (false, _) => strings::RHYTHMBOX_PREFERENCES_IMPORT_UNAVAILABLE,
        (true, true) => strings::SCAN_CHIP_SCANNING,
        (true, false) => strings::RHYTHMBOX_PREFERENCES_IMPORT_SUBTITLE,
    };
    row.set_subtitle(&strings::text(subtitle));
    row.set_sensitive(available && !scanning);
}

/// Keeps the row in step with the scan: the rescan row follows the same
/// button, so both go quiet together and come back together.
pub(super) fn follow_scan(row: &adw::ActionRow, scan_button: &gtk4::Button, available: bool) {
    let row = row.downgrade();
    scan_button.connect_sensitive_notify(move |button| {
        if let Some(row) = row.upgrade() {
            apply_state(&row, available, !button.is_sensitive());
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// START-8: with a Rhythmbox database the Library page's row opens the
    /// import. The row is built on a real page and group, then found the way
    /// the settings search finds rows, and activated like a click.
    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn start_8_the_library_page_offers_the_import_when_a_rhythmbox_database_exists() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let opened = Rc::new(std::cell::Cell::new(0_u32));
        let counter = opened.clone();
        let row = build_rhythmbox_import_row(
            true,
            false,
            Rc::new(move || counter.set(counter.get() + 1)),
        );
        let page = adw::PreferencesPage::new();
        let group = adw::PreferencesGroup::new();
        group.add(&row);
        page.add(&group);

        let mut rows = Vec::new();
        super::super::preferences_search_index::collect_rows(
            page.upcast_ref(),
            super::super::preferences_window::PageId::Library,
            &mut rows,
        );

        let title = strings::text(strings::RHYTHMBOX_PREFERENCES_IMPORT);
        let found = rows
            .iter()
            .find(|indexed| indexed.row.title() == title)
            .expect("the import row is on the page");
        assert!(found.row.is_sensitive());
        row.emit_by_name::<()>("activated", &[]);
        assert_eq!(opened.get(), 1, "activating the row opens the import");
    }

    /// START-8: without a database there is nothing to import, so the row
    /// says so instead of opening an empty dialog or vanishing.
    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn start_8_without_a_rhythmbox_database_the_row_is_insensitive_and_says_why() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let row = build_rhythmbox_import_row(false, false, Rc::new(|| {}));

        assert!(!row.is_sensitive());
        assert_eq!(
            row.subtitle().as_deref(),
            Some(strings::text(strings::RHYTHMBOX_PREFERENCES_IMPORT_UNAVAILABLE).as_str())
        );
        assert_eq!(
            row.title().as_str(),
            strings::text(strings::RHYTHMBOX_PREFERENCES_IMPORT)
        );
    }

    /// START-8: a running scan and an import would both rewrite the same
    /// rows, so the row waits for the scan like the rescan row does, says why,
    /// and returns when the scan ends. Without a database it never returns.
    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn start_8_the_import_waits_for_a_running_scan_and_returns_after_it() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        for available in [true, false] {
            let scan_button = gtk4::Button::new();
            let row = build_rhythmbox_import_row(available, false, Rc::new(|| {}));
            follow_scan(&row, &scan_button, available);

            scan_button.set_sensitive(false);
            assert!(!row.is_sensitive(), "available: {available}");
            if available {
                assert_eq!(
                    row.subtitle().as_deref(),
                    Some(strings::text(strings::SCAN_CHIP_SCANNING).as_str())
                );
            }

            scan_button.set_sensitive(true);
            assert_eq!(row.is_sensitive(), available, "after the scan");
            let expected = if available {
                strings::RHYTHMBOX_PREFERENCES_IMPORT_SUBTITLE
            } else {
                strings::RHYTHMBOX_PREFERENCES_IMPORT_UNAVAILABLE
            };
            assert_eq!(
                row.subtitle().as_deref(),
                Some(strings::text(expected).as_str())
            );
        }
    }

    /// A scan already running when Preferences opens gates the row from the
    /// first paint, not only after the next change of the scan button.
    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn start_8_a_scan_running_when_preferences_opens_gates_the_row_at_once() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let row = build_rhythmbox_import_row(true, true, Rc::new(|| {}));

        assert!(!row.is_sensitive());
    }
}
