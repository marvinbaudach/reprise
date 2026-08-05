use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;

use super::review_header::{OnSelect, ReviewColumnGroups};
use super::review_model::{
    outcome_label, row_selectable, ConfidenceTone, ReviewLayout, ReviewRowModel,
};
use crate::ui::strings;

type OnEdit = Rc<dyn Fn(&[i64])>;

struct RowWidgets {
    root: gtk4::Box,
    selected: gtk4::CheckButton,
    details: gtk4::Box,
    track: gtk4::Label,
    field: gtk4::Label,
    current: gtk4::Label,
    proposed: gtk4::Label,
    source: gtk4::Label,
    warning: gtk4::Image,
    edit: gtk4::Button,
    model: RefCell<Option<ReviewRowModel>>,
    binding: Cell<bool>,
}

pub(super) fn factory(
    on_select: &OnSelect,
    on_edit: &OnEdit,
    groups: &ReviewColumnGroups,
    layout: &Rc<Cell<ReviewLayout>>,
) -> gtk4::SignalListItemFactory {
    let factory = gtk4::SignalListItemFactory::new();
    let states = Rc::new(RefCell::new(HashMap::<usize, Rc<RowWidgets>>::new()));
    {
        let states = states.clone();
        let on_select = on_select.clone();
        let on_edit = on_edit.clone();
        let groups = groups.clone();
        factory.connect_setup(move |_, object| {
            let Some(item) = object.downcast_ref::<gtk4::ListItem>() else {
                return;
            };
            let widgets = Rc::new(build_row(&groups));
            {
                let widgets = widgets.clone();
                let on_select = on_select.clone();
                widgets.selected.clone().connect_toggled(move |button| {
                    if widgets.binding.get() {
                        return;
                    }
                    if let Some(model) = widgets.model.borrow().as_ref() {
                        on_select(&model.selectable_row_ids, button.is_active());
                    }
                });
            }
            {
                let widgets = widgets.clone();
                let on_edit = on_edit.clone();
                widgets.edit.clone().connect_clicked(move |_| {
                    if let Some(model) = widgets.model.borrow().as_ref() {
                        on_edit(&model.track_ids);
                    }
                });
            }
            states
                .borrow_mut()
                .insert(list_item_key(item), widgets.clone());
            item.set_child(Some(&widgets.root));
        });
    }
    {
        let states = states.clone();
        let layout = layout.clone();
        factory.connect_bind(move |_, object| {
            let Some(item) = object.downcast_ref::<gtk4::ListItem>() else {
                return;
            };
            let Some(widgets) = states.borrow().get(&list_item_key(item)).cloned() else {
                return;
            };
            let Some(boxed) = item
                .item()
                .and_then(|object| object.downcast::<glib::BoxedAnyObject>().ok())
            else {
                return;
            };
            let model = boxed.borrow::<ReviewRowModel>();
            bind(&widgets, &model, layout.get());
        });
    }
    {
        let states = states.clone();
        factory.connect_unbind(move |_, object| {
            let Some(item) = object.downcast_ref::<gtk4::ListItem>() else {
                return;
            };
            if let Some(widgets) = states.borrow().get(&list_item_key(item)) {
                widgets.model.borrow_mut().take();
            }
        });
    }
    factory
}

