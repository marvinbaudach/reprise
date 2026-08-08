use std::rc::Rc;

use reprise_core::db::Db;
use reprise_core::device_sync::settings::save_settings;
use reprise_core::device_sync::{DeviceSelection, DeviceSettings, SyncTargetKind};

/// `MTP-30`: seeds a device-settings row with the switch off and no
/// playlist selection, for tests that set up their own podcast/YouTube work
/// directly via SQL and then drive `sync_now` manually — without this, the
/// default-on switch (`DEFAULT 1`, schema v44) would start a sync on
/// connect before the test's own `sync_now` call runs, doubling every copy.
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
            prepare_before_sync: true,
        },
    )
    .unwrap();
}

/// Activates one per-device target for a test that deliberately exercises
/// podcast or YouTube synchronization. This is independent of the runtime
/// fixture's global module switches: a newly seen device keeps both
/// extra-source targets off until that device explicitly opts in.
pub(super) fn enable_device_target(conn: &Rc<Db>, device_id: &str, kind: SyncTargetKind) {
    let mut target = reprise_core::device_sync::load_or_create_targets(conn, device_id)
        .unwrap()
        .into_iter()
        .find(|target| target.kind == kind)
        .unwrap_or_else(|| panic!("missing {kind:?} target for {device_id}"));
    assert!(
        !target.enabled,
        "the extra-source fixture must begin at the new-device default"
    );
    target.enabled = true;
    reprise_core::device_sync::save_target(conn, device_id, &target).unwrap();
}
