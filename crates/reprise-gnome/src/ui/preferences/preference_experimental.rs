//! The Experimental preferences page (docs/ux-rules Section AB): the master
//! "Experimental features" switch (INST-11).
//!
//! The page used to carry the first-use stem-model download flow (INST-12)
//! beside the switch. The instrumental surface was removed from the GTK
//! frontend, so nothing here provisions a model any more; the switch itself
//! stays because it still gates the AI badge (INST-10) and the "Hide AI music"
//! filter (FIL-7), which mark tracks the CLI/MCP frontends can still produce.

use std::rc::Rc;

use libadwaita as adw;
use libadwaita::prelude::*;

use crate::ui::experimental;
use crate::ui::preferences::PreferencesContext;
use crate::ui::strings;

/// Builds the Experimental page. A successful switch write immediately applies
/// to the running window: the sidebar rebuilds and the track list reloads, so
/// the gated AI surface appears or disappears without a restart (INST-11).
pub(in crate::ui) fn build_page(context: &Rc<PreferencesContext>) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::builder()
        .title(strings::text(strings::EXPERIMENTAL_PAGE_TITLE))
        .build();

    let enabled = experimental::experimental_enabled(&context.conn);

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
        let context = Rc::downgrade(context);
        toggle.connect_active_notify(move |row| {
            let active = row.is_active();
            let Some(context) = context.upgrade() else {
                return;
            };
            let persisted = experimental::set_experimental_enabled(&context.conn, active);
            if let Err(error) = persisted {
                tracing::warn!(%error, "could not save the experimental-features switch");
                return;
            }
            context.sidebar.refresh("experimental features toggled");
            context.track_list.reload();
        });
    }
    switch_group.add(&toggle);

    page.add(&switch_group);
    page
}

#[cfg(test)]
mod tests {

    // UX INST-11: the master switch persists, defaults off, and reads back — the
    // gate every AI surface consults.
    #[test]
    fn inst_11_experimental_switch_persists_and_defaults_off() {
        let conn = crate::test_db::open().unwrap();
        assert!(
            !crate::ui::experimental::experimental_enabled(&conn),
            "experimental features are off by default"
        );
        crate::ui::experimental::set_experimental_enabled(&conn, true).unwrap();
        assert!(
            crate::ui::experimental::experimental_enabled(&conn),
            "the switch reads back as on after persisting"
        );
        crate::ui::experimental::set_experimental_enabled(&conn, false).unwrap();
        assert!(!crate::ui::experimental::experimental_enabled(&conn));
    }
}
