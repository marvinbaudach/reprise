//! Keyboard- and assistive-technology-reachable track sorting in the browse bar.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;

use crate::ui::column_layout::{self, ColumnId};
use crate::ui::filter_bar_layout;
use crate::ui::strings;
use crate::ui::track_list::Shared;

type FieldChoice = (String, gtk4::CheckButton);

pub(in crate::ui) struct BrowseSortControl {
    button: gtk4::MenuButton,
    popover: gtk4::Popover,
    field_box: gtk4::Box,
    field_choices: Rc<RefCell<Vec<FieldChoice>>>,
    direction_choices: Vec<gtk4::CheckButton>,
    syncing: Rc<Cell<bool>>,
}

impl BrowseSortControl {
    pub(in crate::ui) fn new() -> Self {
        let field_box = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
        let direction_box = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
        let ascending = radio_choice(&strings::text(strings::SORT_ASCENDING));
        let descending = radio_choice(&strings::text(strings::SORT_DESCENDING));
        descending.set_group(Some(&ascending));
        ascending.set_active(true);
        direction_box.append(&ascending);
        direction_box.append(&descending);

        let content = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
        content.set_margin_top(10);
        content.set_margin_bottom(10);
        content.set_margin_start(10);
        content.set_margin_end(10);
        content.append(&section_label(strings::SORT_BY));
        content.append(&field_box);
        content.append(&section_label(strings::SORT_DIRECTION));
        content.append(&direction_box);

        let popover = gtk4::Popover::new();
        popover.set_autohide(true);
        popover.set_child(Some(&content));
        let keys = gtk4::EventControllerKey::new();
        let popover_weak = popover.downgrade();
        keys.connect_key_pressed(move |_, key, _, modifiers| {
            handle_key(key, modifiers, &popover_weak)
        });
        popover.add_controller(keys);

        let label = gtk4::Label::new(Some(&strings::text(strings::SORT)));
        let button = gtk4::MenuButton::new();
        button.set_child(Some(&label));
        button.set_popover(Some(&popover));
        button.add_css_class("pill");
        filter_bar_layout::style_add_filter(&button);
        button.update_property(&[gtk4::accessible::Property::Label(&strings::text(
            strings::SORT,
        ))]);
        // a11y-semantics: role=button name=explicit-label state=has-popup action=activate
        button.set_focusable(true);

        Self {
            button,
            popover,
            field_box,
            field_choices: Rc::new(RefCell::new(Vec::new())),
            direction_choices: vec![ascending, descending],
            syncing: Rc::new(Cell::new(false)),
        }
    }

    pub(in crate::ui) fn button(&self) -> &gtk4::MenuButton {
        &self.button
    }

    /// Completes construction after `Shared` and the table columns exist.
    /// `wire_sort_clicks` calls this once, after installing the sorter observer.
    pub(in crate::ui) fn wire(&self, view: &gtk4::ColumnView, shared: &Rc<Shared>) {
        debug_assert!(self.field_choices.borrow().is_empty());
        let mut first = None::<gtk4::CheckButton>;
        let mut choices = Vec::new();
        for (field, id) in sortable_columns(view) {
            let choice = radio_choice(&column_layout::column_label(id));
            if let Some(first) = &first {
                choice.set_group(Some(first));
            } else {
                first = Some(choice.clone());
            }
            let view_weak = view.downgrade();
            let shared_weak = Rc::downgrade(shared);
            let syncing = self.syncing.clone();
            let selected_field = field.clone();
            choice.connect_toggled(move |choice| {
                if syncing.get() || !choice.is_active() {
                    return;
                }
                let (Some(view), Some(shared)) = (view_weak.upgrade(), shared_weak.upgrade())
                else {
                    return;
                };
                let direction = shared.sort.borrow().dir.clone();
                apply_sort(&view, &selected_field, &direction);
            });
            self.field_box.append(&choice);
            choices.push((field, choice));
        }
        self.field_choices.replace(choices);

        for (index, choice) in self.direction_choices.iter().enumerate() {
            let view_weak = view.downgrade();
            let shared_weak = Rc::downgrade(shared);
            let syncing = self.syncing.clone();
            choice.connect_toggled(move |choice| {
                if syncing.get() || !choice.is_active() {
                    return;
                }
                let (Some(view), Some(shared)) = (view_weak.upgrade(), shared_weak.upgrade())
                else {
                    return;
                };
                let field = shared.sort.borrow().field.clone();
                let direction = if index == 1 { "desc" } else { "asc" };
                apply_sort(&view, &field, direction);
            });
        }

        let shared_weak = Rc::downgrade(shared);
        let field_choices = self.field_choices.clone();
        let direction_choices = self.direction_choices.clone();
        let syncing = self.syncing.clone();
        self.popover.connect_show(move |_| {
            let Some(shared) = shared_weak.upgrade() else {
                return;
            };
            let current = shared.sort.borrow().clone();
            // The popover has no visible state while closed. Reading the one
            // shared sort value here keeps header clicks mirrored without a
            // second observer or a second source of truth.
            sync_marks(
                &field_choices,
                &direction_choices,
                &syncing,
                &current.field,
                &current.dir,
            );
        });
    }

