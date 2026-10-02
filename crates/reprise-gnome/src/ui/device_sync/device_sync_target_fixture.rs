use std::rc::Rc;

use reprise_core::db::Db;
use reprise_core::device_sync::settings::save_settings;
use reprise_core::device_sync::{DeviceSelection, DeviceSettings};

/// `MTP-30`: seeds a device-settings row with the switch off and no
/// playlist selection, for tests that drive `sync_now` manually. Pinning the
/// switch off ensures an automatic start can never double the test's explicit
/// `sync_now` call.
pub(super) fn disable_auto_start(conn: &Rc<Db>, device_id: &str) {
    save_settings(
        conn,
        &DeviceSettings {
            device_serial: device_id.into(),
            device_name: format!("Phone {device_id}"),
            selection: DeviceSelection::Sources(Vec::new()),
            profile: reprise_core::device_sync::TransferProfile::default(),
            opus_bitrate: 0,
            remove_deleted: true,
            sync_automatically: false,
        },
    )
    .unwrap();
}
