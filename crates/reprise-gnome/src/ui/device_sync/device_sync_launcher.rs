//! Main-menu entry point for the Android synchronization page.

use std::rc::Rc;

use gtk4::gio;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;

use super::device_sync_runtime::{DeviceSyncRuntime, DeviceView};

pub(in crate::ui) type OpenDevice = Rc<dyn Fn(String, String)>;

pub(in crate::ui) fn present(
    parent: &impl IsA<gtk4::Widget>,
    runtime: &Rc<DeviceSyncRuntime>,
    open_device: &OpenDevice,
) {
    let devices = runtime
        .devices()
        .into_iter()
        .filter(|device| device.connected)
        .collect::<Vec<_>>();
    match devices.as_slice() {
        [] => present_no_device(parent),
        [device] => open_device(device.id.clone(), device.name.clone()),
        _ => present_chooser(parent, &devices, open_device),
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
    devices: &[DeviceView],
    open_device: &OpenDevice,
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
            .use_markup(false)
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
        let open_device = open_device.clone();
        let device_id = device.id.clone();
        let device_name = device.name.clone();
        row.connect_activated(move |_| {
            chooser.force_close();
            let open_device = open_device.clone();
            let device_id = device_id.clone();
            let device_name = device_name.clone();
            gtk4::glib::idle_add_local_once(move || {
                open_device(device_id, device_name);
            });
        });
        list.append(&row);
    }
    let focus = initial_focus.unwrap_or_else(|| list.clone().upcast());
    let focus_guard = crate::ui::transient_focus::TransientFocusGuard::capture(&parent);
    focus_guard.bind_closable_dialog(&dialog, &focus);
    dialog.present(Some(&parent));
}
