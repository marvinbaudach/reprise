use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;

use super::review_model::ReviewCategory;
use crate::ui::strings;

type OnChanged = Rc<dyn Fn(Option<ReviewCategory>)>;

pub(super) struct ReviewFilterBar {
    pub(super) root: gtk4::Box,
    slot: gtk4::Box,
    toggle: RefCell<adw::ToggleGroup>,
    categories: RefCell<Vec<ReviewCategory>>,
    callback: RefCell<Option<OnChanged>>,
    sensitive: Cell<bool>,
    summary: gtk4::Label,
}

impl ReviewFilterBar {
    pub(super) fn new(categories: &[ReviewCategory]) -> Self {
        let summary = gtk4::Label::builder()
            .xalign(0.0)
            .hexpand(true)
            .css_classes(["caption", "dim-label"])
            .build();
        let hint = gtk4::Label::builder()
            .label(strings::doctor_preselected_hint())
            .xalign(0.0)
            .wrap(true)
            .css_classes(["caption", "dim-label"])
            .build();
        let slot = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        let initial_toggle = build_toggle(&[], "all");
        slot.append(&initial_toggle);
        let top = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
        top.append(&slot);
        top.append(&summary);
        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
        root.set_margin_start(18);
        root.set_margin_end(18);
        root.append(&top);
        root.append(&hint);
        let bar = Self {
            root,
            slot,
            toggle: RefCell::new(initial_toggle),
            categories: RefCell::new(Vec::new()),
            callback: RefCell::new(None),
            sensitive: Cell::new(true),
            summary,
        };
        bar.set_categories(categories);
        bar
    }

    pub(super) fn connect_changed(&self, callback: OnChanged) {
        self.callback.replace(Some(callback.clone()));
        connect_toggle(
            &self.toggle.borrow(),
            self.categories.borrow().clone(),
            callback,
        );
    }

    pub(super) fn set_categories(&self, categories: &[ReviewCategory]) {
        if self.categories.borrow().as_slice() == categories {
            return;
        }
        let previous = self.toggle.borrow().active_name();
        let active = previous
            .as_deref()
            .filter(|name| {
                *name == "all" || categories.iter().any(|category| category.name() == *name)
            })
            .unwrap_or("all");
        let toggle = build_toggle(categories, active);
        toggle.set_sensitive(self.sensitive.get());
        if let Some(callback) = self.callback.borrow().clone() {
            connect_toggle(&toggle, categories.to_vec(), callback);
        }
        let prior = self.toggle.replace(toggle.clone());
        if prior.parent().is_some() {
            self.slot.remove(&prior);
        }
        self.slot.append(&toggle);
        self.categories.replace(categories.to_vec());
    }

    pub(super) fn set_summary(&self, changes: usize, albums: usize) {
        self.summary
            .set_label(&strings::doctor_changes_and_albums(changes, albums));
    }

    pub(super) fn set_sensitive(&self, sensitive: bool) {
        self.sensitive.set(sensitive);
        self.toggle.borrow().set_sensitive(sensitive);
    }
}

fn build_toggle(categories: &[ReviewCategory], active: &str) -> adw::ToggleGroup {
    let toggle = adw::ToggleGroup::new();
    toggle.add(
        adw::Toggle::builder()
            .name("all")
            .label(strings::text(strings::DOCTOR_ALL))
            .build(),
    );
    for category in categories {
        toggle.add(
            adw::Toggle::builder()
                .name(category.name())
                .label(strings::text(category.label()))
                .build(),
        );
    }
    toggle.set_active_name(Some(active));
    // a11y-semantics: role=group name=doctor-filter state=one-selected action=arrow-keys
    toggle.update_property(&[gtk4::accessible::Property::Label("Filter tag changes")]);
    toggle
}

fn connect_toggle(toggle: &adw::ToggleGroup, categories: Vec<ReviewCategory>, callback: OnChanged) {
    toggle.connect_active_name_notify(move |toggle| {
        let name = toggle.active_name();
        let selected = categories
            .iter()
            .copied()
            .find(|category| name.as_deref() == Some(category.name()));
        callback(selected);
    });
}

#[cfg(test)]
mod tests {
    use reprise_core::library_doctor::ProblemClass;

    use super::ReviewCategory;

    #[test]
    fn doc_9b_the_filter_bar_offers_only_categories_present_in_the_scan() {
        let present = [ProblemClass::CasingWhitespace, ProblemClass::GenreVariant];
        let categories = [
            ReviewCategory::Casing,
            ReviewCategory::Year,
            ReviewCategory::Genre,
        ]
        .into_iter()
        .filter(|category| present.iter().any(|class| category.matches(*class)))
        .collect::<Vec<_>>();
        let names = categories
            .iter()
            .map(|category| category.name())
            .collect::<Vec<_>>();
        assert_eq!(names, ["casing", "genre"]);
        assert!(!names.contains(&"year"));
    }
}