fn build_row(groups: &ReviewColumnGroups) -> RowWidgets {
    let selected = gtk4::CheckButton::builder()
        .valign(gtk4::Align::Center)
        .build();
    selected.update_property(&[gtk4::accessible::Property::Label(&strings::text(
        strings::DOCTOR_SELECT_CHANGE,
    ))]);
    let track = value_label(true);
    let field = value_label(false);
    let current = value_label(false);
    let arrow = gtk4::Label::new(Some("→"));
    let proposed = value_label(false);
    let source = value_label(false);
    let warning = gtk4::Image::from_icon_name("dialog-warning-symbolic");
    warning.set_tooltip_text(Some(&strings::text(strings::DOCTOR_LOW_CONFIDENCE)));
    warning.update_property(&[gtk4::accessible::Property::Label(&strings::text(
        strings::DOCTOR_LOW_CONFIDENCE,
    ))]);
    let source_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    source_box.set_hexpand(true);
    source_box.append(&warning);
    source_box.append(&source);

    let details = gtk4::Box::new(gtk4::Orientation::Horizontal, 18);
    details.set_hexpand(true);
    for widget in [
        track.clone().upcast::<gtk4::Widget>(),
        field.clone().upcast(),
        current.clone().upcast(),
        arrow.clone().upcast(),
        proposed.clone().upcast(),
        source_box.clone().upcast(),
    ] {
        details.append(&widget);
    }

    let edit = gtk4::Button::builder()
        .icon_name("document-edit-symbolic")
        .tooltip_text(strings::text(strings::DOCTOR_EDIT_TRACK_TAGS))
        .valign(gtk4::Align::Center)
        .css_classes(["flat"])
        .build();
    edit.update_property(&[gtk4::accessible::Property::Label(&strings::text(
        strings::DOCTOR_EDIT_TRACK_TAGS,
    ))]);
    for (group, widget) in [
        (&groups.selection, selected.clone().upcast::<gtk4::Widget>()),
        (&groups.track, track.clone().upcast()),
        (&groups.field, field.clone().upcast()),
        (&groups.current, current.clone().upcast()),
        (&groups.arrow, arrow.clone().upcast()),
        (&groups.proposed, proposed.clone().upcast()),
        (&groups.source, source_box.clone().upcast()),
        (&groups.edit, edit.clone().upcast()),
    ] {
        group.add_widget(&widget);
    }

    let root = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
    root.set_margin_top(9);
    root.set_margin_bottom(9);
    root.set_margin_start(12);
    root.set_margin_end(12);
    root.append(&selected);
    root.append(&details);
    root.append(&edit);
    root.set_accessible_role(gtk4::AccessibleRole::ListItem);
    RowWidgets {
        root,
        selected,
        details,
        track,
        field,
        current,
        proposed,
        source,
        warning,
        edit,
        model: RefCell::new(None),
        binding: Cell::new(false),
    }
}

fn value_label(bold: bool) -> gtk4::Label {
    let label = gtk4::Label::builder()
        .xalign(0.0)
        .ellipsize(gtk4::pango::EllipsizeMode::End)
        .hexpand(true)
        .build();
    if bold {
        label.add_css_class("heading");
    }
    label
}

fn bind(widgets: &RowWidgets, model: &ReviewRowModel, layout: ReviewLayout) {
    widgets.binding.set(true);
    widgets.selected.set_active(model.row.selected);
    widgets.selected.set_inconsistent(
        model.selected_change_count > 0
            && model.selected_change_count < model.selectable_row_ids.len(),
    );
    widgets.selected.set_sensitive(row_selectable(model));
    widgets.binding.set(false);
    widgets.details.set_orientation(match layout {
        ReviewLayout::Wide => gtk4::Orientation::Horizontal,
        ReviewLayout::Narrow => gtk4::Orientation::Vertical,
    });
    set_full_text(&widgets.track, &model.track);
    set_full_text(&widgets.field, &model.field);
    set_full_text(&widgets.current, &model.current);
    set_full_text(&widgets.proposed, &model.proposed);
    let source = model.outcome.as_ref().map_or_else(
        || model.confidence.label.clone(),
        |outcome| {
            let status = format!(
                "{} · {}",
                model.confidence.label,
                strings::text(outcome_label(outcome.state))
            );
            outcome
                .error
                .as_ref()
                .map_or(status.clone(), |error| format!("{status} · {error}"))
        },
    );
    set_full_text(&widgets.source, &source);
    let attrs = gtk4::pango::AttrList::new();
    attrs.insert(gtk4::pango::AttrInt::new_strikethrough(true));
    widgets.current.set_attributes(Some(&attrs));
    widgets.warning.set_visible(model.confidence.warning);
    for class in ["accent", "warning", "error"] {
        widgets.source.remove_css_class(class);
    }
    let tone = match model.confidence.tone {
        ConfidenceTone::Accent => Some("accent"),
        ConfidenceTone::Normal => None,
        ConfidenceTone::Warning => Some("warning"),
        ConfidenceTone::Error => Some("error"),
    };
    if let Some(tone) = tone {
        widgets.source.add_css_class(tone);
    }
    let description = model.accessible_description();
    widgets.root.update_property(&[
        gtk4::accessible::Property::Label(&format!("{} · {}", model.track, model.field)),
        gtk4::accessible::Property::Description(&description),
    ]);
    widgets.edit.set_sensitive(!model.track_ids.is_empty());
    *widgets.model.borrow_mut() = Some(model.clone());
}

fn set_full_text(label: &gtk4::Label, value: &str) {
    label.set_label(value);
    label.set_tooltip_text(Some(value));
    label.update_property(&[gtk4::accessible::Property::Description(value)]);
}

fn list_item_key(item: &gtk4::ListItem) -> usize {
    item.as_ptr() as usize
}

#[cfg(test)]
#[path = "review_row_contract_tests.rs"]
mod contract_tests;
