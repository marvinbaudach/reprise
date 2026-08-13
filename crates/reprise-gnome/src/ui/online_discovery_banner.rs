//! One-shot, non-modal discovery for the opt-in online-sources gate.

use std::cell::Cell;
use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;
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
    label: gtk4::Label,
    #[cfg(test)]
    review: gtk4::Button,
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
    let label = gtk4::Label::new(Some(&body));
    label.set_xalign(0.0);
    label.set_wrap(true);
    label.set_wrap_mode(gtk4::pango::WrapMode::WordChar);
    label.set_hexpand(true);
    let dismiss = gtk4::Button::with_label(&not_now);
    dismiss.set_valign(gtk4::Align::Center);
    dismiss.add_css_class("flat");
    let review = gtk4::Button::with_label(&review);
    review.set_valign(gtk4::Align::Center);
    review.add_css_class("suggested-action");

    let root = glib::Object::builder::<gtk4::Box>()
        .property("accessible-role", gtk4::AccessibleRole::Group)
        .property("orientation", gtk4::Orientation::Horizontal)
        .property("spacing", 8)
        .build();
    root.update_relation(&[gtk4::accessible::Relation::LabelledBy(
        &[label.upcast_ref()],
    )]);
    root.set_hexpand(true);
    root.add_css_class("toolbar");
    root.add_css_class("reprise-online-discovery-strip");
    root.append(&label);
    root.append(&dismiss);
    root.append(&review);

    let completed = Rc::new(Cell::new(false));
    review.connect_clicked({
        let db = db.clone();
        let completed = completed.clone();
        let root = root.clone();
        move |_| {
            if !persist_completed(&db, &completed) {
                return;
            }
            root.set_visible(false);
            on_review();
        }
    });
    dismiss.connect_clicked({
        let db = db.clone();
        let completed = completed.clone();
        let root = root.clone();
        move |_| {
            if !persist_completed(&db, &completed) {
                return;
            }
            root.set_visible(false);
        }
    });
    Some(OnlineDiscoveryBanner {
        root,
        #[cfg(test)]
        label,
        #[cfg(test)]
        review,
        #[cfg(test)]
        dismiss,
    })
}

pub(super) fn css() -> String {
    ".reprise-online-discovery-strip {\n\
       background-color: @headerbar_bg_color;\n\
       border-bottom: 1px solid alpha(@window_fg_color, 0.12);\n\
     }"
    .to_string()
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
    fn mtp_65_discovery_strip_owns_left_aligned_copy_both_actions_and_its_edge() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().expect("GTK test display");
        let db = Rc::new(Db::open_in_memory().unwrap());
        let strip = build(&db, || {}).expect("fresh disabled database shows discovery");
        let root = strip.widget();

        assert!(gtk4::test_accessible_has_role(
            root,
            gtk4::AccessibleRole::Group
        ));
        assert!(gtk4::test_accessible_has_relation(
            root,
            gtk4::AccessibleRelation::LabelledBy
        ));
        assert!(root.has_css_class("toolbar"));
        assert!(root.has_css_class("reprise-online-discovery-strip"));
        let children =
            std::iter::successors(root.first_child(), gtk4::prelude::WidgetExt::next_sibling)
                .collect::<Vec<_>>();
        assert_eq!(children.len(), 3, "copy and both actions share one strip");
        let copy = children[0]
            .clone()
            .downcast::<gtk4::Label>()
            .expect("leftmost child is the discovery copy");
        assert_eq!(copy.text(), BODY);
        assert_eq!(copy.xalign(), 0.0);
        assert!(copy.wraps());
        let action_labels = children[1..]
            .iter()
            .map(|child| {
                child
                    .clone()
                    .downcast::<gtk4::Button>()
                    .expect("right-side strip child is a button")
                    .label()
                    .expect("discovery action label")
                    .to_string()
            })
            .collect::<Vec<_>>();
        assert_eq!(action_labels, ["Not now", "Review in Preferences"]);

        let css = crate::ui::style::app_css_for_test();
        let rule = css
            .split(".reprise-online-discovery-strip {")
            .nth(1)
            .and_then(|tail| tail.split('}').next())
            .expect("the installed stylesheet must own the discovery strip surface");
        assert!(rule.contains("background-color: @headerbar_bg_color"));
        assert!(rule.contains("border-bottom: 1px solid alpha(@window_fg_color"));
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn net_4_discovery_banner_persists_review_and_dismiss_actions_before_hiding() {
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
        assert_eq!(banner.label.text(), BODY);
        assert_eq!(
            banner.review.label().as_deref(),
            Some("Review in Preferences")
        );
        assert_eq!(banner.dismiss.label().as_deref(), Some("Not now"));

        banner.review.emit_clicked();
        assert!(reviewed.get());
        assert!(!banner.widget().is_visible());
        assert!(settings::get_online_discovery_banner_completed(&review_db).unwrap());
        assert!(build(&review_db, || {}).is_none());

        let dismiss_db = Rc::new(Db::open_in_memory().unwrap());
        let banner = build(&dismiss_db, || {}).expect("second fresh database shows discovery");
        banner.dismiss.emit_clicked();
        assert!(!banner.widget().is_visible());
        assert!(settings::get_online_discovery_banner_completed(&dismiss_db).unwrap());
    }
}
