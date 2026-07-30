use gtk4::prelude::Cast;
use reprise_core::db::Db;
use reprise_core::device_sync::settings::{
    list_remembered_devices, load_or_create_settings, rekey_legacy_device, LegacyDeviceRekey,
};
use reprise_core::device_sync::{
    load_or_create_targets, DeviceSettings, SyncTarget, SyncTargetKind,
};
use reprise_platform_linux::device_sync::DeviceDescriptor;

pub(super) struct RememberedDeviceMemory {
    pub(super) descriptor: DeviceDescriptor,
    pub(super) settings: DeviceSettings,
    pub(super) targets: [SyncTarget; 3],
    pub(super) last_verified_at: Option<i64>,
    pub(super) size_on_device_bytes: Option<u64>,
}

pub(super) fn load_remembered_device_memories(
    db: &Db,
) -> Result<Vec<RememberedDeviceMemory>, String> {
    list_remembered_devices(db)
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(|remembered| {
            let settings =
                load_or_create_settings(db, &remembered.stable_id, &remembered.local_name)
                    .map_err(|error| error.to_string())?;
            let targets = load_or_create_targets(db, &remembered.stable_id)
                .map_err(|error| error.to_string())?;
            Ok(RememberedDeviceMemory {
                descriptor: DeviceDescriptor {
                    id: remembered.stable_id.clone(),
                    persistent_id: Some(remembered.stable_id),
                    name: remembered.local_name,
                    root_uri: String::new(),
                    reconnectable: true,
                    icon: gtk4::gio::ThemedIcon::new("phone-symbolic").upcast(),
                },
                settings,
                targets,
                last_verified_at: remembered.last_verified_at,
                size_on_device_bytes: remembered.size_on_device_bytes,
            })
        })
        .collect()
}

pub(super) fn load_device_memory(
    db: &Db,
    descriptor: &DeviceDescriptor,
) -> Result<(DeviceSettings, [SyncTarget; 3]), String> {
    let Some(stable_id) = descriptor.persistent_id.as_deref() else {
        return Ok((
            DeviceSettings::transient(&descriptor.id, &descriptor.name),
            SyncTargetKind::ALL.map(SyncTarget::default_for),
        ));
    };
    if descriptor.root_uri.starts_with("mtp://") {
        match rekey_legacy_device(db, &descriptor.root_uri, stable_id) {
            Ok(LegacyDeviceRekey::StableKeyAlreadyExists) => {
                tracing::debug!(
                    stable_id,
                    "kept legacy MTP settings because the stable device key already exists"
                );
            }
            Ok(_) => {}
            Err(error) => {
                tracing::warn!(
                    stable_id,
                    %error,
                    "could not re-key legacy MTP settings; the old row was kept"
                );
            }
        }
    }
    let settings = load_or_create_settings(db, stable_id, &descriptor.name)
        .map_err(|error| error.to_string())?;
    let targets = load_or_create_targets(db, stable_id).map_err(|error| error.to_string())?;
    Ok((settings, targets))
}
