//! Application actions shared by every device-synchronization surface.

use std::rc::Rc;

use gtk4::gio;
use gtk4::gio::prelude::*;
use gtk4::glib;

use super::device_sync_runtime::{DeviceSyncRuntime, PlannedSyncPhase};

const ACTION_SYNC_DEVICE: &str = "sync-device";

pub(in crate::ui) fn install(app: &libadwaita::Application, runtime: &Rc<DeviceSyncRuntime>) {
    app.remove_action(ACTION_SYNC_DEVICE);
    let action = gio::SimpleAction::new(ACTION_SYNC_DEVICE, Some(glib::VariantTy::STRING));
    let runtime = runtime.clone();
    action.connect_activate(move |_, parameter| {
        let Some(device_id) = parameter.and_then(|value| value.str()) else {
            tracing::warn!("sync-device action needs a device serial");
            return;
        };
        let syncing = runtime.devices().iter().any(|device| {
            device.id == device_id
                && matches!(
                    device.sync_phase,
                    PlannedSyncPhase::Syncing { .. } | PlannedSyncPhase::Finishing
                )
        });
        if syncing {
            runtime.cancel_current(device_id);
        } else if let Err(error) = runtime.sync_now(device_id) {
            tracing::warn!(%error, device_id, "could not start device synchronization");
        }
    });
    app.add_action(&action);
}
