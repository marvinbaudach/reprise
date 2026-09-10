//! The one chip widget every filter bar builds from: a search chip (a
//! magnifier and the bare query) or a facet chip (a muted field prefix and
//! its value). Split out of `filter_bar_layout` to keep that file under the
//! repository's size limit.

use gtk4::prelude::*;

use crate::ui::style::tokens::{
    BTN_PRESS_ALPHA, CHIP_BORDER_ALPHA, CHIP_REMOVE_HOVER_BG_ALPHA, CHIP_SURFACE_ALPHA,
    FOCUS_RING_OFFSET, FOCUS_RING_WIDTH, HINT_TEXT_ALPHA, PRIMARY_TEXT_ALPHA, RADIUS_CHIP,
    SECONDARY_TEXT_ALPHA,
};

use super::filter_bar_layout::CHIP_CSS_CLASS;

pub(in crate::ui) const CHIP_ICON_CSS_CLASS: &str = "reprise-filter-chip-icon";
pub(in crate::ui) const CHIP_FIELD_CSS_CLASS: &str = "reprise-filter-chip-field";
pub(in crate::ui) const CHIP_VALUE_CSS_CLASS: &str = "reprise-filter-chip-value";
pub(in crate::ui) const CHIP_REMOVE_CSS_CLASS: &str = "reprise-filter-chip-remove";

const CHIP_SPACING: i32 = 8;
/// "+ Add filter" (`filter_bar_layout::css`) matches this so the two shapes
/// read as one family.
pub(in crate::ui) const CHIP_MIN_HEIGHT: i32 = 36;
const CHIP_ICON_NAME: &str = "system-search-symbolic";
const CHIP_ICON_SIZE: i32 = 13;
const CHIP_REMOVE_GLYPH: &str = "×";
const CHIP_REMOVE_SIZE: i32 = 22;
const CHIP_REMOVE_RADIUS: &str = "11px";

/// The minimum ×-click target FIL-1a requires of a removable chip. The
/// remove button clears it with room to spare — asserted below rather than
/// used to size anything, now that the whole chip is no longer one button.
#[cfg(test)]
const CHIP_MIN_HIT_PX: i32 = 20;

/// What a chip shows ahead of its value.
#[derive(Clone, Copy)]
pub(in crate::ui) enum ChipLead<'a> {
    /// A chip that came from the search: the magnifier marks its origin.
    Search,
    /// A chip that came from "+ Add filter": its field, muted, as a prefix.
    Field(&'a str),
    /// A chip that is its own name, such as the "Hide AI music" toggle.
    Bare,
}

/// Builds one filter-bar chip: an optional lead (icon or muted field name),
/// the value, and a round × that removes it. The container itself is not
/// focusable and carries no accessible name — the × takes over the
/// accessible name the chip used to carry as a whole button, so FIL-1a/FIL-1d
/// still name what removing the chip does.
pub(in crate::ui) fn build_chip(
    lead: ChipLead<'_>,
    value: &str,
    accessible_remove_label: &str,
    on_remove: impl Fn() + 'static,
) -> gtk4::Box {
    let chip = gtk4::Box::new(gtk4::Orientation::Horizontal, CHIP_SPACING);
    chip.add_css_class(CHIP_CSS_CLASS);
    chip.set_size_request(-1, CHIP_MIN_HEIGHT);

    match lead {
        ChipLead::Search => {
            let icon = gtk4::Image::from_icon_name(CHIP_ICON_NAME);
            icon.set_pixel_size(CHIP_ICON_SIZE);
            icon.add_css_class(CHIP_ICON_CSS_CLASS);
            chip.append(&icon);
        }
        ChipLead::Field(field) => {
            let field_label = gtk4::Label::new(Some(field));
            field_label.add_css_class(CHIP_FIELD_CSS_CLASS);
            field_label.add_css_class("caption");
            chip.append(&field_label);
        }
        ChipLead::Bare => {}
    }

    let value_label = gtk4::Label::new(Some(value));
    value_label.add_css_class(CHIP_VALUE_CSS_CLASS);
    chip.append(&value_label);

    let remove = gtk4::Button::with_label(CHIP_REMOVE_GLYPH);
    remove.add_css_class(CHIP_REMOVE_CSS_CLASS);
    remove.set_size_request(CHIP_REMOVE_SIZE, CHIP_REMOVE_SIZE);
    // `set_size_request` is only a floor; without this the box's full
    // `CHIP_MIN_HEIGHT` becomes the button's allocated height, and a round
    // border-radius over a tall rectangle draws a stadium, not a circle.
    remove.set_valign(gtk4::Align::Center);
    // a11y-semantics: role=button name=explicit-label state=focusable action=activate
    remove.set_focusable(true);
    remove.update_property(&[gtk4::accessible::Property::Label(accessible_remove_label)]);
    remove.connect_clicked(move |_| on_remove());
    chip.append(&remove);

    chip
}

