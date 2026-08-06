//! Shared pointer, keyboard, and discovery routes for source context menus.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use reprise_core::podcasts::{EpisodeRow, SourceGroup};

use super::podcasts_context_menu::{self, PodcastSyncDevice};
use super::podcasts_episode_files::EpisodePaths;
use super::podcasts_selection::PodcastSelection;
use crate::ui::strings;

pub(super) fn wire_episode_row(
    widget: &impl IsA<gtk4::Widget>,
    row: &EpisodeRow,
    selection: &Rc<RefCell<PodcastSelection>>,
    paths: &Rc<EpisodePaths>,
    unavailable_episode: Option<i64>,
    select_action: &'static str,
) {
    // input-parity: ACC-8 keyboard=menu-shift-f10
    let gesture = crate::ui::source_context_surface::secondary_click();
    let pointer_row = row.clone();
    let pointer_selection = selection.clone();
    let pointer_paths = paths.clone();
    gesture.connect_pressed(move |gesture, _, x, y| {
        let Some(parent) = gesture.widget() else {
            return;
        };
        gesture.set_state(gtk4::EventSequenceState::Claimed);
        podcasts_context_menu::popup_for_row(
            &parent,
            &pointer_row,
            &pointer_selection,
            &pointer_paths,
            unavailable_episode,
            select_action,
            Some((x, y)),
        );
    });
    widget.as_ref().add_controller(gesture);

    let keys = crate::ui::source_context_surface::context_keys();
    let keyed_parent = widget.as_ref().downgrade();
    let keyed_row = row.clone();
    let keyed_selection = selection.clone();
    let keyed_paths = paths.clone();
    keys.connect_key_pressed(move |_, key, _, modifiers| {
        if !crate::ui::source_context_surface::is_context_menu_shortcut(key, modifiers) {
            return gtk4::glib::Propagation::Proceed;
        }
        let Some(parent) = keyed_parent.upgrade() else {
            return gtk4::glib::Propagation::Proceed;
        };
        podcasts_context_menu::popup_for_row(
            &parent,
            &keyed_row,
            &keyed_selection,
            &keyed_paths,
            unavailable_episode,
            select_action,
            None,
        );
        gtk4::glib::Propagation::Stop
    });
    widget.as_ref().add_controller(keys);
}

pub(super) fn episode_menu_button(
    row: &EpisodeRow,
    selection: &Rc<RefCell<PodcastSelection>>,
    paths: &Rc<EpisodePaths>,
    unavailable_episode: Option<i64>,
    select_action: &'static str,
) -> gtk4::MenuButton {
    let menu = gtk4::MenuButton::builder()
        .icon_name("view-more-symbolic")
        .build();
    let menu_row = row.clone();
    let menu_selection = selection.clone();
    let menu_paths = paths.clone();
    menu.set_create_popup_func(move |menu| {
        let at = Some((
            f64::from(menu.width()) / 2.0,
            f64::from(menu.height()) / 2.0,
        ));
        podcasts_context_menu::popup_for_row(
            menu,
            &menu_row,
            &menu_selection,
            &menu_paths,
            unavailable_episode,
            select_action,
            at,
        );
    });
    menu.add_css_class("flat");
    menu.set_tooltip_text(Some(&strings::text(strings::PODCAST_MORE_OPTIONS)));
    menu
}

pub(super) fn wire_source_header(
    header: &impl IsA<gtk4::Widget>,
    group: &SourceGroup,
    devices: &[PodcastSyncDevice],
    selected_device_ids: &[String],
) {
    // input-parity: ACC-8 keyboard=source-menu-button
    let gesture = crate::ui::source_context_surface::secondary_click();
    let group = group.clone();
    let devices = devices.to_vec();
    let selected_device_ids = selected_device_ids.to_vec();
    gesture.connect_pressed(move |gesture, _, x, y| {
        let Some(parent) = gesture.widget() else {
            return;
        };
        gesture.set_state(gtk4::EventSequenceState::Claimed);
        let popover = gtk4::PopoverMenu::from_model(Some(&podcasts_context_menu::build_source(
            &group,
            &devices,
            &selected_device_ids,
        )));
        popover.set_has_arrow(false);
        popover.set_parent(&parent);
        popover.set_pointing_to(Some(&gtk4::gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
        crate::ui::popover_lifecycle::unparent_after_actions(popover.upcast_ref());
        popover.popup();
    });
    header.as_ref().add_controller(gesture);
}
