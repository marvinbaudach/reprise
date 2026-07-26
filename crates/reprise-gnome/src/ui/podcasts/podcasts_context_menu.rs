//! Podcast episode context menu. External media intentionally has no queue actions.

use gtk4::gio;
use gtk4::glib::variant::ToVariant;
use gtk4::prelude::*;
use reprise_core::podcasts::EpisodeRow;

use crate::ui::strings;

pub(super) const ACTION_PLAY: &str = "play";
pub(super) const ACTION_COPY_URL: &str = "copy-url";
pub(super) const ACTION_TOGGLE_PLAYED: &str = "toggle-played";
pub(super) const ACTION_TOGGLE_DOWNLOAD: &str = "toggle-download";
pub(super) const ACTION_REMOVE_EPISODE: &str = "remove-episode";
pub(super) const ACTION_UNSUBSCRIBE: &str = "unsubscribe";
const ACTIONS: &[&str] = &[
    ACTION_PLAY,
    ACTION_COPY_URL,
    ACTION_TOGGLE_PLAYED,
    ACTION_TOGGLE_DOWNLOAD,
    ACTION_REMOVE_EPISODE,
    ACTION_UNSUBSCRIBE,
];

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
}
