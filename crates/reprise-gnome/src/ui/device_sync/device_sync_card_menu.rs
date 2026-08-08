//! Device-card local-memory actions shared by pointer and keyboard menus.

use std::rc::Rc;

use gtk4::gio;
use gtk4::prelude::*;

use crate::ui::device_sync_runtime::DeviceSyncRuntime;
use crate::ui::device_sync_strings;

const ACTION_GROUP: &str = "remembered-device";
const ACTION_RENAME: &str = "rename";
const ACTION_FORGET: &str = "forget";

pub(in crate::ui::sidebar) fn wire(
    root: &gtk4::Button,
    runtime: &Rc<DeviceSyncRuntime>,
    device_id: &str,
) {
    root.update_property(&[gtk4::accessible::Property::KeyShortcuts("Menu Shift+F10")]);

    // input-parity: ACC-8 keyboard=menu-shift-f10
    let gesture = gtk4::GestureClick::new();
    gesture.set_button(gtk4::gdk::BUTTON_SECONDARY);
    let pointer_root = root.clone();
    let pointer_runtime = runtime.clone();
    let pointer_id = device_id.to_string();
    gesture.connect_pressed(move |gesture, _, x, y| {
        gesture.set_state(gtk4::EventSequenceState::Claimed);
        show(
            &pointer_root,
            &pointer_runtime,
            &pointer_id,
            x as i32,
            y as i32,
        );
    });
    root.add_controller(gesture);

    let keys = crate::ui::source_context_surface::context_keys();
    let key_root = root.clone();
    let key_runtime = runtime.clone();
    let key_id = device_id.to_string();
    keys.connect_key_pressed(move |_, key, _, modifiers| {
        if !crate::ui::source_context_surface::is_context_menu_shortcut(key, modifiers) {
            return gtk4::glib::Propagation::Proceed;
        }
        show(
            &key_root,
            &key_runtime,
            &key_id,
            key_root.width() / 2,
            key_root.height() / 2,
        );
        gtk4::glib::Propagation::Stop
    });
    root.add_controller(keys);
}

fn show(root: &gtk4::Button, runtime: &Rc<DeviceSyncRuntime>, device_id: &str, x: i32, y: i32) {
    let Some(device) = runtime
        .devices()
        .into_iter()
        .find(|device| device.id == device_id && device.rememberable)
    else {
        return;
    };

    let actions = gio::SimpleActionGroup::new();
    let rename = gio::SimpleAction::new(ACTION_RENAME, None);
    {
        let runtime = runtime.clone();
        let root = root.clone();
        let device_id = device_id.to_string();
        rename.connect_activate(move |_, _| {
            crate::ui::device_sync::device_sync_rename::prompt(&root, &runtime, &device_id);
        });
    }
    actions.add_action(&rename);

    let menu = gio::Menu::new();
    menu.append(
        Some(&device_sync_strings::text(
            device_sync_strings::RENAME_DEVICE,
        )),
        Some(&format!("{ACTION_GROUP}.{ACTION_RENAME}")),
    );
    if device.session_state == reprise_core::device_sync::DeviceSessionState::Remembered {
        let forget = gio::SimpleAction::new(ACTION_FORGET, None);
        {
            let runtime = runtime.clone();
            let device_id = device_id.to_string();
            forget.connect_activate(move |_, _| {
                if let Err(error) = runtime.forget_remembered_device(&device_id) {
                    tracing::warn!(device_id, %error, "could not forget remembered device");
                }
            });
        }
        actions.add_action(&forget);
        menu.append(
            Some(&device_sync_strings::text(
                device_sync_strings::FORGET_DEVICE,
            )),
            Some(&format!("{ACTION_GROUP}.{ACTION_FORGET}")),
        );
    }
    root.insert_action_group(ACTION_GROUP, Some(&actions));

    let popover = gtk4::PopoverMenu::from_model(Some(&menu));
    popover.set_parent(root);
    popover.set_has_arrow(false);
    popover.set_pointing_to(Some(&gtk4::gdk::Rectangle::new(x, y, 1, 1)));
    crate::ui::popover_lifecycle::unparent_after_actions(popover.upcast_ref());
    popover.popup();
}