/// The first direct child of `parent` carrying `class`, for tests that need
/// to reach inside a chip built by [`build_chip`] without assuming its
/// internal order.
#[cfg(test)]
pub(in crate::ui) fn child_with_css_class(
    parent: &impl IsA<gtk4::Widget>,
    class: &str,
) -> Option<gtk4::Widget> {
    let mut child = parent.as_ref().first_child();
    while let Some(widget) = child {
        if widget.has_css_class(class) {
            return Some(widget);
        }
        child = widget.next_sibling();
    }
    None
}

pub(in crate::ui) fn css() -> String {
    format!(
        ".{CHIP_CSS_CLASS} {{ border-radius: {RADIUS_CHIP}; min-height: {CHIP_MIN_HEIGHT}px; \
         padding: 0 6px 0 12px; background-color: alpha(@window_fg_color, {CHIP_SURFACE_ALPHA}); \
         border: 1px solid alpha(@window_fg_color, {CHIP_BORDER_ALPHA}); \
         border-left: 2px solid @accent_color; }} \
         .{CHIP_ICON_CSS_CLASS} {{ color: @reprise_accent_text_color; }} \
         .{CHIP_FIELD_CSS_CLASS} {{ color: alpha(@window_fg_color, {SECONDARY_TEXT_ALPHA}); }} \
         .{CHIP_VALUE_CSS_CLASS} {{ color: alpha(@window_fg_color, {PRIMARY_TEXT_ALPHA}); \
         font-weight: 500; }} \
         .{CHIP_REMOVE_CSS_CLASS} {{ min-width: {CHIP_REMOVE_SIZE}px; \
         min-height: {CHIP_REMOVE_SIZE}px; padding: 0; border-radius: {CHIP_REMOVE_RADIUS}; \
         border: none; background-color: transparent; box-shadow: none; \
         font-weight: normal; color: alpha(@window_fg_color, {HINT_TEXT_ALPHA}); }} \
         .{CHIP_REMOVE_CSS_CLASS}:hover {{ \
         background-color: alpha(@window_fg_color, {CHIP_REMOVE_HOVER_BG_ALPHA}); \
         color: alpha(@window_fg_color, {PRIMARY_TEXT_ALPHA}); }} \
         .{CHIP_REMOVE_CSS_CLASS}:active {{ \
         background-color: alpha(@window_fg_color, {BTN_PRESS_ALPHA}); \
         color: alpha(@window_fg_color, {PRIMARY_TEXT_ALPHA}); }} \
         .{CHIP_REMOVE_CSS_CLASS}:focus-visible {{ outline: {FOCUS_RING_WIDTH} solid @accent_color; \
         outline-offset: {FOCUS_RING_OFFSET}; }}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fil_1a_the_remove_button_clears_the_click_target_floor() {
        const {
            assert!(
                CHIP_REMOVE_SIZE >= CHIP_MIN_HIT_PX,
                "the remove button must clear the FIL-1a click-target floor"
            );
        }
    }
}
