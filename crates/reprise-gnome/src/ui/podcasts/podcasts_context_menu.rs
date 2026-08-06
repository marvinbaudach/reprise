//! Podcast episode context menu, including typed manual-queue actions.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::gio;
use gtk4::glib::variant::ToVariant;
use gtk4::prelude::*;
use reprise_core::podcasts::{EpisodeRow, PodcastKind, SourceGroup};

use super::podcasts_selection::{PodcastSelection, SelectMode};
use crate::ui::strings;

pub(super) const ACTION_PLAY: &str = "play";
pub(super) const ACTION_COPY_URL: &str = "copy-url";
pub(super) const ACTION_OPEN_IN_BROWSER: &str = "open-in-browser";
pub(super) const ACTION_PLAY_NEXT: &str = "play-next";
pub(super) const ACTION_ADD_TO_QUEUE: &str = "add-to-queue";
pub(super) const ACTION_PLAY_NEXT_UNAVAILABLE: &str = "play-next-unavailable";
pub(super) const ACTION_ADD_TO_QUEUE_UNAVAILABLE: &str = "add-to-queue-unavailable";
pub(super) const ACTION_TOGGLE_PLAYED: &str = "toggle-played";
pub(super) const ACTION_TOGGLE_DOWNLOAD: &str = "toggle-download";
pub(super) const ACTION_REMOVE_EPISODE: &str = "remove-episode";
pub(super) const ACTION_MARK_PLAYED_SELECTED: &str = "mark-played-selected";
pub(super) const ACTION_MARK_UNPLAYED_SELECTED: &str = "mark-unplayed-selected";
pub(super) const ACTION_DOWNLOAD_SELECTED: &str = "download-selected";
pub(super) const ACTION_DELETE_DOWNLOADS_SELECTED: &str = "delete-downloads-selected";
pub(super) const ACTION_REMOVE_SELECTED: &str = "remove-selected";
pub(super) const ACTION_UNSUBSCRIBE: &str = "unsubscribe";
pub(super) const ACTION_TOGGLE_PHONE_SYNC: &str = "toggle-phone-sync";
const ACTIONS: &[&str] = &[
    ACTION_PLAY,
    ACTION_COPY_URL,
    ACTION_OPEN_IN_BROWSER,
    ACTION_PLAY_NEXT,
    ACTION_ADD_TO_QUEUE,
    ACTION_PLAY_NEXT_UNAVAILABLE,
    ACTION_ADD_TO_QUEUE_UNAVAILABLE,
    ACTION_TOGGLE_PLAYED,
    ACTION_TOGGLE_DOWNLOAD,
    ACTION_REMOVE_EPISODE,
    ACTION_MARK_PLAYED_SELECTED,
    ACTION_MARK_UNPLAYED_SELECTED,
    ACTION_DOWNLOAD_SELECTED,
    ACTION_DELETE_DOWNLOADS_SELECTED,
    ACTION_REMOVE_SELECTED,
    ACTION_UNSUBSCRIBE,
    ACTION_TOGGLE_PHONE_SYNC,
];

#[derive(Clone, Debug, PartialEq, Eq)]
struct SelectionMenuEntry {
    label: String,
    action: &'static str,
}

fn entry(label: &'static str, action: &'static str) -> SelectionMenuEntry {
    SelectionMenuEntry {
        label: strings::text(label),
        action,
    }
}

/// The non-destructive half of a multi-selection menu. The split between this
/// and [`multi_selection_destructive_entry`] is expressed here, at the
/// definition, rather than as a slice bound at the call site: an index-based
/// split silently reassigns entries to the wrong section the moment this list
/// gains or reorders one.
fn multi_selection_primary_entries() -> Vec<SelectionMenuEntry> {
    vec![
        entry(strings::CONTEXT_MENU_PLAY_NEXT, ACTION_PLAY_NEXT),
        entry(strings::CONTEXT_MENU_ADD_TO_QUEUE, ACTION_ADD_TO_QUEUE),
        entry(strings::PODCAST_MARK_PLAYED, ACTION_MARK_PLAYED_SELECTED),
        entry(
            strings::PODCAST_MARK_UNPLAYED,
            ACTION_MARK_UNPLAYED_SELECTED,
        ),
        entry(strings::YOUTUBE_DOWNLOAD_SELECTED, ACTION_DOWNLOAD_SELECTED),
        entry(
            strings::PODCAST_DELETE_FILES,
            ACTION_DELETE_DOWNLOADS_SELECTED,
        ),
    ]
}

