//! The single device rename prompt shared by the sidebar card and page hero.

use std::rc::Rc;

use gtk4::prelude::*;

use super::device_sync_runtime::DeviceSyncRuntime;
use super::device_sync_strings;

pub(in crate::ui) fn prompt(
    parent: &impl IsA<gtk4::Widget>,
    runtime: &Rc<DeviceSyncRuntime>,
    device_id: &str,
) {
    let runtime = runtime.clone();
    let device_id = device_id.to_string();
    crate::ui::dialogs::prompt_optional_name(
        parent,
        &device_sync_strings::text(device_sync_strings::RENAME_DEVICE),
        &device_sync_strings::text(device_sync_strings::LOCAL_DEVICE_NAME),
        &device_sync_strings::text(device_sync_strings::RENAME),
        move |name| {
            if let Err(error) = runtime.rename_remembered_device(&device_id, &name) {
                tracing::warn!(device_id, %error, "could not rename remembered device");
            }
        },
    );
}
