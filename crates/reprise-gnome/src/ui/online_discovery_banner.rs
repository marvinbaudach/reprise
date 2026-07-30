//! One-shot, non-modal discovery for the opt-in online-sources gate.

use std::cell::Cell;
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use reprise_core::db::Db;
use reprise_core::library::settings;

use super::strings;

fn should_show(completed: bool, online_sources_enabled: bool) -> bool {
    !completed && !online_sources_enabled
}

fn initial_visibility(db: &Db) -> Result<bool, rusqlite::Error> {
    let completed = settings::get_online_discovery_banner_completed(db)?;
    let online_sources_enabled = reprise_core::online_sources::is_enabled(db)?;
    Ok(should_show(completed, online_sources_enabled))
}

fn persist_completed(db: &Db, completed: &Cell<bool>) -> bool {
    if completed.get() {
        return true;
    }
    match settings::set_online_discovery_banner_completed(db, true) {
        Ok(()) => {
            completed.set(true);
            true
        }
        Err(error) => {
            tracing::warn!(%error, "could not persist online discovery dismissal");
            false
        }
    }
}

pub(in crate::ui) struct OnlineDiscoveryBanner {
    root: gtk4::Box,
    #[cfg(test)]
    banner: adw::Banner,
    #[cfg(test)]
    dismiss: gtk4::Button,
}

impl OnlineDiscoveryBanner {
    pub(in crate::ui) fn widget(&self) -> &gtk4::Box {
        &self.root
    }
}

pub(in crate::ui) fn build(
    db: &Rc<Db>,
    on_review: impl Fn() + 'static,
) -> Option<OnlineDiscoveryBanner> {
    match initial_visibility(db) {
        Ok(true) => {}
        Ok(false) => return None,
        Err(error) => {
            tracing::warn!(%error, "could not read online discovery state; hiding banner");
            return None;
        }
    }

    let body = strings::text(strings::ONLINE_DISCOVERY_BANNER_BODY);
    let review = strings::text(strings::ONLINE_DISCOVERY_REVIEW);
    let not_now = strings::text(strings::ONLINE_DISCOVERY_NOT_NOW);
    let banner = adw::Banner::new(&body);
    banner.set_use_markup(false);
    banner.set_button_label(Some(&review));
    banner.set_hexpand(true);
    let dismiss = gtk4::Button::with_label(&not_now);
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
    Some(OnlineDiscoveryBanner {
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

    use super::{build, should_show};

    const BODY: &str = "Reprise can now follow podcasts, YouTube channels, radio and concerts — all off by default.";

    #[test]
    fn discovery_banner_visibility_is_one_shot_and_gate_aware() {
        assert!(should_show(false, false));
        assert!(!should_show(true, false));
        assert!(!should_show(false, true));
        assert!(!should_show(true, true));
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn discovery_banner_persists_review_and_dismiss_actions_before_hiding() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        if gtk4::init().is_err() {
            return;
        }

        let review_db = Rc::new(Db::open_in_memory().unwrap());
        let reviewed = Rc::new(Cell::new(false));
        let banner = build(&review_db, {
            let reviewed = reviewed.clone();
            move || reviewed.set(true)
        })
        .expect("fresh disabled database shows discovery");
        assert_eq!(banner.banner.title(), BODY);
        assert_eq!(
            banner.banner.button_label().as_deref(),
            Some("Review in Preferences")
        );
        assert!(banner.banner.is_revealed());
        assert_eq!(banner.dismiss.label().as_deref(), Some("Not now"));

        banner.banner.emit_by_name::<()>("button-clicked", &[]);
        assert!(reviewed.get());
        assert!(!banner.banner.is_revealed());
        assert!(!banner.widget().is_visible());
        assert!(settings::get_online_discovery_banner_completed(&review_db).unwrap());
        assert!(build(&review_db, || {}).is_none());

        let dismiss_db = Rc::new(Db::open_in_memory().unwrap());
        let banner = build(&dismiss_db, || {}).expect("second fresh database shows discovery");
        banner.dismiss.emit_clicked();
        assert!(!banner.banner.is_revealed());
        assert!(!banner.widget().is_visible());
        assert!(settings::get_online_discovery_banner_completed(&dismiss_db).unwrap());
    }
}
