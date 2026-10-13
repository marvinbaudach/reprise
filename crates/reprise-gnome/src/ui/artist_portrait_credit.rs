//! The credit a Wikimedia Commons portrait carries on the desktop (ART-5).
//!
//! Every surface that shows a Commons portrait large gets the credit as its
//! tooltip and accessible description ([`apply_credit`]). Opening the file
//! page is a menu item, and it lives where the surface's own interaction does:
//!
//! - The Releases column sits inside a table whose rows already answer the
//!   context menu in the capture phase, so its "Open image source" item joins
//!   that row menu. [`CreditLinks`] is how the menu learns, without any I/O,
//!   which artists have a credited portrait on screen.
//! - The Stats artist card is a free-standing widget, so it owns a small menu
//!   of its own ([`apply_credit_with_menu`]), wired only while a credit is set.
//!
//! Credit URLs come validated from Core, and are checked again here before
//! anything opens them: `https`, the Commons host, no user info.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{gio, glib};
use reprise_core::artist_portrait::PortraitCredit;

use super::artist_portrait_tiles::PortraitCreditResolver;

const CREDIT_URL_CLASS: &str = "reprise-artist-portrait-credit-url";
const CREDIT_ACTION_GROUP: &str = "artist-portrait-credit";
const CREDIT_ACTION_OPEN: &str = "open-source";
const CREDIT_KEY_SHORTCUTS: &str = "Menu Shift+F10";
const MENU_CONTROLLER_NAME: &str = "reprise-artist-portrait-credit-menu";
const COMMONS_HOST: &str = "commons.wikimedia.org";

/// Artist name to the validated file-page URL of the credited portrait shown
/// for them. Shared by every tile of one view, filled when a portrait is
/// applied, and read by the row menu at the moment it opens.
#[derive(Clone, Default)]
pub(in crate::ui) struct CreditLinks(Rc<RefCell<HashMap<String, String>>>);

impl CreditLinks {
    /// Records the link of a freshly applied portrait; a portrait without a
    /// usable credit (Deezer, or a URL that fails validation) forgets it.
    pub(in crate::ui) fn remember(&self, artist: &str, credit: Option<&PortraitCredit>) {
        let url = credit.and_then(|credit| validated_credit_url(&credit.file_page_url));
        let mut links = self.0.borrow_mut();
        match url {
            Some(url) => {
                links.insert(artist.to_owned(), url);
            }
            None => {
                links.remove(artist);
            }
        }
    }

    pub(in crate::ui) fn url_for(&self, artist: &str) -> Option<String> {
        self.0.borrow().get(artist).cloned()
    }
}

/// Sets the credit as tooltip and accessible description, or clears both.
pub(in crate::ui) fn apply_credit(root: &gtk4::Overlay, credit: Option<&PortraitCredit>) {
    match credit {
        Some(credit) => {
            let text = crate::ui::strings::artist_portrait_credit(&credit.author, &credit.license);
            root.set_tooltip_text(Some(&text));
            root.update_property(&[gtk4::accessible::Property::Description(&text)]);
        }
        None => {
            root.set_tooltip_text(None);
            root.reset_property(gtk4::AccessibleProperty::Description);
        }
    }
}

/// [`apply_credit`] plus the widget's own "Open image source" menu, which
/// exists only while a usable credit is set.
pub(in crate::ui) fn apply_credit_with_menu(root: &gtk4::Overlay, credit: Option<&PortraitCredit>) {
    apply_credit(root, credit);
    match credit.and_then(|credit| validated_credit_url(&credit.file_page_url)) {
        Some(url) => {
            credit_state(root).set_text(&url);
            wire_menu(root);
        }
        None => clear_menu(root),
    }
}

pub(in crate::ui) fn load_credit(
    artist: String,
    resolver: PortraitCreditResolver,
    complete: impl FnOnce(Option<PortraitCredit>) + 'static,
) {
    glib::spawn_future_local(async move {
        let credit = gio::spawn_blocking(move || resolver(&artist))
            .await
            .ok()
            .flatten();
        complete(credit);
    });
}

/// The menu model of the single "Open image source" item, targeting the
/// action group [`install_open_action`] registers.
pub(in crate::ui) fn open_source_menu() -> gio::Menu {
    let menu = gio::Menu::new();
    menu.append(
        Some(&crate::ui::strings::text(
            crate::ui::strings::ARTIST_PORTRAIT_OPEN_SOURCE,
        )),
        Some(&format!("{CREDIT_ACTION_GROUP}.{CREDIT_ACTION_OPEN}")),
    );
    menu
}

pub(in crate::ui) fn validated_credit_url(value: &str) -> Option<String> {
    let value = value.trim();
    let uri = glib::Uri::parse(value, glib::UriFlags::NONE).ok()?;
    let is_commons = uri
        .host()
        .is_some_and(|host| host.eq_ignore_ascii_case(COMMONS_HOST));
    (uri.scheme().eq_ignore_ascii_case("https") && is_commons && uri.userinfo().is_none())
        .then(|| value.to_owned())
}

