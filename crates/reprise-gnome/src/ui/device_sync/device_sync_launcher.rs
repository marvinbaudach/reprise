//! Entry point for opening compact Android synchronization from the main menu.

use std::rc::Rc;

use gtk4::gio;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;

use super::device_sync_dialog;
use super::device_sync_runtime::{DeviceSyncRuntime, DeviceView};

pub(in crate::ui) fn present(parent: &impl IsA<gtk4::Widget>, runtime: &Rc<DeviceSyncRuntime>) {
    let devices = runtime
        .devices()
        .into_iter()
        .filter(|device| device.connected)
        .collect::<Vec<_>>();
    match devices.as_slice() {
        [] => present_no_device(parent),
        [device] => {
            if device_sync_dialog::present(parent, &device.id, runtime).is_none() {
                tracing::warn!(device_id = device.id, "could not open Android sync dialog");
            }
        }
        _ => present_chooser(parent, runtime, &devices),
    }
}

fn present_no_device(parent: &impl IsA<gtk4::Widget>) {
    let dialog = adw::AlertDialog::builder()
        .heading("No Android device connected")
        .body("Connect an unlocked Android device over USB to synchronize playlists.")
        .close_response("close")
        .build();
    dialog.add_response("close", "Close");
    dialog.choose(Some(parent), gio::Cancellable::NONE, |_| {});
}

fn present_chooser(
    parent: &impl IsA<gtk4::Widget>,
    runtime: &Rc<DeviceSyncRuntime>,
    devices: &[DeviceView],
) {
    let list = gtk4::ListBox::new();
    list.add_css_class("boxed-list");
    list.set_selection_mode(gtk4::SelectionMode::None);
    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    content.set_margin_top(18);
    content.set_margin_bottom(18);
    content.set_margin_start(18);
    content.set_margin_end(18);
    let title = gtk4::Label::new(Some("Choose an Android device"));
    title.add_css_class("title-2");
    title.set_xalign(0.0);
    content.append(&title);
    content.append(&list);
    let dialog = adw::Dialog::builder()
        .child(&content)
        .title("Android playlist sync")
        .content_width(420)
        .build();
    let parent = parent.clone().upcast::<gtk4::Widget>();
    let mut initial_focus = None;
    for device in devices {
        let row = adw::ActionRow::builder()
            .title(&device.name)
            .subtitle("MTP · connected")
            .activatable(true)
            .build();
        let icon = gtk4::Image::from_gicon(&device.icon);
        icon.set_pixel_size(32);
        row.add_prefix(&icon);
        row.add_suffix(&gtk4::Image::from_icon_name("go-next-symbolic"));
        if initial_focus.is_none() {
            initial_focus = Some(row.clone().upcast::<gtk4::Widget>());
        }
        let chooser = dialog.clone();
        let parent = parent.clone();
        let runtime = runtime.clone();
        let device_id = device.id.clone();
        row.connect_activated(move |_| {
            chooser.force_close();
            let parent = parent.clone();
            let runtime = runtime.clone();
            let device_id = device_id.clone();
            gtk4::glib::idle_add_local_once(move || {
                if device_sync_dialog::present(&parent, &device_id, &runtime).is_none() {
                    tracing::warn!(device_id, "could not open Android sync dialog");
                }
            });
        });
        list.append(&row);
    }
    let focus = initial_focus.unwrap_or_else(|| list.clone().upcast());
    let focus_guard = crate::ui::transient_focus::TransientFocusGuard::capture(&parent);
    focus_guard.bind_closable_dialog(&dialog, &focus);
    dialog.present(Some(&parent));
}
