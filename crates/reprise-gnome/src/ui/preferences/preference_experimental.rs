//! The Experimental preferences page (docs/ux-rules Section AB): the master
//! "Experimental features" switch that gates all instrumental UI (INST-11), and
//! the model-download placeholder shown behind it (INST-12; the real first-use
//! download is P3b).

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use rusqlite::Connection;

use crate::ui::instrumental;
use crate::ui::strings;

/// Builds the Experimental page. The switch persists the
/// `experimental_features.enabled` key; the model group's visibility follows it
/// live (mirroring the Song Visuals gate). Toggling takes effect for already-
/// running surfaces on the next app start (the worker host reads the switch at
/// launch) — an accepted experimental rough edge; the settings key itself is
/// authoritative immediately.
pub(in crate::ui) fn build_page(conn: &Rc<RefCell<Connection>>) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::builder()
        .title(strings::text(strings::EXPERIMENTAL_PAGE_TITLE))
        .build();

    // INST-12: the model-download placeholder — visible only while the switch
    // is on. Disabled until P3b wires the real first-use download.
    let model_group = adw::PreferencesGroup::builder()
        .title(strings::text(strings::MODEL_GROUP_TITLE))
        .build();
    let model_row = adw::ActionRow::builder()
        .title(strings::text(strings::MODEL_DOWNLOAD_TITLE))
        .subtitle(strings::text(strings::MODEL_DOWNLOAD_SUBTITLE))
        .build();
    let download = gtk4::Button::with_label(&strings::text(strings::MODEL_DOWNLOAD_BUTTON));
    download.set_valign(gtk4::Align::Center);
    download.add_css_class("flat");
    download.set_sensitive(false);
    model_row.add_suffix(&download);
    model_group.add(&model_row);

    let enabled = instrumental::experimental_enabled(&conn.borrow());
    model_group.set_visible(enabled);

    // INST-11: the master switch.
    let switch_group = adw::PreferencesGroup::builder()
        .title(strings::text(strings::EXPERIMENTAL_GROUP_TITLE))
        .description(strings::text(strings::EXPERIMENTAL_GROUP_DESCRIPTION))
        .build();
    let toggle = adw::SwitchRow::builder()
        .title(strings::text(strings::EXPERIMENTAL_TOGGLE_TITLE))
        .subtitle(strings::text(strings::EXPERIMENTAL_TOGGLE_SUBTITLE))
        .active(enabled)
        .build();
    {
        let conn = conn.clone();
        let model_group = model_group.clone();
        toggle.connect_active_notify(move |row| {
            let active = row.is_active();
            if let Err(error) = instrumental::set_experimental_enabled(&conn.borrow(), active) {
                tracing::warn!(%error, "could not save the experimental-features switch");
            }
            model_group.set_visible(active);
        });
    }
    switch_group.add(&toggle);

    page.add(&switch_group);
    page.add(&model_group);
    page
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;

    // UX INST-11: the master switch persists, defaults off, and reads back — the
    // gate every instrumental surface consults.
    #[test]
    fn inst_11_experimental_switch_persists_and_defaults_off() {
        let conn = Connection::open_in_memory().unwrap();
        reprise_core::db::migrate(&conn).unwrap();
        assert!(
            !crate::ui::instrumental::experimental_enabled(&conn),
            "experimental features are off by default"
        );
        crate::ui::instrumental::set_experimental_enabled(&conn, true).unwrap();
        assert!(
            crate::ui::instrumental::experimental_enabled(&conn),
            "the switch reads back as on after persisting"
        );
        crate::ui::instrumental::set_experimental_enabled(&conn, false).unwrap();
        assert!(!crate::ui::instrumental::experimental_enabled(&conn));
    }
}