pub(in crate::ui) fn open_credit_url(value: &str) {
    let Some(url) = validated_credit_url(value) else {
        tracing::warn!("refused to open an untrusted portrait credit URL");
        return;
    };
    gtk4::UriLauncher::new(&url).launch(None::<&gtk4::Window>, gio::Cancellable::NONE, |result| {
        if let Err(error) = result {
            tracing::warn!(%error, "could not open portrait credit URL");
        }
    });
}

fn existing_state(root: &gtk4::Overlay) -> Option<gtk4::Label> {
    let mut child = root.first_child();
    while let Some(widget) = child {
        child = widget.next_sibling();
        if widget.has_css_class(CREDIT_URL_CLASS) {
            if let Ok(label) = widget.downcast::<gtk4::Label>() {
                return Some(label);
            }
        }
    }
    None
}

fn credit_state(root: &gtk4::Overlay) -> gtk4::Label {
    existing_state(root).unwrap_or_else(|| {
        let label = gtk4::Label::new(None);
        label.set_visible(false);
        label.set_can_target(false);
        label.add_css_class(CREDIT_URL_CLASS);
        root.add_overlay(&label);
        label
    })
}

fn menu_url(root: &gtk4::Overlay) -> Option<String> {
    existing_state(root).and_then(|state| validated_credit_url(&state.text()))
}

fn menu_controllers(root: &gtk4::Overlay) -> Vec<gtk4::EventController> {
    let controllers = root.observe_controllers();
    (0..controllers.n_items())
        .filter_map(|index| {
            controllers
                .item(index)?
                .downcast::<gtk4::EventController>()
                .ok()
        })
        .filter(|controller| controller.name().as_deref() == Some(MENU_CONTROLLER_NAME))
        .collect()
}

fn install_open_action(root: &gtk4::Overlay) {
    let actions = gio::SimpleActionGroup::new();
    let open = gio::SimpleAction::new(CREDIT_ACTION_OPEN, None);
    let weak = root.downgrade();
    open.connect_activate(move |_, _| {
        if let Some(url) = weak.upgrade().as_ref().and_then(menu_url) {
            open_credit_url(&url);
        }
    });
    actions.add_action(&open);
    root.insert_action_group(CREDIT_ACTION_GROUP, Some(&actions));
}

fn wire_menu(root: &gtk4::Overlay) {
    if !menu_controllers(root).is_empty() {
        return;
    }
    root.update_property(&[gtk4::accessible::Property::KeyShortcuts(
        CREDIT_KEY_SHORTCUTS,
    )]);
    install_open_action(root);

    // input-parity: ACC-8 keyboard=menu-shift-f10
    let gesture = gtk4::GestureClick::new();
    gesture.set_button(gtk4::gdk::BUTTON_SECONDARY);
    gesture.set_name(Some(MENU_CONTROLLER_NAME));
    let pointer_root = root.downgrade();
    gesture.connect_pressed(move |gesture, _, x, y| {
        let Some(root) = pointer_root.upgrade() else {
            return;
        };
        if menu_url(&root).is_none() {
            return;
        }
        gesture.set_state(gtk4::EventSequenceState::Claimed);
        show_menu(&root, x as i32, y as i32);
    });
    root.add_controller(gesture);

    // The capture phase lets the shortcut arrive from whichever child holds
    // the focus; the card itself is not focusable.
    let keys = crate::ui::source_context_surface::context_keys();
    keys.set_name(Some(MENU_CONTROLLER_NAME));
    let key_root = root.downgrade();
    keys.connect_key_pressed(move |_, key, _, modifiers| {
        if !crate::ui::source_context_surface::is_context_menu_shortcut(key, modifiers) {
            return glib::Propagation::Proceed;
        }
        let Some(root) = key_root.upgrade() else {
            return glib::Propagation::Proceed;
        };
        if menu_url(&root).is_none() {
            return glib::Propagation::Proceed;
        }
        show_menu(&root, root.width() / 2, root.height() / 2);
        glib::Propagation::Stop
    });
    root.add_controller(keys);
}

fn clear_menu(root: &gtk4::Overlay) {
    if let Some(state) = existing_state(root) {
        state.set_text("");
    }
    let controllers = menu_controllers(root);
    if controllers.is_empty() {
        return;
    }
    for controller in &controllers {
        root.remove_controller(controller);
    }
    root.reset_property(gtk4::AccessibleProperty::KeyShortcuts);
    root.insert_action_group(CREDIT_ACTION_GROUP, None::<&gio::SimpleActionGroup>);
}

fn show_menu(root: &gtk4::Overlay, x: i32, y: i32) {
    let popover = crate::ui::menu_a11y::popover_menu_from_model(&open_source_menu());
    popover.set_parent(root);
    popover.set_has_arrow(false);
    popover.set_pointing_to(Some(&gtk4::gdk::Rectangle::new(x, y, 1, 1)));
    crate::ui::popover_lifecycle::unparent_after_actions(popover.upcast_ref());
    popover.popup();
}

#[cfg(test)]
#[path = "artist_portrait_credit_tests.rs"]
mod tests;
