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
//! `menu_a11y_guard_tests.rs` fails when a site bypasses it.
//!
//! GTK puts the relation back whenever a button refreshes its accessible
//! properties, and it creates fresh buttons when a model changes, so naming once
//! is not enough. For one popover the items are named:
//!
//! - when the popover is set up;
//! - every time it maps;
//! - when its menu model is replaced;
//! - when the model, or any section or submenu below it, emits `items-changed`,
//!   which covers in-place edits such as `remove_all` plus `append`;
//! - on each model button's notification of a property whose setter makes GTK
//!   refresh the button's accessible properties ([`RELABELING_PROPERTIES`]):
//!   `active` is how a check or radio item reports an action state change, and
//!   `text`, `role`, `accel`, `menu-name` and `popover` change when the model
//!   edits an item in place.
//!
//! Each button, each model and each popover is connected to at most once, however
//! often the popover re-maps or the walks repeat. The bookkeeping holds weak
//! references only, and the handlers hold the popover weakly, so naming keeps
//! nothing alive.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::gio;
use gtk4::glib::{self, WeakRef};
use gtk4::prelude::*;

const MODEL_BUTTON_TYPE: &str = "GtkModelButton";
/// The `GtkModelButton` properties whose setters restore the labelled-by
/// relation. GTK notifies each one after its setter has updated the button.
const RELABELING_PROPERTIES: [&str; 6] =
    ["active", "text", "role", "accel", "menu-name", "popover"];
const MENU_LINKS: [&str; 2] = ["section", "submenu"];

/// What one popover has been connected to. Weak references only: an entry
/// whose object is gone is dropped the next time the list is consulted.
#[derive(Default)]
struct Connected {
    buttons: RefCell<Vec<WeakRef<gtk4::Widget>>>,
    models: RefCell<Vec<WeakRef<gio::MenuModel>>>,
}

impl Connected {
    /// Records `object` and returns whether it was new.
    fn remember<T: ObjectType>(list: &RefCell<Vec<WeakRef<T>>>, object: &T) -> bool {
        let mut list = list.borrow_mut();
        list.retain(|weak| weak.upgrade().is_some());
        if list
            .iter()
            .any(|weak| weak.upgrade().as_ref() == Some(object))
        {
            return false;
        }
        list.push(object.downgrade());
        true
    }
}

thread_local! {
    /// The popovers already set up, so a second call for one popover is a no-op.
    static NAMED_POPOVERS: RefCell<Vec<WeakRef<gtk4::PopoverMenu>>> =
        const { RefCell::new(Vec::new()) };
    /// How many signal handlers this module has connected on this thread.
    #[cfg(test)]
    static CONNECTIONS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

fn count_connection() {
    #[cfg(test)]
    CONNECTIONS.with(|count| count.set(count.get() + 1));
}

/// A `PopoverMenu::from_model` whose items are named for assistive technology.
pub(crate) fn popover_menu_from_model(model: &impl IsA<gio::MenuModel>) -> gtk4::PopoverMenu {
    let popover = gtk4::PopoverMenu::from_model(Some(model));
    name_model_buttons_on_map(&popover);
    popover
}

/// Keeps the items of `popover` named: now, on every map, when its model is
/// replaced, on `items-changed` of the model and of every section and submenu
/// below it, and when a button reports a relabeling property. Calling it again
/// for the same popover does nothing.
pub(crate) fn name_model_buttons_on_map(popover: &gtk4::PopoverMenu) {
    let first_call = NAMED_POPOVERS.with(|named| {
        let mut named = named.borrow_mut();
        named.retain(|weak| weak.upgrade().is_some());
        if named
            .iter()
            .any(|weak| weak.upgrade().as_ref() == Some(popover))
        {
            return false;
        }
        named.push(popover.downgrade());
        true
    });
    if !first_call {
        return;
    }
    let connected = Rc::new(Connected::default());
    refresh(popover, &connected);

    let weak = popover.downgrade();
    popover.connect_map({
        let (weak, connected) = (weak.clone(), connected.clone());
        move |_| refresh_weak(&weak, &connected)
    });
    popover.connect_menu_model_notify(move |_| refresh_weak(&weak, &connected));
    count_connection();
    count_connection();
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

fn refresh_weak(popover: &WeakRef<gtk4::PopoverMenu>, connected: &Rc<Connected>) {
    if let Some(popover) = popover.upgrade() {
        refresh(&popover, connected);
    }
}

/// Follows the popover's current model tree and names every model button.
fn refresh(popover: &gtk4::PopoverMenu, connected: &Rc<Connected>) {
    if let Some(model) = popover.menu_model() {
        follow_model(&popover.downgrade(), connected, &model);
    }
    name_model_buttons(popover.upcast_ref(), connected);
}

/// Connects `items-changed` once on `model` and on every section and submenu
/// below it, including the ones added since the last walk.
fn follow_model(
    popover: &WeakRef<gtk4::PopoverMenu>,
    connected: &Rc<Connected>,
    model: &gio::MenuModel,
) {
    if Connected::remember(&connected.models, model) {
        let (popover, connected) = (popover.clone(), connected.clone());
        model.connect_items_changed(move |_, _, _, _| refresh_weak(&popover, &connected));
        count_connection();
    }
    for item in 0..model.n_items() {
        for link in MENU_LINKS {
            if let Some(child) = model.item_link(item, link) {
                follow_model(popover, connected, &child);
            }
        }
    }
}

/// Walks every descendant of `root`, the unmapped submenu pages included, and
/// gives each model button the visible text as its accessible label in place of
/// the labelled-by relation GTK installed. A button is also named again
/// whenever it notifies one of [`RELABELING_PROPERTIES`].
fn name_model_buttons(root: &gtk4::Widget, connected: &Connected) {
    let mut child = root.first_child();
    while let Some(widget) = child {
        if widget.type_().name() == MODEL_BUTTON_TYPE {
            name_model_button(&widget);
            if Connected::remember(&connected.buttons, &widget) {
                widget.connect_notify_local(None, |button, pspec| {
                    if RELABELING_PROPERTIES.contains(&pspec.name()) {
                        name_model_button(button.upcast_ref());
                    }
                });
                count_connection();
            }
        }
        name_model_buttons(&widget, connected);
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
fn connection_count() -> usize {
    CONNECTIONS.with(std::cell::Cell::get)
}

#[cfg(test)]
#[path = "menu_a11y_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "menu_a11y_guard_tests.rs"]
mod guard_tests;
