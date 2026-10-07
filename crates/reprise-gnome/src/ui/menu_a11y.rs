//! Accessible names for the items of a `gio::MenuModel` popover.
//!
//! GTK builds every item of a `GtkPopoverMenu` as a `GtkModelButton`, whose
//! text lives in a private `GtkLabel` with the presentation role, and it names
//! the button through a labelled-by relation to that label. Since GTK commit
//! `fb6f2118` ("label: Don't set a11y label when role is presentation", GTK
//! 4.18) such a label no longer copies its text into its accessible label,
//! because the presentation role is `NAME_PROHIBITED`. The name computation
//! follows the button's labelled-by relation into that label, finds nothing,
//! and never falls back to the button's own label property. Every menu item
//! therefore reaches AT-SPI as a `menu item` with an empty name.
//!
//! Setting `Property::Label` alone does not help, because a labelled-by relation
//! takes precedence over it. [`name_model_buttons`] removes the relation and
//! sets the label from the item's visible text.
//!
//! This module is a workaround and can be removed, together with its call
//! sites, once GTK names its model buttons itself.
//!
//! Build every popover from a model with [`popover_menu_from_model`], or call
//! [`name_model_buttons_on_map`] for a popover another builder made, such as the
//! one a `GtkMenuButton` creates ([`name_menu_button_items`]). A source scan in
//! this module's tests fails when a site bypasses it.

use gtk4::prelude::*;
use gtk4::{gio, glib};

const MODEL_BUTTON_TYPE: &str = "GtkModelButton";

/// A `PopoverMenu::from_model` whose items are named for assistive technology.
pub(crate) fn popover_menu_from_model(model: &impl IsA<gio::MenuModel>) -> gtk4::PopoverMenu {
    let popover = gtk4::PopoverMenu::from_model(Some(model));
    name_model_buttons_on_map(&popover);
    popover
}

/// Names the items of `popover` now and again every time it maps or its model
/// is replaced, which rebuilds the buttons.
pub(crate) fn name_model_buttons_on_map(popover: &gtk4::PopoverMenu) {
    name_model_buttons(popover.upcast_ref());
    popover.connect_map(|popover| name_model_buttons(popover.upcast_ref()));
    popover.connect_menu_model_notify(|popover| name_model_buttons(popover.upcast_ref()));
}

/// [`name_model_buttons_on_map`] for the popover a `GtkMenuButton` builds from
/// its menu model. A button with a different kind of popover is left alone.
pub(crate) fn name_menu_button_items(button: &gtk4::MenuButton) {
    if let Some(popover) = button
        .popover()
        .and_then(|popover| popover.downcast::<gtk4::PopoverMenu>().ok())
    {
        name_model_buttons_on_map(&popover);
    }
}

/// Walks every descendant of `root`, the unmapped submenu pages included, and
/// gives each model button the visible text as its accessible label in place of
/// the labelled-by relation GTK installed.
fn name_model_buttons(root: &gtk4::Widget) {
    let mut child = root.first_child();
    while let Some(widget) = child {
        if widget.type_().name() == MODEL_BUTTON_TYPE {
            name_model_button(&widget);
        }
        name_model_buttons(&widget);
        child = widget.next_sibling();
    }
}

fn name_model_button(button: &gtk4::Widget) {
    let Some(text) = visible_text(button) else {
        return;
    };
    let Some(accessible) = button.dynamic_cast_ref::<gtk4::Accessible>() else {
        return;
    };
    accessible.reset_relation(gtk4::AccessibleRelation::LabelledBy);
    accessible.update_property(&[gtk4::accessible::Property::Label(&text)]);
}

/// The text the item draws. It is read from the button's own label child
/// because the `text` property still carries the mnemonic underscore a screen
/// reader must not speak.
fn visible_text(button: &gtk4::Widget) -> Option<glib::GString> {
    std::iter::successors(button.first_child(), gtk4::prelude::WidgetExt::next_sibling)
        .find_map(|child| child.downcast::<gtk4::Label>().ok())
        .map(|label| label.text())
        .filter(|text| !text.trim().is_empty())
}

#[cfg(test)]
#[path = "menu_a11y_tests.rs"]
mod tests;