/// The destructive entry, kept last in its own section (CTX convention).
fn multi_selection_destructive_entry() -> SelectionMenuEntry {
    entry(strings::YOUTUBE_REMOVE_SELECTED, ACTION_REMOVE_SELECTED)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PodcastSyncDevice {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct DeviceSyncChoice {
    pub device: PodcastSyncDevice,
    pub selected: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum SyncControl {
    Hidden,
    Direct {
        device: PodcastSyncDevice,
        selected: bool,
    },
    Chooser(Vec<DeviceSyncChoice>),
}

pub(super) fn sync_control(
    devices: &[PodcastSyncDevice],
    selected_device_ids: &[String],
) -> SyncControl {
    let choices = devices
        .iter()
        .cloned()
        .map(|device| {
            let selected = selected_device_ids.contains(&device.id);
            DeviceSyncChoice { device, selected }
        })
        .collect::<Vec<_>>();
    match choices.as_slice() {
        [] => SyncControl::Hidden,
        [choice] => SyncControl::Direct {
            device: choice.device.clone(),
            selected: choice.selected,
        },
        _ => SyncControl::Chooser(choices),
    }
}

pub(super) fn build(row: &EpisodeRow) -> gio::Menu {
    build_for_selection(row, &[row.id], None)
}

pub(super) fn browser_url(row: &EpisodeRow) -> Option<&str> {
    let candidate = match row.kind {
        PodcastKind::Youtube => Some(row.audio_url.as_str()),
        PodcastKind::Rss => row.page_url.as_deref(),
    };
    candidate.filter(|url| reprise_core::external_link::is_launchable_url(url))
}

pub(super) fn build_for_selection(
    row: &EpisodeRow,
    selected_ids: &[i64],
    unavailable_episode: Option<i64>,
) -> gio::Menu {
    // A menu acts on the row it was opened on. It widens to the whole
    // selection only when that row is part of it — the same rule
    // `podcasts_dnd::drag_items` applies to a drag. Without the membership
    // test, opening the menu on an unselected row while several others are
    // selected offers "Remove" for those others, and the row under the
    // pointer is not among them.
    let target_ids = if selected_ids.len() > 1 && selected_ids.contains(&row.id) {
        selected_ids.to_vec()
    } else {
        vec![row.id]
    };
    let queue_available = !target_ids
        .iter()
        .any(|episode_id| Some(*episode_id) == unavailable_episode);
    if target_ids.len() <= 1 {
        return build_single(row, &target_ids, queue_available);
    }
    let menu = gio::Menu::new();
    let primary = gio::Menu::new();
    for mut entry in multi_selection_primary_entries() {
        if !queue_available {
            entry.action = match entry.action {
                ACTION_PLAY_NEXT => ACTION_PLAY_NEXT_UNAVAILABLE,
                ACTION_ADD_TO_QUEUE => ACTION_ADD_TO_QUEUE_UNAVAILABLE,
                other => other,
            };
        }
        append_selected(&primary, &entry.label, entry.action, &target_ids);
    }
    menu.append_section(None, &primary);
    let destructive = gio::Menu::new();
    let remove = multi_selection_destructive_entry();
    append_selected(&destructive, &remove.label, remove.action, &target_ids);
    menu.append_section(None, &destructive);
    menu
}

/// Takes the selection over when `row` sits outside it, then builds the menu
/// for whatever the selection is now. `parent` owns the popover; `at` is a
/// widget-local pointer position, or `None` to anchor it to the row centre.
pub(super) fn popup_for_row(
    parent: &impl IsA<gtk4::Widget>,
    row: &EpisodeRow,
    selection: &Rc<RefCell<PodcastSelection>>,
    unavailable_episode: Option<i64>,
    select_action: &str,
    at: Option<(f64, f64)>,
) {
    let took_over = selection.borrow_mut().take_over_for_context_menu(row.id);
    if took_over {
        let target = (row.id, SelectMode::Only.as_u8()).to_variant();
        if let Err(error) = parent.activate_action(select_action, Some(&target)) {
            tracing::debug!(%error, "podcast row menu could not publish its selection take-over");
        }
    }
    let selected_ids = selection.borrow().selected_ids();
    let popover = gtk4::PopoverMenu::from_model(Some(&build_for_selection(
        row,
        &selected_ids,
        unavailable_episode,
    )));
    popover.set_has_arrow(false);
    popover.set_parent(parent);
    let (x, y) = at.unwrap_or_else(|| {
        let parent = parent.as_ref();
        (
            f64::from(parent.width()) / 2.0,
            f64::from(parent.height()) / 2.0,
        )
    });
    popover.set_pointing_to(Some(&gtk4::gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
    crate::ui::popover_lifecycle::unparent_after_actions(popover.upcast_ref());
    popover.popup();
}

fn build_single(row: &EpisodeRow, target_ids: &[i64], queue_available: bool) -> gio::Menu {
    let menu = gio::Menu::new();
    let primary = gio::Menu::new();
    append_targeted(
        &primary,
        if row.position_ms > 0 && row.played_at.is_none() {
            strings::PODCAST_STATUS_RESUME
        } else {
            strings::PODCAST_PLAY
        },
        ACTION_PLAY,
        row.id,
    );
    append_targeted(&primary, strings::PODCAST_COPY_URL, ACTION_COPY_URL, row.id);
    append_browser_action(&primary, row);
    append_selected(
        &primary,
        &strings::text(strings::CONTEXT_MENU_PLAY_NEXT),
        if queue_available {
            ACTION_PLAY_NEXT
        } else {
            ACTION_PLAY_NEXT_UNAVAILABLE
        },
        target_ids,
    );
    append_selected(
        &primary,
        &strings::text(strings::CONTEXT_MENU_ADD_TO_QUEUE),
        if queue_available {
            ACTION_ADD_TO_QUEUE
        } else {
            ACTION_ADD_TO_QUEUE_UNAVAILABLE
        },
        target_ids,
    );
    append_targeted(
        &primary,
        if row.played_at.is_some() {
            strings::PODCAST_MARK_UNPLAYED
        } else {
            strings::PODCAST_MARK_PLAYED
        },
        ACTION_TOGGLE_PLAYED,
        row.id,
    );
    append_targeted(
        &primary,
        if row.downloaded_path.is_some() {
            strings::PODCAST_DELETE_DOWNLOAD
        } else {
            strings::PODCAST_DOWNLOAD
        },
        ACTION_TOGGLE_DOWNLOAD,
        row.id,
    );
    menu.append_section(None, &primary);

    let destructive = gio::Menu::new();
    append_targeted(
        &destructive,
        strings::PODCAST_REMOVE_EPISODE,
        ACTION_REMOVE_EPISODE,
        row.id,
    );
    append_targeted(
        &destructive,
        &strings::podcast_unsubscribe_from(&row.show),
        ACTION_UNSUBSCRIBE,
        row.subscription_id,
    );
    menu.append_section(None, &destructive);
    menu
}

fn append_browser_action(menu: &gio::Menu, row: &EpisodeRow) {
    if browser_url(row).is_some() {
        append_targeted(
            menu,
            strings::PODCAST_OPEN_IN_BROWSER,
            ACTION_OPEN_IN_BROWSER,
            row.id,
        );
    }
}

pub(super) fn install_disabled_queue_actions(group: &gio::SimpleActionGroup) {
    for name in [
        ACTION_PLAY_NEXT_UNAVAILABLE,
        ACTION_ADD_TO_QUEUE_UNAVAILABLE,
    ] {
        let action = gio::SimpleAction::new(name, Some(&Vec::<i64>::static_variant_type()));
        action.set_enabled(false);
        group.add_action(&action);
    }
}

/// Builds the source-level context menu, including the phone-sync section.
/// RSS and YouTube sources get the same sync section (`POD-12`) — each
/// lands on its own device target folder (`MTP-38`), a routing decision
/// made downstream in `device_sync::podcasts`, not here.
pub(super) fn build_source(
    group: &SourceGroup,
    devices: &[PodcastSyncDevice],
    selected_device_ids: &[String],
) -> gio::Menu {
    let menu = gio::Menu::new();
    if group.kind == PodcastKind::Youtube {
        let channel = gio::Menu::new();
        append_targeted(
            &channel,
            strings::YOUTUBE_OPEN_CHANNEL,
            "open-channel",
            group.subscription_id,
        );
        menu.append_section(None, &channel);
    }
    match sync_control(devices, selected_device_ids) {
        SyncControl::Hidden => {}
        SyncControl::Direct { device, selected } => append_device_targeted(
            &menu,
            group.subscription_id,
            &DeviceSyncChoice { device, selected },
        ),
        SyncControl::Chooser(choices) => {
            let choices_menu = gio::Menu::new();
            for choice in choices {
                append_device_targeted(&choices_menu, group.subscription_id, &choice);
            }
            menu.append_submenu(
                Some(&strings::text(strings::PODCAST_SYNC_DEVICES)),
                &choices_menu,
            );
        }
    }
    append_targeted(
        &menu,
        &strings::podcast_unsubscribe_from(&group.title),
        ACTION_UNSUBSCRIBE,
        group.subscription_id,
    );
    menu
}

fn append_device_targeted(menu: &gio::Menu, subscription_id: i64, choice: &DeviceSyncChoice) {
    let label = if choice.selected {
        strings::podcast_stop_sync_device(&choice.device.name)
    } else {
        strings::podcast_sync_device(&choice.device.name)
    };
    let item = gio::MenuItem::new(Some(&label), None);
    item.set_action_and_target_value(
        Some(&format!("podcasts.{ACTION_TOGGLE_PHONE_SYNC}")),
        Some(&(subscription_id, choice.device.id.clone()).to_variant()),
    );
    menu.append_item(&item);
}

fn append_targeted(menu: &gio::Menu, label: &str, action: &str, id: i64) {
    let item = gio::MenuItem::new(Some(&strings::text(label)), None);
    item.set_action_and_target_value(Some(&format!("podcasts.{action}")), Some(&id.to_variant()));
    menu.append_item(&item);
}

fn append_selected(menu: &gio::Menu, label: &str, action: &str, episode_ids: &[i64]) {
    let item = gio::MenuItem::new(Some(label), None);
    item.set_action_and_target_value(
        Some(&format!("podcasts.{action}")),
        Some(&episode_ids.to_variant()),
    );
    menu.append_item(&item);
}

pub(super) fn wire_gesture(widget: &impl IsA<gtk4::Widget>, item: &gtk4::ListItem) {
    // input-parity: ACC-8 keyboard=menu-shift-f10
    let gesture = crate::ui::source_context_surface::secondary_click();
    let pointer_item = item.clone();
    gesture.connect_pressed(move |gesture, _, x, y| {
        let Some(parent) = gesture.widget() else {
            return;
        };
        gesture.set_state(gtk4::EventSequenceState::Claimed);
        popup(&pointer_item, &parent, x as i32, y as i32);
    });
    widget.upcast_ref::<gtk4::Widget>().add_controller(gesture);
}

pub(super) fn wire_keyboard(view: &gtk4::ColumnView, selection: &gtk4::SingleSelection) {
    let keys = crate::ui::source_context_surface::context_keys();
    let parent = view.clone();
    let selection = selection.clone();
    keys.connect_key_pressed(move |_, key, _, modifiers| {
        if !crate::ui::source_context_surface::is_context_menu_shortcut(key, modifiers) {
            return gtk4::glib::Propagation::Proceed;
        }
        let Some(object) = selection
            .selected_item()
            .and_downcast::<super::podcasts_model::PodcastEpisodeObject>()
        else {
            return gtk4::glib::Propagation::Proceed;
        };
        // No pointer position to anchor to, so the menu opens over the middle
        // of the table — the same place the radio table uses.
        popup_at(
            &object.row(),
            parent.upcast_ref(),
            parent.width() / 2,
            parent.height() / 2,
        );
        gtk4::glib::Propagation::Stop
    });
    view.add_controller(keys);
}

fn popup(item: &gtk4::ListItem, parent: &gtk4::Widget, x: i32, y: i32) {
    let Some(object) = item
        .item()
        .and_downcast::<super::podcasts_model::PodcastEpisodeObject>()
    else {
        return;
    };
    popup_at(&object.row(), parent, x, y);
}

/// The one place a podcast row menu is built and shown, shared by the pointer
/// and keyboard paths so they cannot drift apart (ACC-1).
fn popup_at(row: &EpisodeRow, parent: &gtk4::Widget, x: i32, y: i32) {
    let popover = gtk4::PopoverMenu::from_model(Some(&build(row)));
    popover.set_has_arrow(false);
    popover.set_parent(parent);
    popover.set_pointing_to(Some(&gtk4::gdk::Rectangle::new(x, y, 1, 1)));
    crate::ui::popover_lifecycle::unparent_after_actions(popover.upcast_ref());
    popover.popup();
}

#[cfg(test)]
#[path = "podcasts_context_menu_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "podcasts_source_menu_tests.rs"]
mod source_menu_tests;

#[cfg(test)]
#[path = "podcasts_context_menu_browser_tests.rs"]
mod browser_tests;