    #[cfg(test)]
    pub(in crate::ui) fn field_choices(&self) -> Vec<FieldChoice> {
        self.field_choices.borrow().clone()
    }

    #[cfg(test)]
    pub(in crate::ui) fn direction_choices(&self) -> Vec<gtk4::CheckButton> {
        self.direction_choices.clone()
    }

    #[cfg(test)]
    pub(in crate::ui) fn activate_field(&self, field: &str) -> bool {
        let choice = self
            .field_choices
            .borrow()
            .iter()
            .find(|(candidate, _)| candidate == field)
            .map(|(_, choice)| choice.clone());
        choice.is_some_and(|choice| choice.activate())
    }

    #[cfg(test)]
    pub(in crate::ui) fn activate_direction(&self, direction: &str) -> bool {
        let index = usize::from(direction == "desc");
        self.direction_choices
            .get(index)
            .is_some_and(gtk4::prelude::WidgetExt::activate)
    }

    #[cfg(test)]
    pub(in crate::ui) fn press_key(&self, key: gtk4::gdk::Key) -> glib::Propagation {
        handle_key(
            key,
            gtk4::gdk::ModifierType::empty(),
            &self.popover.downgrade(),
        )
    }
}

fn sortable_columns(view: &gtk4::ColumnView) -> Vec<(String, ColumnId)> {
    let columns = view.columns();
    (0..columns.n_items())
        .filter_map(|index| {
            let column = columns
                .item(index)
                .and_downcast::<gtk4::ColumnViewColumn>()?;
            let field = column.id()?.to_string();
            ColumnId::from_sort_field(&field).map(|id| (field, id))
        })
        .collect()
}

fn radio_choice(label: &str) -> gtk4::CheckButton {
    let choice = gtk4::CheckButton::builder()
        .label(label)
        .accessible_role(gtk4::AccessibleRole::Radio)
        .build();
    choice.update_property(&[gtk4::accessible::Property::Label(label)]);
    // a11y-semantics: role=radio name=explicit-label state=checked action=activate
    choice.set_focusable(true);
    choice
}

fn section_label(message: &str) -> gtk4::Label {
    let label = gtk4::Label::new(Some(&strings::text(message)));
    label.set_halign(gtk4::Align::Start);
    label.add_css_class("heading");
    label
}

fn apply_sort(view: &gtk4::ColumnView, field: &str, direction: &str) {
    let order = if direction == "desc" {
        gtk4::SortType::Descending
    } else {
        gtk4::SortType::Ascending
    };
    if !crate::ui::track_list::track_list_sort::sort_by_field(view, field, order) {
        tracing::warn!(%field, "browse sort: current field has no sortable table column");
    }
    // `sort_by_field` changes the ColumnView sorter; its existing observer is
    // the only writer of Shared::sort and the only reload trigger.
}

fn sync_marks(
    field_choices: &RefCell<Vec<FieldChoice>>,
    direction_choices: &[gtk4::CheckButton],
    syncing: &Cell<bool>,
    field: &str,
    direction: &str,
) {
    let choices = field_choices.borrow().clone();
    syncing.set(true);
    for (candidate, choice) in &choices {
        choice.set_active(candidate == field);
    }
    if let Some(ascending) = direction_choices.first() {
        ascending.set_active(direction != "desc");
    }
    if let Some(descending) = direction_choices.get(1) {
        descending.set_active(direction == "desc");
    }
    syncing.set(false);

    let focus = choices
        .iter()
        .find(|(candidate, _)| candidate == field)
        .map(|(_, choice)| choice.clone())
        .or_else(|| choices.first().map(|(_, choice)| choice.clone()));
    if let Some(choice) = focus {
        choice.grab_focus();
    }
}

fn handle_key(
    key: gtk4::gdk::Key,
    modifiers: gtk4::gdk::ModifierType,
    popover: &glib::WeakRef<gtk4::Popover>,
) -> glib::Propagation {
    if key != gtk4::gdk::Key::Escape || !modifiers.is_empty() {
        return glib::Propagation::Proceed;
    }
    if let Some(popover) = popover.upgrade() {
        popover.popdown();
    }
    glib::Propagation::Stop
}
