use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;

use super::review_header::{OnSelect, ReviewColumnGroups};
use super::review_model::{
    outcome_label, row_selectable, row_state_label, row_state_reason, ConfidenceTone, ReviewLayout,
    ReviewRowModel,
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
            if let Some(widget) = item.item().and_downcast::<gtk4::Widget>() {
                widgets.model.borrow_mut().take();
                item.set_child(Some(&widget));
                return;
            }
            let Some(boxed) = item
                .item()
                .and_then(|object| object.downcast::<glib::BoxedAnyObject>().ok())
            else {
                return;
            };
            item.set_child(Some(&widgets.root));
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
            item.set_child(gtk4::Widget::NONE);
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
    let track = value_label(true, TRACK_MAX_CHARS);
    let field = value_label(false, FIELD_MAX_CHARS);
    let current = value_label(false, VALUE_MAX_CHARS);
    current.add_css_class("doctor-review-current");
    let arrow = gtk4::Image::builder()
        .icon_name("go-next-symbolic")
        .pixel_size(15)
        .css_classes(["doctor-review-arrow"])
        .build();
    let proposed = value_label(false, VALUE_MAX_CHARS);
    let source = value_label(false, SOURCE_MAX_CHARS);
    source.add_css_class("doctor-review-source");
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
    root.set_margin_start(28);
    root.set_margin_end(28);
    root.append(&selected);
    root.append(&details);
    root.append(&edit);
    root.set_accessible_role(gtk4::AccessibleRole::ListItem);
    root.add_css_class("doctor-review-row");
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

/// Natural widths, in characters, for the row's five text columns.
///
/// An ellipsizing label still requests its whole text as its natural width.
/// Bound into a horizontal size group, that request sets the column width for
/// every row, and the row grows past a scrolled window that refuses to scroll
/// sideways — so the rightmost columns are simply not drawn. Capping the
/// natural width is what keeps every column on the page; ellipsizing then
/// shortens the few values that are genuinely too long.
pub(super) const TRACK_MAX_CHARS: i32 = 30;
pub(super) const FIELD_MAX_CHARS: i32 = 12;
pub(super) const VALUE_MAX_CHARS: i32 = 24;
pub(super) const SOURCE_MAX_CHARS: i32 = 20;

pub(super) fn value_label(bold: bool, max_chars: i32) -> gtk4::Label {
    let label = gtk4::Label::builder()
        .xalign(0.0)
        .ellipsize(gtk4::pango::EllipsizeMode::End)
        .max_width_chars(max_chars)
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
    apply_album_wide_style(&widgets.track, model.is_album_wide);
    set_full_text(&widgets.field, &model.field);
    let rendered_current = visible_edge_spaces(&model.current);
    let current = narrow_prefixed(layout, ValueKind::Current, &rendered_current);
    let proposed = narrow_prefixed(layout, ValueKind::Proposed, &model.proposed);
    set_full_text(&widgets.current, &current);
    set_full_text(&widgets.proposed, &proposed);
    let source = if let Some(outcome) = model.outcome.as_ref() {
        let status = format!(
            "{} · {}",
            model.confidence.label,
            strings::text(outcome_label(outcome.state))
        );
        outcome
            .error
            .as_ref()
            .map_or(status.clone(), |error| format!("{status} · {error}"))
    } else if let Some(state) = row_state_label(model.row.state) {
        format!("{} · {}", model.confidence.label, strings::text(state))
    } else {
        model.confidence.label.clone()
    };
    set_full_text(
        &widgets.source,
        &narrow_prefixed(layout, ValueKind::Source, &source),
    );
    widgets.current.remove_css_class("doctor-current-empty");
    if model.current == strings::text(strings::DOCTOR_EMPTY_VALUE) {
        widgets.current.set_attributes(None);
        widgets.current.add_css_class("doctor-current-empty");
    } else {
        let attrs = gtk4::pango::AttrList::new();
        let mut strikethrough = gtk4::pango::AttrInt::new_strikethrough(true);
        let (start, end) = strike_range(&current, &rendered_current);
        strikethrough.set_start_index(start);
        strikethrough.set_end_index(end);
        attrs.insert(strikethrough);
        widgets.current.set_attributes(Some(&attrs));
    }
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
    if model.row.selected {
        widgets
            .root
            .remove_css_class("doctor-review-row-deselected");
    } else {
        widgets.root.add_css_class("doctor-review-row-deselected");
    }
    let reason = row_state_reason(model.row.state).map(strings::text);
    widgets.root.set_tooltip_text(reason.as_deref());
    let description = model.accessible_description();
    widgets.root.update_property(&[
        gtk4::accessible::Property::Label(&format!("{} · {}", model.track, model.field)),
        gtk4::accessible::Property::Description(&description),
    ]);
    widgets.edit.set_sensitive(!model.track_ids.is_empty());
    *widgets.model.borrow_mut() = Some(model.clone());
}

fn apply_album_wide_style(label: &gtk4::Label, album_wide: bool) {
    if album_wide {
        let attrs = gtk4::pango::AttrList::new();
        attrs.insert(gtk4::pango::AttrInt::new_style(gtk4::pango::Style::Italic));
        label.set_attributes(Some(&attrs));
        label.add_css_class("doctor-album-wide-track");
    } else {
        label.set_attributes(None);
        label.remove_css_class("doctor-album-wide-track");
    }
}

fn visible_edge_spaces(value: &str) -> String {
    let leading = value
        .chars()
        .take_while(|character| *character == ' ')
        .count();
    let trailing = value
        .chars()
        .rev()
        .take_while(|character| *character == ' ')
        .count();
    if leading == 0 && trailing == 0 {
        return value.to_owned();
    }
    if leading == value.len() {
        return "␣".repeat(leading);
    }
    let middle = &value[leading..value.len().saturating_sub(trailing)];
    format!("{}{}{}", "␣".repeat(leading), middle, "␣".repeat(trailing))
}

#[derive(Clone, Copy)]
pub(super) enum ValueKind {
    Current,
    Proposed,
    Source,
}

/// Below the breakpoint the shared column header is hidden, so each value has
/// to name itself. Above it the header does that job and the value stands
/// alone — the same row, two ways of saying which column it is in.
pub(super) fn narrow_prefixed(layout: ReviewLayout, kind: ValueKind, value: &str) -> String {
    match layout {
        ReviewLayout::Wide => value.to_owned(),
        ReviewLayout::Narrow => match kind {
            ValueKind::Current => strings::doctor_narrow_current(value),
            ValueKind::Proposed => strings::doctor_narrow_proposed(value),
            ValueKind::Source => strings::doctor_narrow_source(value),
        },
    }
}

/// Byte range of `value` inside `rendered`, for the strikethrough.
///
/// The prefix must not be struck through — it is a label, not a superseded
/// value — and a translation is free to put it somewhere other than the front,
/// so the range is searched rather than assumed.
pub(super) fn strike_range(rendered: &str, value: &str) -> (u32, u32) {
    let start = rendered.find(value).unwrap_or(0);
    let end = start + value.len();
    (
        u32::try_from(start).unwrap_or(0),
        u32::try_from(end).unwrap_or(u32::MAX),
    )
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
pub(super) fn row_at(model: &gtk4::SortListModel, position: u32) -> Option<ReviewRowModel> {
    let object = model
        .item(position)?
        .downcast::<glib::BoxedAnyObject>()
        .ok()?;
    let row = object.borrow::<ReviewRowModel>().clone();
    Some(row)
}

/// The list's order: albums first, then the row's place inside its album.
///
/// `section_only` compares album position alone, which is what a `SectionSorter`
/// needs — a section boundary must not depend on the order *within* a section.
pub(super) fn compare_rows(
    left: &glib::Object,
    right: &glib::Object,
    section_only: bool,
) -> gtk4::Ordering {
    let left = left.downcast_ref::<glib::BoxedAnyObject>();
    let right = right.downcast_ref::<glib::BoxedAnyObject>();
    let (Some(left), Some(right)) = (left, right) else {
        return match (left.is_some(), right.is_some()) {
            (true, false) => gtk4::Ordering::Smaller,
            (false, true) => gtk4::Ordering::Larger,
            _ => gtk4::Ordering::Equal,
        };
    };
    let left = left.borrow::<ReviewRowModel>();
    let right = right.borrow::<ReviewRowModel>();
    let ordering = if section_only {
        left.album_position.cmp(&right.album_position)
    } else {
        (left.album_position, left.row_position).cmp(&(right.album_position, right.row_position))
    };
    match ordering {
        std::cmp::Ordering::Less => gtk4::Ordering::Smaller,
        std::cmp::Ordering::Equal => gtk4::Ordering::Equal,
        std::cmp::Ordering::Greater => gtk4::Ordering::Larger,
    }
}

#[cfg(test)]
#[path = "review_row_contract_tests.rs"]
pub(in crate::ui::library_doctor) mod contract_tests;
