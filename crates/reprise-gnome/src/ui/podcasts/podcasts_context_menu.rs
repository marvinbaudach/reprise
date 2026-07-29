//! Podcast episode context menu. External media intentionally has no queue actions.

use gtk4::gio;
use gtk4::glib::variant::ToVariant;
use gtk4::prelude::*;
use reprise_core::podcasts::{EpisodeRow, SourceGroup};

use crate::ui::strings;

pub(super) const ACTION_PLAY: &str = "play";
pub(super) const ACTION_COPY_URL: &str = "copy-url";
pub(super) const ACTION_TOGGLE_PLAYED: &str = "toggle-played";
pub(super) const ACTION_TOGGLE_DOWNLOAD: &str = "toggle-download";
pub(super) const ACTION_REMOVE_EPISODE: &str = "remove-episode";
pub(super) const ACTION_UNSUBSCRIBE: &str = "unsubscribe";
pub(super) const ACTION_TOGGLE_PHONE_SYNC: &str = "toggle-phone-sync";
const ACTIONS: &[&str] = &[
    ACTION_PLAY,
    ACTION_COPY_URL,
    ACTION_TOGGLE_PLAYED,
    ACTION_TOGGLE_DOWNLOAD,
    ACTION_REMOVE_EPISODE,
    ACTION_UNSUBSCRIBE,
    ACTION_TOGGLE_PHONE_SYNC,
];

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

pub(super) fn wire_gesture(widget: &impl IsA<gtk4::Widget>, item: &gtk4::ListItem) {
    // input-parity: ACC-8 keyboard=menu-shift-f10
    let gesture = gtk4::GestureClick::new();
    gesture.set_button(gtk4::gdk::BUTTON_SECONDARY);
    let pointer_item = item.clone();
    gesture.connect_pressed(move |gesture, _, x, y| {
        let Some(parent) = gesture.widget() else {
            return;
        };
        gesture.set_state(gtk4::EventSequenceState::Claimed);
        popup(&pointer_item, &parent, x as i32, y as i32);
    });
    widget.upcast_ref::<gtk4::Widget>().add_controller(gesture);

    let key = gtk4::EventControllerKey::new();
    let keyboard_item = item.clone();
    let parent = widget.upcast_ref::<gtk4::Widget>().clone();
    key.connect_key_pressed(move |_, key, _, modifiers| {
        let opens_menu = key == gtk4::gdk::Key::Menu
            || (key == gtk4::gdk::Key::F10
                && modifiers.contains(gtk4::gdk::ModifierType::SHIFT_MASK));
        if !opens_menu {
            return gtk4::glib::Propagation::Proceed;
        }
        popup(
            &keyboard_item,
            &parent,
            parent.width() / 2,
            parent.height() / 2,
        );
        gtk4::glib::Propagation::Stop
    });
    widget.upcast_ref::<gtk4::Widget>().add_controller(key);
}

fn popup(item: &gtk4::ListItem, parent: &gtk4::Widget, x: i32, y: i32) {
    let Some(object) = item
        .item()
        .and_downcast::<super::podcasts_model::PodcastEpisodeObject>()
    else {
        return;
    };
    let popover = gtk4::PopoverMenu::from_model(Some(&build(&object.row())));
    popover.set_has_arrow(false);
    popover.set_parent(parent);
    popover.set_pointing_to(Some(&gtk4::gdk::Rectangle::new(x, y, 1, 1)));
    popover.connect_closed(gtk4::prelude::WidgetExt::unparent);
    popover.popup();
}

#[cfg(test)]
mod tests {
    use reprise_core::podcasts::PodcastKind;

    use super::*;

    #[test]
    fn context_menu_never_exposes_queue_membership_actions() {
        assert!(ACTIONS.iter().all(|action| {
            !action.contains("queue")
                && !action.contains("play-next")
                && !action.contains("play_next")
        }));
    }

    #[test]
    fn pod_6_context_menu_exposes_individual_episode_removal() {
        assert!(ACTIONS.contains(&ACTION_REMOVE_EPISODE));
    }

    #[test]
    fn pod_12_youtube_exposes_phone_sync_action_just_like_rss() {
        let group = SourceGroup {
            subscription_id: 1,
            title: "Channel".into(),
            author: None,
            image_url: None,
            kind: PodcastKind::Youtube,
            sync_to_phone: false,
            episodes: Vec::new(),
        };
        let menu = build_source(
            &group,
            &[PodcastSyncDevice {
                id: "mtp:pixel".into(),
                name: "Pixel".into(),
            }],
            &[],
        );
        // 1 section for "Sync to <device>" plus 1 for "Unsubscribe".
        assert_eq!(menu.n_items(), 2);
    }

    #[test]
    fn pod_12_sync_control_is_hidden_without_a_connected_device() {
        assert_eq!(sync_control(&[], &[]), SyncControl::Hidden);
    }

    #[test]
    fn pod_12_one_connected_device_is_targeted_directly() {
        let devices = [PodcastSyncDevice {
            id: "mtp:pixel".into(),
            name: "Pixel".into(),
        }];

        assert_eq!(
            sync_control(&devices, &["mtp:pixel".into()]),
            SyncControl::Direct {
                device: devices[0].clone(),
                selected: true,
            }
        );
    }

    #[test]
    fn pod_12_multiple_connected_devices_offer_independent_choices() {
        let devices = [
            PodcastSyncDevice {
                id: "mtp:phone".into(),
                name: "Phone".into(),
            },
            PodcastSyncDevice {
                id: "mtp:tablet".into(),
                name: "Tablet".into(),
            },
        ];

        assert_eq!(
            sync_control(&devices, &["mtp:tablet".into()]),
            SyncControl::Chooser(vec![
                DeviceSyncChoice {
                    device: devices[0].clone(),
                    selected: false,
                },
                DeviceSyncChoice {
                    device: devices[1].clone(),
                    selected: true,
                },
            ])
        );
    }
}
