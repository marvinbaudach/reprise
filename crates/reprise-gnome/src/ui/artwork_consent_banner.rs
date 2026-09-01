//! One-shot disclosure for artwork consent inherited from retired modules.

use std::cell::Cell;
use std::rc::Rc;

use gtk4::{glib, prelude::*};
use libadwaita as adw;
use reprise_core::db::Db;
use reprise_core::library::settings;

use super::strings;

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
    #[cfg(test)]
    banner: adw::Banner,
    #[cfg(test)]
    dismiss: gtk4::Button,
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
    match settings::get_bool(
        db,
        settings::ARTWORK_CONSENT_MERGE_NOTICE_PENDING_KEY,
        false,
    ) {
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
    let review_db = db.clone();
    let review_completed = completed.clone();
    banner.connect_button_clicked(glib::clone!(
        #[weak]
        root,
        move |banner| {
            if !persist_completed(&review_db, &review_completed) {
                return;
            }
            banner.set_revealed(false);
            root.set_visible(false);
            on_review();
        }
    ));
    let dismiss_db = db.clone();
    let dismiss_completed = completed;
    dismiss.connect_clicked(glib::clone!(
        #[weak]
        banner,
        #[weak]
        root,
        move |_| {
            if !persist_completed(&dismiss_db, &dismiss_completed) {
                return;
            }
            banner.set_revealed(false);
            root.set_visible(false);
        }
    ));
    banner.set_revealed(true);
    Some(ArtworkConsentBanner {
        root,
        #[cfg(test)]
        banner,
        #[cfg(test)]
        dismiss,
    })
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use gtk4::prelude::*;
    use reprise_core::db::Db;
    use reprise_core::library::settings;

    use crate::ui::strings;

    use super::{build, persist_completed};

    #[test]
    fn src_11_artwork_consent_notice_is_pending_only_until_consumed() {
        let untouched = Db::open_in_memory().unwrap();
        assert!(!settings::get_bool(
            &untouched,
            settings::ARTWORK_CONSENT_MERGE_NOTICE_PENDING_KEY,
            false,
        )
        .unwrap());

        let repaired = Db::open_in_memory().unwrap();
        settings::set_bool(
            &repaired,
            settings::ARTWORK_CONSENT_MERGE_NOTICE_PENDING_KEY,
            true,
        )
        .unwrap();
        assert!(settings::get_bool(
            &repaired,
            settings::ARTWORK_CONSENT_MERGE_NOTICE_PENDING_KEY,
            false,
        )
        .unwrap());

        let completed = Cell::new(false);
        assert!(persist_completed(&repaired, &completed));
        assert!(completed.get());
        assert!(!settings::get_bool(
            &repaired,
            settings::ARTWORK_CONSENT_MERGE_NOTICE_PENDING_KEY,
            false,
        )
        .unwrap());
        assert!(persist_completed(&repaired, &completed));
        assert!(!settings::get_bool(
            &repaired,
            settings::ARTWORK_CONSENT_MERGE_NOTICE_PENDING_KEY,
            false,
        )
        .unwrap());
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

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn src_11_artwork_consent_banner_persists_review_and_dismiss_actions_before_hiding() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        if gtk4::init().is_err() {
            return;
        }

        let review_db = Rc::new(Db::open_in_memory().unwrap());
        settings::set_bool(
            &review_db,
            settings::ARTWORK_CONSENT_MERGE_NOTICE_PENDING_KEY,
            true,
        )
        .unwrap();
        let reviewed = Rc::new(Cell::new(false));
        let banner = build(&review_db, {
            let reviewed = reviewed.clone();
            move || reviewed.set(true)
        })
        .expect("inherited consent shows the notice");
        assert_eq!(
            banner.banner.title(),
            strings::ARTWORK_CONSENT_MERGE_NOTICE_BODY
        );
        assert_eq!(
            banner.banner.button_label().as_deref(),
            Some(strings::ARTWORK_CONSENT_MERGE_NOTICE_REVIEW)
        );
        assert!(banner.banner.is_revealed());
        assert_eq!(
            banner.dismiss.label().as_deref(),
            Some(strings::ARTWORK_CONSENT_MERGE_NOTICE_DISMISS)
        );

        banner.banner.emit_by_name::<()>("button-clicked", &[]);
        assert!(reviewed.get());
        assert!(!settings::get_bool(
            &review_db,
            settings::ARTWORK_CONSENT_MERGE_NOTICE_PENDING_KEY,
            false,
        )
        .unwrap());
        assert!(!banner.banner.is_revealed());
        assert!(!banner.widget().is_visible());
        assert!(build(&review_db, || {}).is_none());

        let dismiss_db = Rc::new(Db::open_in_memory().unwrap());
        settings::set_bool(
            &dismiss_db,
            settings::ARTWORK_CONSENT_MERGE_NOTICE_PENDING_KEY,
            true,
        )
        .unwrap();
        let banner = build(&dismiss_db, || {}).expect("second inherited consent shows the notice");
        banner.dismiss.emit_clicked();
        assert!(!settings::get_bool(
            &dismiss_db,
            settings::ARTWORK_CONSENT_MERGE_NOTICE_PENDING_KEY,
            false,
        )
        .unwrap());
        assert!(!banner.banner.is_revealed());
        assert!(!banner.widget().is_visible());
        assert!(build(&dismiss_db, || {}).is_none());
    }
}
