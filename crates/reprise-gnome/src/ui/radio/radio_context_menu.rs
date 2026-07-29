use gtk4::gio;
use gtk4::glib::variant::ToVariant;
use gtk4::prelude::*;
use reprise_core::connectivity::Connectivity;
use reprise_core::radio::StationRow;

use crate::ui::strings;

pub(super) const ACTION_PLAY: &str = "play";
pub(super) const ACTION_COPY_URL: &str = "copy-url";
pub(super) const ACTION_EDIT: &str = "edit";
pub(super) const ACTION_REMOVE: &str = "remove";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum StationAction {
    Play,
    Stop,
    CopyUrl,
    Edit,
    Remove,
}

impl StationAction {
    pub(super) const fn is_queue_action(&self) -> bool {
        false
    }
}

pub(super) fn station_actions(playing: bool) -> Vec<StationAction> {
    vec![
        if playing {
            StationAction::Stop
        } else {
            StationAction::Play
        },
        StationAction::CopyUrl,
        StationAction::Edit,
        StationAction::Remove,
    ]
}

/// `NET-3b`: Radio is the one exception to "queue it" — a live stream
/// cannot be deferred. A station that is not currently playing shows the
/// normal "Play" label while online, but while offline it reads "No
/// connection · Retry" instead: nothing is queued, the label itself is the
/// retry affordance. An already-playing station keeps its "Stop" label
/// regardless of connectivity — stopping never needs the network.
pub(super) fn play_menu_label(connectivity: Connectivity, playing: bool) -> &'static str {
    if playing {
        strings::RADIO_STOP
    } else if connectivity.is_offline() {
        strings::RADIO_NO_CONNECTION_RETRY
    } else {
        strings::RADIO_PLAY
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RemovalStage {
    Visible,
    Tombstoned,
    Purged,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RemovalEvent {
    Remove,
    Undo,
    ToastDismissed,
}

pub(super) fn removal_transition(stage: RemovalStage, event: RemovalEvent) -> RemovalStage {
    match (stage, event) {
        (RemovalStage::Visible, RemovalEvent::Remove) => RemovalStage::Tombstoned,
        (RemovalStage::Tombstoned, RemovalEvent::Undo) => RemovalStage::Visible,
        (RemovalStage::Tombstoned, RemovalEvent::ToastDismissed) => RemovalStage::Purged,
        _ => stage,
    }
}

pub(super) fn build(row: &StationRow, playing: bool, connectivity: Connectivity) -> gio::Menu {
    let menu = gio::Menu::new();
    let primary = gio::Menu::new();
    append_targeted(
        &primary,
        play_menu_label(connectivity, playing),
        ACTION_PLAY,
        row.id,
    );
    append_targeted(&primary, strings::RADIO_COPY_URL, ACTION_COPY_URL, row.id);
    append_targeted(&primary, strings::RADIO_EDIT, ACTION_EDIT, row.id);
    menu.append_section(None, &primary);
    let destructive = gio::Menu::new();
    append_targeted(
        &destructive,
        strings::RADIO_REMOVE_FAVORITE,
        ACTION_REMOVE,
        row.id,
    );
    menu.append_section(None, &destructive);
    menu
}

fn append_targeted(menu: &gio::Menu, label: &str, action: &str, id: i64) {
    let item = gio::MenuItem::new(Some(&strings::text(label)), None);
    item.set_action_and_target_value(Some(&format!("radio.{action}")), Some(&id.to_variant()));
    menu.append_item(&item);
}

pub(super) fn wire_gesture(
    widget: &impl IsA<gtk4::Widget>,
    item: &gtk4::ListItem,
    is_playing: impl Fn(i64) -> bool + 'static,
    connectivity: impl Fn() -> Connectivity + 'static,
) {
    // input-parity: ACC-8 keyboard=radio-context-menu-shift-f10
    let gesture = crate::ui::source_context_surface::secondary_click();
    let item = item.clone();
    gesture.connect_pressed(move |gesture, _, x, y| {
        let Some(object) = item
            .item()
            .and_downcast::<super::radio_model::RadioObject>()
        else {
            return;
        };
        let row = object.row();
        let Some(parent) = gesture.widget() else {
            return;
        };
        gesture.set_state(gtk4::EventSequenceState::Claimed);
        let popover =
            gtk4::PopoverMenu::from_model(Some(&build(&row, is_playing(row.id), connectivity())));
        popover.set_has_arrow(false);
        popover.set_parent(&parent);
        popover.set_pointing_to(Some(&gtk4::gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
        crate::ui::popover_lifecycle::unparent_after_actions(popover.upcast_ref());
        popover.popup();
    });
    widget.upcast_ref::<gtk4::Widget>().add_controller(gesture);
}

pub(super) fn wire_keyboard(
    view: &gtk4::ColumnView,
    selection: &gtk4::SingleSelection,
    is_playing: impl Fn(i64) -> bool + 'static,
    connectivity: impl Fn() -> Connectivity + 'static,
) {
    let keys = crate::ui::source_context_surface::context_keys();
    let menu_parent = view.clone();
    let selection = selection.clone();
    keys.connect_key_pressed(move |_, key, _, modifiers| {
        if !crate::ui::source_context_surface::is_context_menu_shortcut(key, modifiers) {
            return gtk4::glib::Propagation::Proceed;
        }
        let Some(object) = selection
            .selected_item()
            .and_downcast::<super::radio_model::RadioObject>()
        else {
            return gtk4::glib::Propagation::Proceed;
        };
        let row = object.row();
        let popover =
            gtk4::PopoverMenu::from_model(Some(&build(&row, is_playing(row.id), connectivity())));
        popover.set_has_arrow(false);
        popover.set_parent(&menu_parent);
        popover.set_pointing_to(Some(&gtk4::gdk::Rectangle::new(
            menu_parent.width() / 2,
            menu_parent.height() / 2,
            1,
            1,
        )));
        crate::ui::popover_lifecycle::unparent_after_actions(popover.upcast_ref());
        popover.popup();
        gtk4::glib::Propagation::Stop
    });
    view.add_controller(keys);
}

#[cfg(test)]
mod tests {
    use super::*;
    use gtk4::glib;

    #[test]
    fn station_context_menu_keeps_destructive_action_last_and_omits_queue_actions() {
        let actions = station_actions(false);
        assert_eq!(
            actions,
            [
                StationAction::Play,
                StationAction::CopyUrl,
                StationAction::Edit,
                StationAction::Remove,
            ]
        );
        assert!(!actions.iter().any(StationAction::is_queue_action));
        assert_eq!(actions.last(), Some(&StationAction::Remove));
        assert_eq!(station_actions(true)[0], StationAction::Stop);
    }

    #[test]
    fn net_3b_play_label_reads_no_connection_retry_only_when_offline_and_not_playing() {
        assert_eq!(
            play_menu_label(Connectivity::Online, false),
            strings::RADIO_PLAY
        );
        assert_eq!(
            play_menu_label(Connectivity::Offline, false),
            strings::RADIO_NO_CONNECTION_RETRY
        );
        // Already playing keeps "Stop" regardless of connectivity — that
        // path never starts a fresh network connection.
        assert_eq!(
            play_menu_label(Connectivity::Online, true),
            strings::RADIO_STOP
        );
        assert_eq!(
            play_menu_label(Connectivity::Offline, true),
            strings::RADIO_STOP
        );
    }

    #[test]
    fn net_3b_radio_context_menu_play_item_carries_the_no_connection_retry_label_offline() {
        let row = StationRow {
            id: 1,
            uuid: None,
            name: "Test Station".into(),
            stream_url: "https://example.invalid/stream".into(),
            homepage: None,
            favicon_url: None,
            genre: None,
            codec: None,
            bitrate_kbps: None,
            country_code: None,
            votes: None,
            added_at: 0,
            removed_at: None,
        };
        let menu = build(&row, false, Connectivity::Offline);
        let primary = menu
            .item_link(0, "section")
            .expect("primary section exists");
        let label = primary
            .item_attribute_value(0, "label", Some(glib::VariantTy::STRING))
            .and_then(|value| value.str().map(str::to_owned))
            .expect("play item has a label");
        assert_eq!(label, strings::text(strings::RADIO_NO_CONNECTION_RETRY));
    }

    #[test]
    fn src_4_remove_is_tombstone_until_toast_commit() {
        assert_eq!(
            removal_transition(RemovalStage::Visible, RemovalEvent::Remove),
            RemovalStage::Tombstoned
        );
        assert_eq!(
            removal_transition(RemovalStage::Tombstoned, RemovalEvent::Undo),
            RemovalStage::Visible
        );
        assert_eq!(
            removal_transition(RemovalStage::Tombstoned, RemovalEvent::ToastDismissed),
            RemovalStage::Purged
        );
    }
}
