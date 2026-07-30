//! Main-menu entry point for the Android synchronization page.

use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;

use super::device_sync_runtime::DeviceSyncRuntime;

pub(in crate::ui) type OpenDevice = Rc<dyn Fn(String, String)>;

pub(in crate::ui) fn present(
    parent: &impl IsA<gtk4::Widget>,
    runtime: &Rc<DeviceSyncRuntime>,
    open_device: &OpenDevice,
) {
    let device = runtime
        .devices()
        .into_iter()
        .find(|device| device.connected && device.session_state.opens_session());
    match device {
        None => present_no_device(parent),
        Some(device) => open_device(device.id, device.name),
    }
}

fn present_no_device(parent: &impl IsA<gtk4::Widget>) {
    let dialog = adw::AlertDialog::builder()
        .heading("No Android device connected")
        .body("Connect an unlocked Android device over USB to synchronize playlists.")
        .close_response("close")
        .build();
    dialog.add_response("close", "Close");
    dialog.choose(Some(parent), gtk4::gio::Cancellable::NONE, |_| {});
}
