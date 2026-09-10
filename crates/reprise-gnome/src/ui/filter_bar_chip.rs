//! The one chip widget every filter bar builds from: a search chip (a
//! magnifier and the bare query) or a facet chip (a muted field prefix and
//! its value). Split out of `filter_bar_layout` to keep that file under the
//! repository's size limit.
//!
//! What each chip leads with, shows and names its remove affordance is
//! decided in `reprise_view::filter_chip` — this module only renders that
//! decision into GTK widgets and translated text.

use gtk4::prelude::*;
use reprise_view::filter_chip::{ChipLead, ChipRemoveLabel, FilterChipModel};

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
/// Symbolic icons are drawn on a 16px grid, and the chip is an even number of
/// pixels tall, so 16 is the one nearby size that neither blurs nor lands
/// off-centre. Measured 2026-09-10 at 13px: the glyph was scaled by 13/16 and
/// centred at `(36 - 13) / 2 = 11.5`, half a pixel above where it belongs,
/// while the 22px `×` beside it divided evenly and looked right. Any odd
/// size brings that half-pixel back.
pub(in crate::ui) const CHIP_ICON_SIZE: i32 = 16;
const CHIP_REMOVE_GLYPH: &str = "×";
const CHIP_REMOVE_SIZE: i32 = 22;
const CHIP_REMOVE_RADIUS: &str = "11px";

/// The minimum ×-click target FIL-1a requires of a removable chip. The
/// remove button clears it with room to spare — asserted below rather than
/// used to size anything, now that the whole chip is no longer one button.
#[cfg(test)]
const CHIP_MIN_HIT_PX: i32 = 20;

/// Renders a [`ChipRemoveLabel`]: a shared, translatable message goes
/// through gettext and its placeholders; a resolved label is already final
/// text and passes through unchanged.
pub(in crate::ui) fn render_remove_label(label: &ChipRemoveLabel) -> String {
    match label {
        ChipRemoveLabel::Translatable(message) => crate::ui::filter_bar_strings::render(message),
        ChipRemoveLabel::Resolved(text) => text.clone(),
    }
}

/// Builds one filter-bar chip from its model: an optional lead (icon or
/// muted field name), the value, and a round × that removes it. The
/// container itself is not focusable and carries no accessible name — the ×
/// takes over the accessible name the chip used to carry as a whole button,
/// so FIL-1a/FIL-1d still name what removing the chip does.
pub(in crate::ui) fn build_chip(
    model: &FilterChipModel,
    on_remove: impl Fn() + 'static,
) -> gtk4::Box {
    let chip = gtk4::Box::new(gtk4::Orientation::Horizontal, CHIP_SPACING);
    chip.add_css_class(CHIP_CSS_CLASS);
    chip.set_size_request(-1, CHIP_MIN_HEIGHT);

    match &model.lead {
        ChipLead::Search => {
            let icon = gtk4::Image::from_icon_name(CHIP_ICON_NAME);
            icon.set_pixel_size(CHIP_ICON_SIZE);
            // Without this the image fills the chip's full height and does its
            // own centring inside that; with it the box centres a square 16px
            // allocation, which is the same arithmetic the `×` already uses.
            icon.set_valign(gtk4::Align::Center);
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

    let value_label = gtk4::Label::new(Some(&model.value));
    value_label.add_css_class(CHIP_VALUE_CSS_CLASS);
    chip.append(&value_label);

    let accessible_remove_label = render_remove_label(&model.accessible_remove_label);
    let remove = gtk4::Button::with_label(CHIP_REMOVE_GLYPH);
    remove.add_css_class(CHIP_REMOVE_CSS_CLASS);
    remove.set_size_request(CHIP_REMOVE_SIZE, CHIP_REMOVE_SIZE);
    // `set_size_request` is only a floor; without this the box's full
    // `CHIP_MIN_HEIGHT` becomes the button's allocated height, and a round
    // border-radius over a tall rectangle draws a stadium, not a circle.
    remove.set_valign(gtk4::Align::Center);
    // a11y-semantics: role=button name=explicit-label state=focusable action=activate
    remove.set_focusable(true);
    remove.update_property(&[gtk4::accessible::Property::Label(
        accessible_remove_label.as_str(),
    )]);
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

    // UX FIL-1d: the × accessible label stays the same regardless of which
    // fields the view searches — that promise lives in the caption
    // (`filter_bar_strings::search_2c_caption_names_the_fields_of_its_view`),
    // not on the chip itself.
    #[test]
    fn fil_1d_remove_search_label_stays_scope_independent() {
        let falling = FilterChipModel::search("falling").expect("non-blank query is a chip");
        let wer = FilterChipModel::search("wer").expect("non-blank query is a chip");
        assert_eq!(
            render_remove_label(&falling.accessible_remove_label),
            "Remove search: falling"
        );
        assert_eq!(
            render_remove_label(&wer.accessible_remove_label),
            "Remove search: wer"
        );
    }

    #[test]
    fn facet_remove_label_names_both_field_and_value() {
        let chip = FilterChipModel::facet("Genre", "Metal");
        assert_eq!(
            render_remove_label(&chip.accessible_remove_label),
            "Remove Genre filter: Metal"
        );
    }

    #[test]
    fn bare_remove_label_passes_its_resolved_text_through_unchanged() {
        let chip = FilterChipModel::bare("Hide AI music", "Remove filter: Hide AI music");
        assert_eq!(
            render_remove_label(&chip.accessible_remove_label),
            "Remove filter: Hide AI music"
        );
    }
}
