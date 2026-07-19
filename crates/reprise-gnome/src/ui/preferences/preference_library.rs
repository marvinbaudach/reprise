//! Library preferences backed by the existing safe picker, scanner, and import paths.

use std::rc::Rc;

use gtk4::gio::prelude::*;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::library::settings;

use super::strings;
use super::{action_row, PreferencesContext};

fn library_root_text(context: &PreferencesContext) -> String {
    let root = {
        let conn = context.conn.borrow();
        settings::get_library_root(&conn)
    };
    root.ok()
        .flatten()
        .unwrap_or_else(|| strings::text(strings::NO_LIBRARY_FOLDER))
}

impl PreferencesContext {
    pub(in crate::ui) fn refresh_library_folder_rows(&self) {
        let subtitle = library_root_text(self);
        let rows = std::mem::take(&mut *self.library_folder_rows.borrow_mut());
        let mut live_rows = Vec::with_capacity(rows.len());
        for weak in rows {
            if let Some(row) = weak.upgrade() {
                row.set_subtitle(&subtitle);
                live_rows.push(weak);
            }
        }
        *self.library_folder_rows.borrow_mut() = live_rows;
    }

    pub(in crate::ui) fn library_page(self: &Rc<Self>) -> adw::PreferencesPage {
        let page = adw::PreferencesPage::builder()
            .title(strings::text(strings::PREFERENCES_LIBRARY))
            .icon_name("folder-music-symbolic")
            .build();
        let group = adw::PreferencesGroup::new();
        let folder = adw::ActionRow::builder()
            .title(strings::text(strings::LIBRARY_FOLDER))
            .subtitle(library_root_text(self))
            .build();
        self.library_folder_rows
            .borrow_mut()
            .push(folder.downgrade());
        let choose = gtk4::Button::with_label(&strings::text(strings::CHOOSE_FOLDER));
        choose.set_valign(gtk4::Align::Center);
        let scan_button = self.scan_button.clone();
        choose.connect_clicked(move |_| scan_button.emit_clicked());
        folder.add_suffix(&choose);
        group.add(&folder);

        let weak = Rc::downgrade(self);
        group.add(&action_row(
            strings::CONTEXT_MENU_RESCAN_LIBRARY,
            Rc::new(move || {
                if let Some(context) = weak.upgrade() {
                    context.track_list.rescan_library();
                }
            }),
        ));

        super::preference_rhythmbox::add_rhythmbox_import_row(self, &group);

        page.add(&group);
        page.add(&self.audio_analysis.build_group(&self.preferences_parent()));
        page
    }
}
