//! One-shot disclosure for artwork consent inherited from retired modules.

use std::cell::Cell;
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use reprise_core::db::Db;
use reprise_core::library::settings;

use super::strings;

fn initial_visibility(db: &Db) -> Result<bool, rusqlite::Error> {
    settings::get_bool(
        db,
        settings::ARTWORK_CONSENT_MERGE_NOTICE_PENDING_KEY,
        false,
    )
}

fn persist_completed(db: &Db, completed: &Cell<bool>) -> bool {
    if completed.get() {
        return true;
    }
    match settings::set_bool(
        db,
        settings::ARTWORK_CONSENT_MERGE_NOTICE_PENDING_KEY,
        false,
    ) {
        Ok(()) => {
            completed.set(true);
            true
        }
        Err(error) => {
            tracing::warn!(%error, "could not persist artwork consent notice dismissal");
            false
        }
    }
}

pub(in crate::ui) struct ArtworkConsentBanner {
    root: gtk4::Box,
}

impl ArtworkConsentBanner {
    pub(in crate::ui) fn widget(&self) -> &gtk4::Box {
        &self.root
    }
}

pub(in crate::ui) fn install(
    db: &Rc<Db>,
    preferences: &Rc<super::preferences::PreferencesContext>,
    toolbar: &adw::ToolbarView,
) {
    let preferences = Rc::downgrade(preferences);
    if let Some(notice) = build(db, move || {
        if let Some(preferences) = preferences.upgrade() {
            preferences.present_artwork_settings();
        }
    }) {
        toolbar.add_top_bar(notice.widget());
    }
}

pub(in crate::ui) fn build(
    db: &Rc<Db>,
    on_review: impl Fn() + 'static,
) -> Option<ArtworkConsentBanner> {
    match initial_visibility(db) {
        Ok(true) => {}
        Ok(false) => return None,
        Err(error) => {
            tracing::warn!(%error, "could not read artwork consent notice state; hiding banner");
            return None;
        }
    }

    let body = strings::text(strings::ARTWORK_CONSENT_MERGE_NOTICE_BODY);
    let review = strings::text(strings::ARTWORK_CONSENT_MERGE_NOTICE_REVIEW);
    let dismiss_label = strings::text(strings::ARTWORK_CONSENT_MERGE_NOTICE_DISMISS);
    let banner = adw::Banner::new(&body);
    banner.set_use_markup(false);
    banner.set_button_label(Some(&review));
    banner.set_hexpand(true);
    let dismiss = gtk4::Button::with_label(&dismiss_label);
    dismiss.set_valign(gtk4::Align::Center);
    dismiss.set_margin_end(12);

    let root = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    root.set_hexpand(true);
    root.append(&banner);
    root.append(&dismiss);

    let completed = Rc::new(Cell::new(false));
    banner.connect_button_clicked({
        let db = db.clone();
        let completed = completed.clone();
        let root = root.clone();
        move |banner| {
            if !persist_completed(&db, &completed) {
                return;
            }
            banner.set_revealed(false);
            root.set_visible(false);
            on_review();
        }
    });
    dismiss.connect_clicked({
        let db = db.clone();
        let completed = completed.clone();
        let banner = banner.clone();
        let root = root.clone();
        move |_| {
            if !persist_completed(&db, &completed) {
                return;
            }
            banner.set_revealed(false);
            root.set_visible(false);
        }
    });
    banner.set_revealed(true);
    Some(ArtworkConsentBanner { root })
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use reprise_core::db::Db;
    use reprise_core::library::settings;

    use crate::ui::strings;

    use super::{initial_visibility, persist_completed};

    #[test]
    fn src_11_artwork_consent_notice_is_pending_only_until_consumed() {
        let untouched = Db::open_in_memory().unwrap();
        assert!(!initial_visibility(&untouched).unwrap());

        let repaired = Db::open_in_memory().unwrap();
        settings::set_bool(
            &repaired,
            settings::ARTWORK_CONSENT_MERGE_NOTICE_PENDING_KEY,
            true,
        )
        .unwrap();
        assert!(initial_visibility(&repaired).unwrap());

        let completed = Cell::new(false);
        assert!(persist_completed(&repaired, &completed));
        assert!(completed.get());
        assert!(!initial_visibility(&repaired).unwrap());
        assert!(persist_completed(&repaired, &completed));
        assert!(!initial_visibility(&repaired).unwrap());
    }

    #[test]
    fn src_11_artwork_consent_notice_names_the_merge_purposes_and_settings_action() {
        assert_eq!(
            strings::ARTWORK_CONSENT_MERGE_NOTICE_BODY,
            "Reprise merged the separate image modules into Artwork. It now loads album covers, artist portraits, and images for podcasts, YouTube, and radio."
        );
        assert_eq!(
            strings::ARTWORK_CONSENT_MERGE_NOTICE_REVIEW,
            "Review Artwork Settings"
        );
        assert_eq!(strings::ARTWORK_CONSENT_MERGE_NOTICE_DISMISS, "Dismiss");
    }
}
