use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::library_doctor::{DoctorReviewRow, DoctorReviewRowId};

use super::review_model::{
    outcome_label, row_selectable, ConfidenceTone, ReviewRowModel, WIDE_BREAKPOINT,
};
use crate::ui::strings;

type OnSelect = Rc<dyn Fn(DoctorReviewRowId, bool)>;
type OnEdit = Rc<dyn Fn(i64)>;

struct ValueWidgets {
    section: gtk4::Box,
    value: gtk4::Label,
}

struct RowWidgets {
    root: gtk4::Box,
    selected: gtk4::CheckButton,
    track_field: ValueWidgets,
    current: ValueWidgets,
    proposed: ValueWidgets,
    source: ValueWidgets,
    warning: gtk4::Image,
    edit: gtk4::Button,
    row: RefCell<Option<DoctorReviewRow>>,
    binding: Cell<bool>,
}

pub(super) fn factory(on_select: &OnSelect, on_edit: &OnEdit) -> gtk4::SignalListItemFactory {
    let factory = gtk4::SignalListItemFactory::new();
    let states = Rc::new(RefCell::new(HashMap::<usize, Rc<RowWidgets>>::new()));
    {
        let states = states.clone();
        let on_select = on_select.clone();
        let on_edit = on_edit.clone();
        factory.connect_setup(move |_, object| {
            let Some(item) = object.downcast_ref::<gtk4::ListItem>() else {
                return;
            };
            let widgets = Rc::new(build_row());
            {
                let widgets = widgets.clone();
                let on_select = on_select.clone();
                let button = widgets.selected.clone();
                button.connect_toggled(move |button| {
                    if widgets.binding.get() {
                        return;
                    }
                    let row = widgets.row.borrow().clone();
                    if let Some(row) = row {
                        on_select(row.id, button.is_active());
                    }
                });
            }
            {
                let widgets = widgets.clone();
                let on_edit = on_edit.clone();
                let button = widgets.edit.clone();
                button.connect_clicked(move |_| {
                    let row = widgets.row.borrow().clone();
                    if let Some(row) = row {
                        on_edit(row.track_id);
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
            bind(&widgets, &model);
        });
    }
    {
        let states = states.clone();
        factory.connect_unbind(move |_, object| {
            let Some(item) = object.downcast_ref::<gtk4::ListItem>() else {
                return;
            };
            if let Some(widgets) = states.borrow().get(&list_item_key(item)) {
                widgets.row.borrow_mut().take();
            }
        });
    }
    factory
}

fn build_row() -> RowWidgets {
    let selected = gtk4::CheckButton::builder()
        .valign(gtk4::Align::Center)
        .build();
    selected.update_property(&[gtk4::accessible::Property::Label(&strings::text(
        strings::DOCTOR_SELECT_CHANGE,
    ))]);
    let track_field = value_widgets(strings::DOCTOR_TRACK_AND_FIELD, true);
    let current = value_widgets(strings::DOCTOR_CURRENT, false);
    let proposed = value_widgets(strings::DOCTOR_PROPOSED, false);
    let source = value_widgets(strings::DOCTOR_SOURCE, false);
    let warning = gtk4::Image::from_icon_name("dialog-warning-symbolic");
    warning.set_tooltip_text(Some(&strings::text(strings::DOCTOR_LOW_CONFIDENCE)));
    warning.update_property(&[gtk4::accessible::Property::Label(&strings::text(
        strings::DOCTOR_LOW_CONFIDENCE,
    ))]);
    let source_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    source_box.append(&warning);
    source_box.append(&source.value);

    let details = gtk4::Box::new(gtk4::Orientation::Horizontal, 18);
    details.set_hexpand(true);
    details.append(&track_field.section);
    details.append(&current.section);
    details.append(&proposed.section);
    details.append(&source_box);
    let responsive = adw::BreakpointBin::new();
    responsive.set_hexpand(true);
    responsive.set_child(Some(&details));
    let narrow = adw::BreakpointCondition::new_length(
        adw::BreakpointConditionLengthType::MaxWidth,
        f64::from(WIDE_BREAKPOINT),
        adw::LengthUnit::Px,
    );
    let breakpoint = adw::Breakpoint::new(narrow);
    breakpoint.add_setter(
        &details,
        "orientation",
        Some(&gtk4::Orientation::Vertical.to_value()),
    );
    responsive.add_breakpoint(breakpoint);

    let edit = gtk4::Button::builder()
        .icon_name("document-edit-symbolic")
        .tooltip_text(strings::text(strings::DOCTOR_EDIT_TRACK_TAGS))
        .valign(gtk4::Align::Center)
        .css_classes(["flat"])
        .build();
    edit.update_property(&[gtk4::accessible::Property::Label(&strings::text(
        strings::DOCTOR_EDIT_TRACK_TAGS,
    ))]);
    let root = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
    root.set_margin_top(9);
    root.set_margin_bottom(9);
    root.set_margin_start(12);
    root.set_margin_end(12);
    root.append(&selected);
    root.append(&responsive);
    root.append(&edit);
    root.set_accessible_role(gtk4::AccessibleRole::ListItem);
    RowWidgets {
        root,
        selected,
        track_field,
        current,
        proposed,
        source,
        warning,
        edit,
        row: RefCell::new(None),
        binding: Cell::new(false),
    }
}

fn value_widgets(caption: &'static str, bold: bool) -> ValueWidgets {
    let value = gtk4::Label::builder()
        .xalign(0.0)
        .ellipsize(gtk4::pango::EllipsizeMode::End)
        .hexpand(true)
        .build();
    if bold {
        value.add_css_class("heading");
    }
    let caption = gtk4::Label::builder()
        .label(strings::text(caption))
        .xalign(0.0)
        .css_classes(["caption", "dim-label"])
        .build();
    let section = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    section.set_hexpand(true);
    section.append(&caption);
    section.append(&value);
    ValueWidgets { section, value }
}

fn bind(widgets: &RowWidgets, model: &ReviewRowModel) {
    widgets.binding.set(true);
    widgets.selected.set_active(model.row.selected);
    widgets.selected.set_sensitive(row_selectable(model));
    widgets.binding.set(false);
    widgets
        .track_field
        .value
        .set_label(&format!("{} · {}", model.track, model.field));
    set_full_text(&widgets.current.value, &model.current);
    set_full_text(&widgets.proposed.value, &model.proposed);
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
    set_full_text(&widgets.source.value, &source);
    let attrs = gtk4::pango::AttrList::new();
    attrs.insert(gtk4::pango::AttrInt::new_strikethrough(true));
    widgets.current.value.set_attributes(Some(&attrs));
    widgets.warning.set_visible(model.confidence.warning);
    for class in ["accent", "warning", "error"] {
        widgets.source.value.remove_css_class(class);
    }
    let tone = match model.confidence.tone {
        ConfidenceTone::Accent => Some("accent"),
        ConfidenceTone::Normal => None,
        ConfidenceTone::Warning => Some("warning"),
        ConfidenceTone::Error => Some("error"),
    };
    if let Some(tone) = tone {
        widgets.source.value.add_css_class(tone);
    }
    let description = model.accessible_description();
    widgets.root.update_property(&[
        gtk4::accessible::Property::Label(&format!("{} · {}", model.track, model.field)),
        gtk4::accessible::Property::Description(&description),
    ]);
    widgets.edit.set_sensitive(model.row.track_id > 0);
    *widgets.row.borrow_mut() = Some(model.row.clone());
}

fn set_full_text(label: &gtk4::Label, value: &str) {
    label.set_label(value);
    label.set_tooltip_text(Some(value));
    label.update_property(&[gtk4::accessible::Property::Description(value)]);
}

fn list_item_key(item: &gtk4::ListItem) -> usize {
    item.as_ptr() as usize
}
